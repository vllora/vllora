use crate::error::GatewayError;
use reqwest::Client;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::model::ClientCapabilities;
use rmcp::model::ClientInfo;
use rmcp::model::Content;
use rmcp::model::GetPromptRequestParam;
use rmcp::model::GetPromptResult;
use rmcp::model::Implementation;
use rmcp::model::InitializeRequestParam;
use rmcp::model::InitializeResult;
use rmcp::model::ListPromptsResult;
use rmcp::model::ListResourceTemplatesResult;
use rmcp::model::ListResourcesResult;
use rmcp::model::PaginatedRequestParam;
use rmcp::model::ProtocolVersion;
use rmcp::model::ReadResourceRequestParam;
use rmcp::model::ReadResourceResult;
use rmcp::service::RequestContext;
use rmcp::ClientHandler;
use rmcp::ErrorData;
use rmcp::RoleServer;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;

use rmcp::{
    handler::server::router::tool::ToolRouter,
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServerHandler,
};

const TAVILY_API_URL: &str = "https://api.tavily.com/search";

#[derive(Serialize, Deserialize, Debug)]
pub struct QueryResult {
    pub query: String,
    pub follow_up_questions: Option<Vec<String>>,
    pub answer: Option<String>,
    pub images: Vec<String>,
    pub results: Vec<SearchResult>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub content: String,
    pub score: f64,
    pub raw_content: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchRequest {
    #[schemars(description = "Search query")]
    pub query: String,
}

#[derive(Clone)]
pub struct TavilySearch {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl TavilySearch {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Search the web and return results")]
    pub async fn search(
        &self,
        Parameters(SearchRequest { query }): Parameters<SearchRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let r = search_tavily(&query, &env::var("TAVILY_API_KEY").unwrap()).await;

        match r {
            Ok(r) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string(&r).unwrap(),
            )])),
            Err(e) => Err(ErrorData::internal_error(e.to_string(), None)),
        }
    }
}

impl Default for TavilySearch {
    fn default() -> Self {
        Self::new()
    }
}

async fn search_tavily(query: &str, api_key: &str) -> Result<Value, GatewayError> {
    let client = Client::new();
    let response = client
        .post(TAVILY_API_URL)
        .header("Content-Type", "application/json")
        .json(&json!({
            "api_key": api_key,
            "query": query
        }))
        .send()
        .await?
        .json::<QueryResult>()
        .await?;

    // Note: Remove unnecessary parts
    let result = json!({
      "answer": response.answer,
      "results": response.results
    });

    Ok(result)
}

#[tool_handler]
impl ServerHandler for TavilySearch {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("This server provides a counter tool that can increment and decrement values. The counter starts at 0 and can be modified using the 'increment' and 'decrement' tools. Use 'get_value' to check the current count.".to_string()),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult {
            resources: Vec::new(),
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        ReadResourceRequestParam { uri }: ReadResourceRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        Err(ErrorData::resource_not_found(
            "resource_not_found",
            Some(json!({
                "uri": uri.as_str()
            })),
        ))
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        Ok(ListPromptsResult {
            next_cursor: None,
            prompts: vec![],
        })
    }

    async fn get_prompt(
        &self,
        GetPromptRequestParam { .. }: GetPromptRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        Err(ErrorData::invalid_params("prompt not found", None))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        Ok(ListResourceTemplatesResult {
            next_cursor: None,
            resource_templates: Vec::new(),
        })
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        Ok(ServerHandler::get_info(self))
    }
}

impl ClientHandler for TavilySearch {
    fn get_info(&self) -> ClientInfo {
        ClientInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ClientCapabilities::builder().build(),
            client_info: Implementation::from_build_env(),
        }
    }
}
