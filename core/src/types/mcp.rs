use crate::types::gateway::{McpDefinition, McpTransportType, ToolsFilter};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for MCP servers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpConfig {
    /// Map of server names to their configurations
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

/// Configuration for a single MCP server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfig {
    /// URL of the MCP server
    pub url: String,
    /// Optional headers to include in requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

impl McpConfig {
    /// Creates a new empty MCP configuration
    pub fn new() -> Self {
        Self {
            mcp_servers: HashMap::new(),
        }
    }

    /// Adds a server configuration
    pub fn add_server(&mut self, name: String, config: McpServerConfig) {
        self.mcp_servers.insert(name, config);
    }

    /// Gets a server configuration by name
    pub fn get_server(&self, name: &str) -> Option<&McpServerConfig> {
        self.mcp_servers.get(name)
    }

    /// Converts all server configurations to McpDefinition with HTTP transport
    pub fn to_mcp_definitions(&self) -> Vec<McpDefinition> {
        self.mcp_servers
            .values()
            .map(|config| config.to_mcp_definition())
            .collect()
    }

    /// Converts all server configurations to McpDefinition with SSE transport
    pub fn to_mcp_definitions_sse(&self) -> Vec<McpDefinition> {
        self.mcp_servers
            .values()
            .map(|config| config.to_mcp_definition_sse())
            .collect()
    }

    /// Converts all server configurations to McpDefinition with WebSocket transport
    pub fn to_mcp_definitions_ws(&self) -> Vec<McpDefinition> {
        self.mcp_servers
            .values()
            .map(|config| config.to_mcp_definition_ws())
            .collect()
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServerConfig {
    /// Creates a new MCP server configuration
    pub fn new(url: String) -> Self {
        Self { url, headers: None }
    }

    /// Creates a new MCP server configuration with headers
    pub fn with_headers(url: String, headers: HashMap<String, String>) -> Self {
        Self {
            url,
            headers: Some(headers),
        }
    }

    /// Adds a header to the configuration
    pub fn add_header(&mut self, key: String, value: String) {
        if self.headers.is_none() {
            self.headers = Some(HashMap::new());
        }
        if let Some(ref mut headers) = self.headers {
            headers.insert(key, value);
        }
    }

    /// Converts this McpServerConfig to an McpDefinition with HTTP transport
    pub fn to_mcp_definition(&self) -> McpDefinition {
        let transport_type = McpTransportType::Http {
            server_url: self.url.clone(),
            headers: self.headers.clone().unwrap_or_default(),
            env: None,
        };

        McpDefinition {
            filter: ToolsFilter::All,
            r#type: transport_type,
        }
    }

    /// Converts this McpServerConfig to an McpDefinition with SSE transport
    pub fn to_mcp_definition_sse(&self) -> McpDefinition {
        let transport_type = McpTransportType::Sse {
            server_url: self.url.clone(),
            headers: self.headers.clone().unwrap_or_default(),
            env: None,
        };

        McpDefinition {
            filter: ToolsFilter::All,
            r#type: transport_type,
        }
    }

    /// Converts this McpServerConfig to an McpDefinition with WebSocket transport
    pub fn to_mcp_definition_ws(&self) -> McpDefinition {
        let transport_type = McpTransportType::Ws {
            server_url: self.url.clone(),
            headers: self.headers.clone().unwrap_or_default(),
            env: None,
        };

        McpDefinition {
            filter: ToolsFilter::All,
            r#type: transport_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_config_creation() {
        let config = McpConfig::new();
        assert!(config.mcp_servers.is_empty());
    }

    #[test]
    fn test_mcp_server_config_creation() {
        let server_config = McpServerConfig::new("http://localhost:3000/mcp".to_string());
        assert_eq!(server_config.url, "http://localhost:3000/mcp");
        assert!(server_config.headers.is_none());
    }

    #[test]
    fn test_mcp_server_config_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("API_KEY".to_string(), "value".to_string());

        let server_config =
            McpServerConfig::with_headers("http://localhost:3000/mcp".to_string(), headers.clone());

        assert_eq!(server_config.url, "http://localhost:3000/mcp");
        assert_eq!(server_config.headers, Some(headers));
    }

    #[test]
    fn test_mcp_config_add_server() {
        let mut config = McpConfig::new();
        let server_config = McpServerConfig::new("http://localhost:3000/mcp".to_string());

        config.add_server("server-name".to_string(), server_config);

        assert!(config.get_server("server-name").is_some());
        assert_eq!(
            config.get_server("server-name").unwrap().url,
            "http://localhost:3000/mcp"
        );
    }

    #[test]
    fn test_serialization() {
        let mut config = McpConfig::new();
        let mut headers = HashMap::new();
        headers.insert("API_KEY".to_string(), "value".to_string());

        let server_config =
            McpServerConfig::with_headers("http://localhost:3000/mcp".to_string(), headers);

        config.add_server("server-name".to_string(), server_config);

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: McpConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_json_compatibility() {
        // Test that we can deserialize from the expected JSON format
        let json = r#"{
            "mcpServers": {
                "server-name": {
                    "url": "http://localhost:3000/mcp",
                    "headers": {
                        "API_KEY": "value"
                    }
                }
            }
        }"#;

        let config: McpConfig = serde_json::from_str(json).unwrap();

        assert!(config.get_server("server-name").is_some());
        let server = config.get_server("server-name").unwrap();
        assert_eq!(server.url, "http://localhost:3000/mcp");
        assert!(server.headers.is_some());
        assert_eq!(
            server.headers.as_ref().unwrap().get("API_KEY"),
            Some(&"value".to_string())
        );

        // Test that we can serialize back to the same format
        let serialized = serde_json::to_string(&config).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        // Verify the structure matches expected format
        assert!(
            parsed["mcpServers"]["server-name"]["url"].as_str().unwrap()
                == "http://localhost:3000/mcp"
        );
        assert!(
            parsed["mcpServers"]["server-name"]["headers"]["API_KEY"]
                .as_str()
                .unwrap()
                == "value"
        );
    }

    #[test]
    fn test_mcp_server_config_to_definition_http() {
        let mut headers = HashMap::new();
        headers.insert("API_KEY".to_string(), "value".to_string());

        let server_config =
            McpServerConfig::with_headers("http://localhost:3000/mcp".to_string(), headers);

        let definition = server_config.to_mcp_definition();

        match definition.r#type {
            McpTransportType::Http {
                server_url,
                headers,
                env,
            } => {
                assert_eq!(server_url, "http://localhost:3000/mcp");
                assert_eq!(headers.get("API_KEY"), Some(&"value".to_string()));
                assert_eq!(env, None);
            }
            _ => panic!("Expected Http transport type"),
        }

        match definition.filter {
            ToolsFilter::All => {}
            _ => panic!("Expected All tools filter"),
        }
    }

    #[test]
    fn test_mcp_server_config_to_definition_sse() {
        let mut headers = HashMap::new();
        headers.insert("API_KEY".to_string(), "value".to_string());

        let server_config =
            McpServerConfig::with_headers("http://localhost:3000/mcp".to_string(), headers);

        let definition = server_config.to_mcp_definition_sse();

        match definition.r#type {
            McpTransportType::Sse {
                server_url,
                headers,
                env,
            } => {
                assert_eq!(server_url, "http://localhost:3000/mcp");
                assert_eq!(headers.get("API_KEY"), Some(&"value".to_string()));
                assert_eq!(env, None);
            }
            _ => panic!("Expected Sse transport type"),
        }
    }

    #[test]
    fn test_mcp_server_config_to_definition_ws() {
        let mut headers = HashMap::new();
        headers.insert("API_KEY".to_string(), "value".to_string());

        let server_config =
            McpServerConfig::with_headers("ws://localhost:3000/mcp".to_string(), headers);

        let definition = server_config.to_mcp_definition_ws();

        match definition.r#type {
            McpTransportType::Ws {
                server_url,
                headers,
                env,
            } => {
                assert_eq!(server_url, "ws://localhost:3000/mcp");
                assert_eq!(headers.get("API_KEY"), Some(&"value".to_string()));
                assert_eq!(env, None);
            }
            _ => panic!("Expected Ws transport type"),
        }
    }

    #[test]
    fn test_mcp_server_config_to_definition_no_headers() {
        let server_config = McpServerConfig::new("http://localhost:3000/mcp".to_string());

        let definition = server_config.to_mcp_definition();

        match definition.r#type {
            McpTransportType::Http {
                server_url,
                headers,
                env,
            } => {
                assert_eq!(server_url, "http://localhost:3000/mcp");
                assert!(headers.is_empty());
                assert_eq!(env, None);
            }
            _ => panic!("Expected Http transport type"),
        }
    }

    #[test]
    fn test_mcp_config_to_definitions() {
        let mut config = McpConfig::new();

        let mut headers1 = HashMap::new();
        headers1.insert("API_KEY".to_string(), "value1".to_string());
        let server1 =
            McpServerConfig::with_headers("http://server1:3000/mcp".to_string(), headers1);

        let mut headers2 = HashMap::new();
        headers2.insert("API_KEY".to_string(), "value2".to_string());
        let server2 =
            McpServerConfig::with_headers("http://server2:3000/mcp".to_string(), headers2);

        config.add_server("server1".to_string(), server1);
        config.add_server("server2".to_string(), server2);

        let definitions = config.to_mcp_definitions();
        assert_eq!(definitions.len(), 2);

        // Check that both definitions are HTTP transport
        for definition in &definitions {
            match definition.r#type {
                McpTransportType::Http { .. } => {}
                _ => panic!("Expected Http transport type"),
            }
        }

        // Test SSE conversion
        let definitions_sse = config.to_mcp_definitions_sse();
        assert_eq!(definitions_sse.len(), 2);

        for definition in &definitions_sse {
            match definition.r#type {
                McpTransportType::Sse { .. } => {}
                _ => panic!("Expected Sse transport type"),
            }
        }
    }
}
