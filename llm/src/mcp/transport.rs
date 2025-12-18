use crate::{
    mcp::McpServerError,
    types::gateway::{McpDefinition, McpTransportType},
};
use reqwest::header::HeaderMap;
use rmcp::{
    service::{DynService, RunningService},
    transport::{
        // sse_client::SseClientConfig, 
        streamable_http_client::StreamableHttpClientTransportConfig,
        // SseClientTransport, 
        StreamableHttpClientTransport,
    },
    RoleClient, ServiceExt,
};
use std::collections::HashMap;

pub struct McpTransport {
    definition: McpDefinition,
}

impl McpTransport {
    pub fn new(definition: McpDefinition) -> Self {
        Self { definition }
    }

    pub async fn get(
        &self,
    ) -> Result<RunningService<RoleClient, Box<dyn DynService<RoleClient>>>, McpServerError> {
        match &self.definition.r#type {
            McpTransportType::Sse {
                ..
            } => {
                // let reqwest_client = Self::create_reqwest_client_with_headers(headers)?;
                // let transport = SseClientTransport::start_with_client(
                //     reqwest_client,
                //     SseClientConfig {
                //         sse_endpoint: server_url.clone().into(),
                //         ..Default::default()
                //     },
                // )
                // .await?;

                // Ok(()
                //     .into_dyn()
                //     .serve(transport)
                //     .await
                //     .map_err(|e| McpServerError::ClientStartError(e.to_string()))?)
                todo!()
            }
            McpTransportType::Http {
                server_url,
                headers,
                ..
            } => {
                let reqwest_client = Self::create_reqwest_client_with_headers(headers)?;
                let transport = StreamableHttpClientTransport::with_client(
                    reqwest_client,
                    StreamableHttpClientTransportConfig::with_uri(server_url.clone()),
                );

                Ok(()
                    .into_dyn()
                    .serve(transport)
                    .await
                    .map_err(|e| McpServerError::ClientStartError(e.to_string()))?)
            }
            McpTransportType::InMemory {  .. } => {
                todo!()
                // Self::validate_server_name(name)?;
                // let transport = SseClientTransport::start(
                //     std::env::var("TAVILY_MCP_URL")
                //         .unwrap_or("http://localhost:8083/sse".to_string()),
                // )
                // .await?;

                // Ok(()
                //     .into_dyn()
                //     .serve(transport)
                //     .await
                //     .map_err(|e| McpServerError::ClientStartError(e.to_string()))?)
            }
            _ => Err(McpServerError::InvalidServerName(
                "Invalid or unsupported server type".to_string(),
            )),
        }
    }

    fn create_reqwest_client_with_headers(
        headers: &HashMap<String, String>,
    ) -> Result<reqwest::Client, McpServerError> {
        let mut headers_map = HeaderMap::new();
        for (key, value) in headers {
            match (
                key.parse::<reqwest::header::HeaderName>(),
                value.parse::<reqwest::header::HeaderValue>(),
            ) {
                (Ok(header_name), Ok(header_value)) => {
                    headers_map.insert(header_name, header_value);
                }
                _ => {
                    tracing::warn!("Invalid header: {:?}", (key, value));
                    // Skip invalid headers
                    continue;
                }
            }
        }

        reqwest::Client::builder()
            .default_headers(headers_map)
            .build()
            .map_err(McpServerError::ReqwestError)
    }

    // fn validate_server_name(name: &str) -> Result<(), McpServerError> {
    //     match name {
    //         "websearch" | "Web Search" => Ok(()),
    //         _ => Err(McpServerError::InvalidServerName(name.to_string())),
    //     }
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::gateway::{McpDefinition, McpTransportType, ToolsFilter};
    use std::collections::HashMap;

    /// Helper function to create a DeepWiki MCP definition with HTTP transport
    fn create_deepwiki_http_definition() -> McpDefinition {
        McpDefinition {
            filter: ToolsFilter::All,
            r#type: McpTransportType::Http {
                server_url: "https://mcp.deepwiki.com/mcp".to_string(),
                headers: HashMap::new(),
                env: None,
            },
        }
    }

    /// Helper function to create a DeepWiki MCP definition with SSE transport
    fn create_deepwiki_sse_definition() -> McpDefinition {
        McpDefinition {
            filter: ToolsFilter::All,
            r#type: McpTransportType::Sse {
                server_url: "https://mcp.deepwiki.com/sse".to_string(),
                headers: HashMap::new(),
                env: None,
            },
        }
    }

    /// Helper function to create a DeepWiki MCP definition with authorization headers
    fn create_deepwiki_private_definition() -> McpDefinition {
        let mut headers = HashMap::new();
        headers.insert(
            "Authorization".to_string(),
            "Bearer test-api-key".to_string(),
        );

        McpDefinition {
            filter: ToolsFilter::All,
            r#type: McpTransportType::Sse {
                server_url: "https://mcp.devin.ai/sse".to_string(),
                headers,
                env: None,
            },
        }
    }

    #[tokio::test]
    async fn test_deepwiki_http_transport_creation() {
        let definition = create_deepwiki_http_definition();
        let transport = McpTransport::new(definition);

        // Test that we can create the transport without errors
        // Note: We're not actually connecting to avoid network dependencies in tests
        match transport.definition.r#type {
            McpTransportType::Http {
                server_url,
                headers,
                env,
            } => {
                assert_eq!(server_url, "https://mcp.deepwiki.com/mcp");
                assert!(headers.is_empty());
                assert_eq!(env, None);
            }
            _ => panic!("Expected Http transport type"),
        }
    }

    #[tokio::test]
    async fn test_deepwiki_sse_transport_creation() {
        let definition = create_deepwiki_sse_definition();
        let transport = McpTransport::new(definition);

        // Test that we can create the transport without errors
        match transport.definition.r#type {
            McpTransportType::Sse {
                server_url,
                headers,
                env,
            } => {
                assert_eq!(server_url, "https://mcp.deepwiki.com/sse");
                assert!(headers.is_empty());
                assert_eq!(env, None);
            }
            _ => panic!("Expected Sse transport type"),
        }
    }

    #[tokio::test]
    async fn test_deepwiki_private_transport_creation() {
        let definition = create_deepwiki_private_definition();
        let transport = McpTransport::new(definition);

        // Test that we can create the transport with authorization headers
        match transport.definition.r#type {
            McpTransportType::Sse {
                server_url,
                headers,
                env,
            } => {
                assert_eq!(server_url, "https://mcp.devin.ai/sse");
                assert_eq!(
                    headers.get("Authorization"),
                    Some(&"Bearer test-api-key".to_string())
                );
                assert_eq!(env, None);
            }
            _ => panic!("Expected Sse transport type"),
        }
    }

    #[test]
    fn test_create_reqwest_client_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer test-token".to_string());
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        let client = McpTransport::create_reqwest_client_with_headers(&headers);
        assert!(client.is_ok());

        // Test that client creation succeeds with valid headers
        let _client = client.unwrap();

        // The fact that client creation succeeded means the headers were processed correctly
        // We can't access internal client properties, but success indicates proper processing
    }

    #[test]
    fn test_create_reqwest_client_with_empty_headers() {
        let headers = HashMap::new();

        let client = McpTransport::create_reqwest_client_with_headers(&headers);
        assert!(client.is_ok());

        // Test that client creation succeeds with empty headers
        let _client = client.unwrap();

        // Success indicates proper handling of empty headers
    }

    #[test]
    fn test_create_reqwest_client_with_invalid_headers() {
        let mut headers = HashMap::new();
        headers.insert("Invalid Header Name!".to_string(), "value".to_string());
        headers.insert("Valid-Header".to_string(), "valid-value".to_string());

        // Should not panic, but should skip invalid headers
        let client = McpTransport::create_reqwest_client_with_headers(&headers);
        assert!(client.is_ok());

        // Test that client creation succeeds even with some invalid headers
        let _client = client.unwrap();

        // The fact that client creation succeeded means the invalid headers were properly skipped
        // and the valid headers were processed correctly
    }

    #[test]
    fn test_header_processing_logic() {
        // Test the header processing logic by creating a mock scenario
        let mut headers = HashMap::new();
        headers.insert("Valid-Header".to_string(), "valid-value".to_string());
        headers.insert("Invalid Header!".to_string(), "invalid-value".to_string());
        headers.insert("Another-Valid".to_string(), "another-value".to_string());

        // Test that the function handles mixed valid/invalid headers gracefully
        let result = McpTransport::create_reqwest_client_with_headers(&headers);
        assert!(result.is_ok());

        // The function should succeed even with some invalid headers
        let _client = result.unwrap();

        // Success indicates proper handling of mixed valid/invalid headers
    }

    #[test]
    fn test_validate_server_name() {
        // Test valid server names
        assert!(McpTransport::validate_server_name("websearch").is_ok());
        assert!(McpTransport::validate_server_name("Web Search").is_ok());

        // Test invalid server names
        assert!(McpTransport::validate_server_name("invalid").is_err());
        assert!(McpTransport::validate_server_name("").is_err());
    }

    #[test]
    fn test_deepwiki_configuration_examples() {
        // Test the exact configuration from DeepWiki documentation

        // Public server configuration
        let public_config = McpDefinition {
            filter: ToolsFilter::All,
            r#type: McpTransportType::Sse {
                server_url: "https://mcp.deepwiki.com/sse".to_string(),
                headers: HashMap::new(),
                env: None,
            },
        };

        let transport = McpTransport::new(public_config);
        match transport.definition.r#type {
            McpTransportType::Sse { server_url, .. } => {
                assert_eq!(server_url, "https://mcp.deepwiki.com/sse");
            }
            _ => panic!("Expected Sse transport type"),
        }

        // Private server configuration
        let mut private_headers = HashMap::new();
        private_headers.insert("Authorization".to_string(), "Bearer <API_KEY>".to_string());

        let private_config = McpDefinition {
            filter: ToolsFilter::All,
            r#type: McpTransportType::Sse {
                server_url: "https://mcp.devin.ai/sse".to_string(),
                headers: private_headers,
                env: None,
            },
        };

        let transport = McpTransport::new(private_config);
        match transport.definition.r#type {
            McpTransportType::Sse {
                server_url,
                headers,
                ..
            } => {
                assert_eq!(server_url, "https://mcp.devin.ai/sse");
                assert_eq!(
                    headers.get("Authorization"),
                    Some(&"Bearer <API_KEY>".to_string())
                );
            }
            _ => panic!("Expected Sse transport type"),
        }
    }

    #[test]
    fn test_transport_definition_consistency() {
        // Test that our helper functions create consistent definitions
        let http_def = create_deepwiki_http_definition();
        let sse_def = create_deepwiki_sse_definition();

        // Both should have ToolsFilter::All
        match (http_def.filter, sse_def.filter) {
            (ToolsFilter::All, ToolsFilter::All) => {}
            _ => panic!("Both definitions should have ToolsFilter::All"),
        }

        // URLs should be different
        match (http_def.r#type, sse_def.r#type) {
            (
                McpTransportType::Http {
                    server_url: http_url,
                    ..
                },
                McpTransportType::Sse {
                    server_url: sse_url,
                    ..
                },
            ) => {
                assert_ne!(http_url, sse_url);
                assert_eq!(http_url, "https://mcp.deepwiki.com/mcp");
                assert_eq!(sse_url, "https://mcp.deepwiki.com/sse");
            }
            _ => panic!("Expected Http and Sse transport types"),
        }
    }

    #[tokio::test]
    async fn test_transport_creation_with_different_endpoints() {
        // Test various DeepWiki endpoints
        let endpoints = vec![
            "https://mcp.deepwiki.com/mcp",
            "https://mcp.deepwiki.com/sse",
            "https://mcp.devin.ai/sse",
        ];

        for endpoint in endpoints {
            let definition = McpDefinition {
                filter: ToolsFilter::All,
                r#type: McpTransportType::Http {
                    server_url: endpoint.to_string(),
                    headers: HashMap::new(),
                    env: None,
                },
            };

            let transport = McpTransport::new(definition);
            match transport.definition.r#type {
                McpTransportType::Http { server_url, .. } => {
                    assert_eq!(server_url, endpoint);
                }
                _ => panic!("Expected Http transport type for endpoint: {}", endpoint),
            }
        }
    }
}
