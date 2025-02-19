use async_mcp::server::{Server, ServerBuilder};
use async_mcp::transport::Transport;
use async_mcp::types::{
    CallToolRequest, CallToolResponse, ListRequest, PromptsListResponse, ResourcesListResponse,
    ServerCapabilities, Tool, ToolResponseContent,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use tracing::info;

use crate::error::GatewayError;

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

pub fn build<T: Transport>(t: T) -> Result<Server<T>, GatewayError> {
    let mut server = Server::builder(t)
        .capabilities(ServerCapabilities {
            tools: Some(json!({})),
            ..Default::default()
        })
        .request_handler("resources/list", |_req: ListRequest| {
            Box::pin(async move {
                Ok(ResourcesListResponse {
                    resources: Vec::new(),
                    next_cursor: None,
                    meta: None,
                })
            })
        })
        .request_handler("prompts/list", |_req: ListRequest| {
            Box::pin(async move {
                Ok(PromptsListResponse {
                    prompts: Vec::new(),
                    next_cursor: None,
                    meta: None,
                })
            })
        });

    register_tools(&mut server)?;

    let server = server.build();
    Ok(server)
}

fn register_tools<T: Transport>(server: &mut ServerBuilder<T>) -> Result<(), GatewayError> {
    // Search Tool
    let search_tool = Tool {
        name: "search".to_string(),
        description: Some("Search the web and return results".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            },
            "required": ["query"],
            "additionalProperties": false
        }),
    };

    // Register search tool
    server.register_tool(search_tool, |req: CallToolRequest| {
        Box::pin(async move {
            let args = req.arguments.unwrap_or_default();

            let result: Result<CallToolResponse, GatewayError> = async {
                let api_key = env::var("TAVILY_API_KEY").map_err(|_| {
                    GatewayError::CustomError("TAVILY_API_KEY not found in environment".to_string())
                })?;

                let query = args["query"].as_str().ok_or_else(|| {
                    GatewayError::CustomError("Missing query parameter".to_string())
                })?;

                let search_results = search_tavily(query, &api_key).await?;

                Ok(CallToolResponse {
                    content: vec![ToolResponseContent::Text {
                        text: serde_json::to_string(&search_results)?,
                    }],
                    is_error: None,
                    meta: None,
                })
            }
            .await;

            match result {
                Ok(response) => Ok(response),
                Err(e) => {
                    info!("Error handling request: {:#?}", e);
                    Ok(CallToolResponse {
                        content: vec![ToolResponseContent::Text {
                            text: format!("{}", e),
                        }],
                        is_error: Some(true),
                        meta: None,
                    })
                }
            }
        })
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_mcp::{
        client::ClientBuilder,
        protocol::RequestOptions,
        transport::{ClientInMemoryTransport, ServerInMemoryTransport, Transport},
    };
    use serde_json::json;

    use crate::{error::GatewayError, model::mcp_server::tavily::build};

    #[tokio::test]
    async fn test_tavily_tool() -> Result<(), GatewayError> {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            // needs to be stderr due to stdio transport
            .with_writer(std::io::stderr)
            .init();

        async fn async_server(transport: ServerInMemoryTransport) {
            let server = build(transport.clone()).unwrap();
            server.listen().await.unwrap();
        }

        let transport = ClientInMemoryTransport::new(|t| tokio::spawn(async_server(t)));
        transport
            .open()
            .await
            .map_err(|e| GatewayError::CustomError(e.to_string()))?;

        let client = ClientBuilder::new(transport).build();
        let client_clone = client.clone();
        tokio::spawn(async move { client_clone.start().await });

        let response = client
            .request(
                "tools/call",
                Some(json!({"name": "search", "arguments": {"query": "How many EOs did Trump sign?"}})),
                RequestOptions::default().timeout(Duration::from_secs(10)),
            )
            .await
            .map_err(|e| GatewayError::CustomError(e.to_string()))?;
        println!("{:?}", response);

        Ok(())
    }
}
