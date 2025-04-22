use std::{collections::HashMap, time::Duration};

use async_mcp::{
    client::ClientBuilder,
    protocol::RequestOptions,
    transport::{
        ClientInMemoryTransport, ClientSseTransport, ClientWsTransport, ServerInMemoryTransport,
        Transport,
    },
    types::{CallToolRequest, CallToolResponse, Tool, ToolResponseContent, ToolsListResponse},
};
use regex::Regex;
use serde_json::json;
use tracing::debug;

use super::mcp_server::tavily;
use crate::{
    error::GatewayError,
    types::gateway::{McpDefinition, McpTool, ServerTools, ToolsFilter},
};

fn validate_server_name(name: &str) -> Result<(), GatewayError> {
    match name {
        "websearch" => Ok(()),
        _ => Err(GatewayError::CustomError("Invalid server name".to_string())),
    }
}

async fn async_server(name: &str, transport: ServerInMemoryTransport) -> Result<(), GatewayError> {
    match name {
        "websearch" => {
            let server = tavily::build(transport)?;
            server
                .listen()
                .await
                .map_err(|e| GatewayError::CustomError(e.to_string()))
        }
        _ => Err(GatewayError::CustomError("Invalid server name".to_string())),
    }
}

macro_rules! with_transport {
    ($mcp_server:expr, $body:expr) => {
        match $mcp_server.r#type {
            crate::types::gateway::McpTransportType::Sse {
                server_url,
                headers,
            } => {
                let mut transport = ClientSseTransport::builder(server_url);
                for (k, v) in headers {
                    transport = transport.with_header(k.to_string(), v.to_string());
                }
                let transport = transport.build();
                transport
                    .open()
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))?;
                $body(transport)
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))
            }
            crate::types::gateway::McpTransportType::Ws {
                server_url,
                headers,
            } => {
                let mut transport = ClientWsTransport::builder(server_url);
                for (k, v) in headers {
                    transport = transport.with_header(k.to_string(), v.to_string());
                }
                let transport = transport.build();
                transport
                    .open()
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))?;
                $body(transport)
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))
            }
            crate::types::gateway::McpTransportType::InMemory { name } => {
                validate_server_name(&name)?;
                let client_transport = ClientInMemoryTransport::new(move |t| {
                    let name = name.clone();
                    tokio::spawn(async move {
                        let name = name.as_str();
                        async_server(name, t).await.unwrap()
                    })
                });
                client_transport
                    .open()
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))?;
                $body(client_transport)
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))
            }
        }
    };
}
pub async fn get_tools(definitions: &[McpDefinition]) -> Result<Vec<ServerTools>, GatewayError> {
    let mut all_tools = Vec::new();

    for tool_def in definitions {
        let mcp_server_name = tool_def.server_name();
        let tools: Result<Vec<Tool>, GatewayError> =
            with_transport!(tool_def.clone(), |transport| async move {
                let client = ClientBuilder::new(transport).build();

                let client_clone = client.clone();
                let _handle = tokio::spawn(async move { client_clone.start().await });
                // Get available tools
                let response = client
                    .request(
                        "tools/list",
                        Some(json!({})),
                        RequestOptions::default().timeout(Duration::from_secs(10)),
                    )
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))?;
                // Parse response into Vec<Tool>
                let response: ToolsListResponse = serde_json::from_value(response)?;
                let mut tools = response.tools;

                let total_tools = tools.len();

                // Filter tools based on actions_filter if specified
                match &tool_def.filter {
                    ToolsFilter::All => {
                        tracing::debug!(
                            "Loading all {} tools from {}",
                            total_tools,
                            mcp_server_name
                        );
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
                                tool.description = Some(d.clone());
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
                Ok::<Vec<Tool>, GatewayError>(tools)
            });

        match tools {
            Ok(tools) => {
                let mcp_tools = tools
                    .into_iter()
                    .map(|t| McpTool(t, tool_def.clone()))
                    .collect();

                all_tools.push(ServerTools {
                    tools: mcp_tools,
                    definition: tool_def.clone(),
                });
            }
            Err(e) => {
                tracing::error!("{e}");
                return Err(GatewayError::CustomError(e.to_string()));
            }
        }
    }

    tracing::debug!("Loaded {} tool definitions in total", all_tools.len());
    Ok(all_tools)
}

pub async fn execute_mcp_tool(
    def: &McpDefinition,
    tool: &async_mcp::types::Tool,
    inputs: HashMap<String, serde_json::Value>,
    meta: Option<serde_json::Value>,
) -> Result<String, GatewayError> {
    let name = tool.name.clone();
    let mcp_server = def.server_name();
    tracing::info!("Executing tool: {name}, mcp_server: {mcp_server}");

    let response: serde_json::Value = with_transport!(def.clone(), |transport| async move {
        let client = ClientBuilder::new(transport).build();
        let request = CallToolRequest {
            name: name.clone(),
            arguments: Some(inputs),
            meta,
        };

        let params =
            serde_json::to_value(request).map_err(|e| GatewayError::CustomError(e.to_string()))?;
        tracing::debug!("Sending tool request");
        tracing::debug!("{}", params);

        let client_clone = client.clone();
        let _handle = tokio::spawn(async move { client_clone.start().await });

        let response = client
            .request(
                "tools/call",
                Some(params),
                RequestOptions::default().timeout(Duration::from_secs(30)),
            )
            .await
            .map_err(|e| GatewayError::CustomError(e.to_string()))?;
        Ok::<serde_json::Value, GatewayError>(response)
    })?;

    let response: CallToolResponse = serde_json::from_value(response)?;
    tracing::debug!("Tool {name}: Processing tool response", name = tool.name);
    tracing::debug!("{:?}", response);
    let text = response
        .content
        .first()
        .and_then(|c| match c {
            ToolResponseContent::Text { text } => Some(text.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            tracing::error!(
                "Tool {name}: No text content in tool response",
                name = tool.name
            );
            GatewayError::CustomError("Tool {name}: No text content in response".to_string())
        })?;

    tracing::debug!(
        "Tool {name}: execution completed successfully",
        name = tool.name
    );
    Ok(text)
}
