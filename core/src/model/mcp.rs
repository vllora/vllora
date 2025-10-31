use std::collections::HashMap;

use regex::Regex;
use rmcp::model::CallToolRequestParam;
use rmcp::model::{
    CallToolRequest, CallToolRequestMethod, ClientRequest, Extensions, GetMeta, ServerResult,
};
use rmcp::ServiceError;
use tracing::debug;

use crate::mcp::transport::McpTransport;
use crate::types::gateway::{McpDefinition, McpTool, ServerTools, ToolsFilter};

#[derive(Debug, thiserror::Error)]
pub enum McpServerError {
    #[error("Invalid server name: {0}")]
    InvalidServerName(String),

    #[error("Server initialization error: {0}")]
    ServerInitializeError(#[from] Box<rmcp::service::ServerInitializeError>),

    #[error("SSE transport error: {0}")]
    SseTransportError(#[from] rmcp::transport::sse_client::SseTransportError<reqwest::Error>),

    #[error("Client initialization error: {0}")]
    ClientInitializeError(#[from] Box<rmcp::service::ClientInitializeError>),

    #[error("Service error: {0}")]
    ServiceError(#[from] rmcp::ServiceError),

    #[error("Client start error: {0}")]
    ClientStartError(String),

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),

    #[error(transparent)]
    BoxedError(#[from] Box<dyn std::error::Error + Send + Sync>),

    #[error(transparent)]
    StdIOError(#[from] std::io::Error),

    #[error(transparent)]
    ParseError(#[from] serde_json::Error),

    #[error("No text content in tool {0} result")]
    NoTextInToolResult(String),

    #[error("Join error: {0}")]
    JoinError(#[from] tokio::task::JoinError),
}

impl From<rmcp::service::ClientInitializeError> for McpServerError {
    fn from(value: rmcp::service::ClientInitializeError) -> Self {
        McpServerError::ClientInitializeError(Box::new(value))
    }
}

impl From<rmcp::service::ServerInitializeError> for McpServerError {
    fn from(value: rmcp::service::ServerInitializeError) -> Self {
        McpServerError::ServerInitializeError(Box::new(value))
    }
}

pub fn stdio() -> (tokio::io::Stdin, tokio::io::Stdout) {
    (tokio::io::stdin(), tokio::io::stdout())
}

pub async fn get_tools(definitions: &[McpDefinition]) -> Result<Vec<ServerTools>, McpServerError> {
    let mut all_tools = Vec::new();

    for tool_def in definitions {
        let mcp_server_name = tool_def.server_name();
        let client = McpTransport::new(tool_def.clone()).get().await?;
        let tools = client.list_tools(Default::default()).await?;
        client.cancel().await?;

        let mut tools = tools.tools;
        let total_tools = tools.len();

        // Filter tools based on actions_filter if specified
        match &tool_def.filter {
            ToolsFilter::All => {
                tracing::debug!("Loading all {} tools from {}", total_tools, mcp_server_name);
            }
            ToolsFilter::Selected(selected) => {
                let before_count = tools.len();
                tools.retain_mut(|tool| {
                    let found = selected.iter().find(|t| {
                        if tool.name == t.name {
                            true
                        } else if let Ok(name_regex) = Regex::new(&t.name) {
                            debug!("Matching {} against pattern {}", tool.name, t.name);
                            name_regex.is_match(&tool.name)
                        } else {
                            false
                        }
                    });
                    if let Some(Some(d)) = found.as_ref().map(|t| t.description.as_ref()) {
                        tool.description = Some(d.clone().into());
                    }
                    found.is_some()
                });
                tracing::debug!(
                    "Filtered tools for {}: {}/{} tools selected",
                    mcp_server_name,
                    tools.len(),
                    before_count
                );
            }
        }

        let mcp_tools = tools
            .into_iter()
            .map(|t| McpTool(t, tool_def.clone()))
            .collect();

        all_tools.push(ServerTools {
            tools: mcp_tools,
            definition: tool_def.clone(),
        });
    }

    tracing::debug!("Loaded {} tool definitions in total", all_tools.len());
    Ok(all_tools)
}

pub async fn get_raw_tools(
    definitions: &McpDefinition,
) -> Result<Vec<rmcp::model::Tool>, McpServerError> {
    let client = McpTransport::new(definitions.clone()).get().await?;
    let tools = client.list_tools(Default::default()).await?;
    client.cancel().await?;

    Ok(tools.tools)
}

pub async fn execute_mcp_tool(
    def: &McpDefinition,
    tool: &rmcp::model::Tool,
    inputs: HashMap<String, serde_json::Value>,
    meta: Option<serde_json::Value>,
) -> Result<String, McpServerError> {
    let name = tool.name.clone();

    let client = McpTransport::new(def.clone()).get().await?;

    let mut args = serde_json::Map::new();

    for (key, value) in inputs {
        args.insert(key, value);
    }

    let params = CallToolRequestParam {
        name: tool.name.clone(),
        arguments: Some(args),
    };
    let mut t = ClientRequest::CallToolRequest(CallToolRequest {
        method: CallToolRequestMethod,
        params,
        extensions: Extensions::default(),
    });
    if let Some(meta) = meta {
        if let Some(map) = meta.as_object() {
            for (key, value) in map {
                t.get_meta_mut().insert(key.clone(), value.clone());
            }
        }
    }

    let response = client.send_request(t).await?;
    client.cancel().await?;

    let response = match response {
        ServerResult::CallToolResult(result) => Ok(result),
        _ => Err(ServiceError::UnexpectedResponse),
    }?;

    // Extract text from the response
    if !response.content.is_empty() {
        // Try to extract text from the first content item
        if let Some(content) = response.content.first() {
            // Access text content from the raw field
            if let Some(text) = content.raw.as_text().map(|t| t.text.clone()) {
                tracing::debug!("Tool {name}: execution completed successfully", name = name);
                return Ok(text);
            }
        }
    }

    tracing::error!("Tool {name}: No text content in tool response", name = name);
    Err(McpServerError::NoTextInToolResult(name.to_string()))
}
