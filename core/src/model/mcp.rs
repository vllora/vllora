use std::{collections::HashMap, time::Duration};

use async_mcp::{
    client::{Client, ClientBuilder},
    protocol::RequestOptions,
    transport::{ClientHttpTransport, ClientSseTransport, ClientWsTransport, Transport},
    types::{CallToolRequest, CallToolResponse, ToolResponseContent, ToolsListResponse},
};
use serde_json::json;

use crate::{
    error::GatewayError,
    types::gateway::{McpDefinition, McpTool, ServerTools, ToolsFilter},
};

use super::error::ModelError;

pub async fn get_client(
    mcp_server: &McpDefinition,
) -> Result<Client<ClientHttpTransport>, ModelError> {
    let transport = match mcp_server.r#type {
        crate::types::gateway::McpServerType::Sse => {
            let mut transport = ClientSseTransport::builder(mcp_server.server_url.clone());
            for (k, v) in &mcp_server.headers {
                transport = transport.with_header(k.to_string(), v.to_string());
            }
            let transport = transport.build();
            transport
                .open()
                .await
                .map_err(|e| ModelError::CustomError(e.to_string()))?;

            ClientHttpTransport::Sse(transport)
        }
        crate::types::gateway::McpServerType::Ws => {
            let mut transport = ClientWsTransport::builder(mcp_server.server_url.clone());
            for (k, v) in &mcp_server.headers {
                transport = transport.with_header(k.to_string(), v.to_string());
            }
            let transport = transport.build();
            transport
                .open()
                .await
                .map_err(|e| ModelError::CustomError(e.to_string()))?;
            ClientHttpTransport::Ws(transport)
        }
    };
    Ok(ClientBuilder::new(transport).build())
}
pub async fn get_mcp_tools(mcp_servers: &[McpDefinition]) -> Result<Vec<ServerTools>, ModelError> {
    // Create futures for each server
    let futures = mcp_servers.iter().map(|def| async move {
        let client: Client<ClientHttpTransport> = get_client(def).await?;

        // Start the client
        let client_clone = client.clone();
        let _client_handle = tokio::spawn(async move { client_clone.start().await });

        // Get available tools
        let response = client
            .request(
                "tools/list",
                Some(json!({})),
                RequestOptions::default().timeout(Duration::from_secs(10)),
            )
            .await
            .map_err(|e| ModelError::CustomError(e.to_string()))?;

        // Parse response into Vec<Tool>
        let response: ToolsListResponse = serde_json::from_value(response)?;
        let mut tools = response.tools;

        let total_tools = tools.len();

        // Filter tools based on actions_filter if specified
        match &def.filter {
            ToolsFilter::All => {
                tracing::info!("Loading all {} tools from {}", total_tools, def.server_url);
            }
            ToolsFilter::Selected(selected) => {
                let before_count = tools.len();
                tools.retain_mut(|tool| {
                    let found = selected.iter().find(|t| *t.name == tool.name);
                    if let Some(Some(d)) = found.as_ref().map(|t| t.description.as_ref()) {
                        tool.description = Some(d.clone());
                    }
                    found.is_some()
                });
                tracing::info!(
                    "Filtered tools for {}: {}/{} tools selected",
                    def.server_url,
                    tools.len(),
                    before_count
                );
            }
        }

        // Convert Vec<Tool> to Vec<McpTool>
        let mcp_tools = tools.into_iter().map(|t| McpTool(t, def.clone())).collect();

        Ok::<_, ModelError>(ServerTools {
            tools: mcp_tools,
            definition: def.clone(),
        })
    });

    // Run all futures in parallel and collect results
    let results = futures::future::join_all(futures).await;

    // Collect successful results and log errors
    let all_tools: Vec<ServerTools> = results
        .into_iter()
        .filter_map(|result| match result {
            Ok(tools) => Some(tools),
            Err(e) => {
                tracing::error!("Failed to get tools from MCP server: {}", e);
                None
            }
        })
        .collect();

    tracing::info!("Loaded {} tool definitions in total", all_tools.len());
    Ok(all_tools)
}

pub async fn execute_mcp_tool(
    def: &McpDefinition,
    tool: &async_mcp::types::Tool,
    inputs: HashMap<String, serde_json::Value>,
) -> Result<String, GatewayError> {
    let name = tool.name.clone();
    let mcp_server = def.server_url.to_string();
    tracing::info!("Executing tool: {name}, mcp_server: {mcp_server}");

    let request = CallToolRequest {
        name: name.clone(),
        arguments: Some(inputs),
        meta: None,
    };

    let params = serde_json::to_value(request)?;

    tracing::debug!("Starting tool client");
    let client: Client<ClientHttpTransport> = get_client(def).await?;

    // Start the client
    let client_clone = client.clone();
    let client_handle = tokio::spawn(async move { client_clone.start().await });

    tracing::debug!("Sending tool request");
    tracing::debug!("{}", params);
    let response = client
        .request(
            "tools/call",
            Some(params),
            RequestOptions::default().timeout(Duration::from_secs(10)),
        )
        .await
        .map_err(|e| GatewayError::CustomError(e.to_string()))?;

    let response: CallToolResponse = serde_json::from_value(response)?;

    tracing::debug!("Tool {name}: Processing tool response");
    tracing::debug!("{:?}", response);
    let text = response
        .content
        .first()
        .and_then(|c| match c {
            ToolResponseContent::Text { text } => Some(text.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            tracing::error!("Tool {name}: No text content in tool response");
            GatewayError::CustomError("Tool {name}: No text content in response".to_string())
        })?;
    client_handle.abort();
    tracing::debug!("Tool {name}: execution completed successfully");
    Ok(text)
}
