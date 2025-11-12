use crate::metadata::types::UUID;
use crate::rmcp::model::Tool;
use chrono::{DateTime, Utc};
use diesel::helper_types::AsSelect;
use diesel::helper_types::Select;
#[cfg(feature = "postgres")]
use diesel::pg::Pg;
#[cfg(feature = "sqlite")]
use diesel::sqlite::Sqlite;
use diesel::SelectableHelper;
use diesel::{AsChangeset, Insertable, QueryableByName, Selectable};
use diesel::{Identifiable, Queryable};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::metadata::schema::mcp_configs;
use crate::types::mcp::McpConfig;

#[cfg(feature = "sqlite")]
use diesel::sql_types::Text;
#[cfg(feature = "postgres")]
use diesel::sql_types::Uuid;

#[derive(
    QueryableByName,
    Selectable,
    Queryable,
    PartialEq,
    Eq,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    Identifiable,
    AsChangeset,
)]
#[serde(crate = "serde")]
#[diesel(table_name = mcp_configs)]
pub struct DbMcpConfig {
    #[cfg_attr(feature = "postgres", diesel(sql_type = Uuid))]
    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    pub id: UUID,
    pub company_slug: String,
    pub config: String,
    pub tools: String,
    pub tools_refreshed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Insertable, Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = mcp_configs)]
pub struct NewMcpConfig {
    pub company_slug: String,
    pub config: String,
    pub tools: String,
    pub tools_refreshed_at: Option<DateTime<Utc>>,
}

#[derive(AsChangeset, Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = mcp_configs)]
pub struct UpdateMcpConfig {
    pub company_slug: Option<String>,
    pub config: Option<String>,
    pub tools: Option<String>,
    pub tools_refreshed_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[cfg(feature = "sqlite")]
type All = Select<mcp_configs::table, AsSelect<DbMcpConfig, Sqlite>>;
#[cfg(feature = "postgres")]
type All = Select<mcp_configs::table, AsSelect<DbMcpConfig, Pg>>;

impl DbMcpConfig {
    #[cfg(feature = "sqlite")]
    pub fn all() -> All {
        diesel::QueryDsl::select(mcp_configs::table, DbMcpConfig::as_select())
    }

    #[cfg(feature = "postgres")]
    pub fn all() -> All {
        diesel::QueryDsl::select(mcp_configs::table, DbMcpConfig::as_select())
    }

    /// Converts the database model to the domain model
    pub fn to_mcp_config(&self) -> Result<McpConfig, serde_json::Error> {
        serde_json::from_str(&self.config)
    }

    /// Gets the tools as a JSON value
    pub fn get_tools(&self) -> Result<Value, serde_json::Error> {
        serde_json::from_str(&self.tools)
    }

    /// Checks if tools need to be refreshed based on the last refresh time
    pub fn should_refresh_tools(&self, max_age_minutes: i64) -> bool {
        match &self.tools_refreshed_at {
            Some(last_refresh) => {
                let now = Utc::now();
                let age = now.signed_duration_since(last_refresh.with_timezone(&Utc));
                age.num_minutes() > max_age_minutes
            }
            None => true, // Never refreshed, so should refresh
        }
    }

    /// Updates the tools and refresh timestamp
    pub fn update_tools(
        &mut self,
        tools: &HashMap<String, Vec<Tool>>,
    ) -> Result<(), serde_json::Error> {
        self.tools = serde_json::to_string(tools)?;
        self.tools_refreshed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Updates the MCP configuration
    pub fn update_config(&mut self, config: McpConfig) -> Result<(), serde_json::Error> {
        self.config = serde_json::to_string(&config)?;
        self.updated_at = Utc::now();
        Ok(())
    }
}

impl NewMcpConfig {
    /// Creates a new MCP config from a domain model
    pub fn from_mcp_config(
        company_slug: String,
        config: &McpConfig,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            company_slug,
            config: serde_json::to_string(config)?,
            tools: serde_json::to_string(&serde_json::Value::Array(vec![]))?, // Empty tools array by default
            tools_refreshed_at: None,
        })
    }

    /// Creates a new MCP config with tools
    pub fn from_mcp_config_with_tools(
        company_slug: String,
        config: &McpConfig,
        tools: &HashMap<String, Vec<Tool>>,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            company_slug,
            config: serde_json::to_string(config)?,
            tools: serde_json::to_string(tools)?,
            tools_refreshed_at: Some(Utc::now()),
        })
    }
}

impl UpdateMcpConfig {
    /// Creates an update from a domain model
    pub fn from_mcp_config(config: &McpConfig) -> Result<Self, serde_json::Error> {
        Ok(Self {
            company_slug: None,
            config: Some(serde_json::to_string(config)?),
            tools: None,
            tools_refreshed_at: None,
            updated_at: Some(Utc::now()),
        })
    }

    /// Creates an update for tools only
    pub fn from_tools(tools: &HashMap<String, Vec<Tool>>) -> Result<Self, serde_json::Error> {
        Ok(Self {
            company_slug: None,
            config: None,
            tools: Some(serde_json::to_string(tools)?),
            tools_refreshed_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        })
    }

    /// Creates an update for both config and tools
    pub fn from_config_and_tools(
        config: &McpConfig,
        tools: &HashMap<String, Vec<Tool>>,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            company_slug: None,
            config: Some(serde_json::to_string(config)?),
            tools: Some(serde_json::to_string(tools)?),
            tools_refreshed_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::mcp::McpServerType;
    use crate::types::mcp::{McpConfig, McpServerConfig};
    use rmcp::model::JsonObject;
    use std::collections::HashMap;

    #[test]
    fn test_new_mcp_config_from_domain_model() {
        let mut config = McpConfig::new();
        let server_config =
            McpServerConfig::new("http://localhost:3000/mcp".to_string(), McpServerType::Http);
        config.add_server("test-server".to_string(), server_config);

        let new_config =
            NewMcpConfig::from_mcp_config("test-company".to_string(), &config).unwrap();
        assert_eq!(new_config.company_slug, "test-company");
        let tools: Value = serde_json::from_str(&new_config.tools).unwrap();
        assert!(tools.is_array());
        assert_eq!(tools.as_array().unwrap().len(), 0);
        assert!(new_config.tools_refreshed_at.is_none());
    }

    #[test]
    fn test_new_mcp_config_with_tools() {
        let mut config = McpConfig::new();
        let server_config =
            McpServerConfig::new("http://localhost:3000/mcp".to_string(), McpServerType::Http);
        config.add_server("test-server".to_string(), server_config);

        let tools = HashMap::from([(
            "test-tool".to_string(),
            vec![Tool::new("test-tool", "A test tool", JsonObject::new())],
        )]);
        let new_config =
            NewMcpConfig::from_mcp_config_with_tools("test-company".to_string(), &config, &tools)
                .unwrap();

        assert_eq!(new_config.company_slug, "test-company");
        assert_eq!(new_config.tools, serde_json::to_string(&tools).unwrap());
        assert!(new_config.tools_refreshed_at.is_some());
    }

    #[test]
    fn test_should_refresh_tools() {
        let config = DbMcpConfig {
            id: "test-id".to_string(),
            company_slug: "test-company".to_string(),
            config: "{}".to_string(),
            tools: "[]".to_string(),
            tools_refreshed_at: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        // Should refresh if never refreshed
        assert!(config.should_refresh_tools(60));

        let config_with_recent_refresh = DbMcpConfig {
            id: "test-id".to_string(),
            company_slug: "test-company".to_string(),
            config: "{}".to_string(),
            tools: "[]".to_string(),
            tools_refreshed_at: Some(Utc::now().to_rfc3339()),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        // Should not refresh if recently refreshed
        assert!(!config_with_recent_refresh.should_refresh_tools(60));

        let config_with_old_refresh = DbMcpConfig {
            id: "test-id".to_string(),
            company_slug: "test-company".to_string(),
            config: "{}".to_string(),
            tools: "[]".to_string(),
            tools_refreshed_at: Some((Utc::now() - chrono::Duration::minutes(120)).to_rfc3339()),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        // Should refresh if old refresh
        assert!(config_with_old_refresh.should_refresh_tools(60));
    }

    #[test]
    fn test_update_tools() {
        let mut config = DbMcpConfig {
            id: "test-id".to_string(),
            company_slug: "test-company".to_string(),
            config: "{}".to_string(),
            tools: "[]".to_string(),
            tools_refreshed_at: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        let old_updated_at = config.updated_at.clone();

        let new_tools = HashMap::from([(
            "updated-tool".to_string(),
            vec![Tool::new(
                "updated-tool",
                "An updated tool",
                JsonObject::new(),
            )],
        )]);
        config.update_tools(&new_tools).unwrap();

        assert_eq!(config.tools, serde_json::to_string(&new_tools).unwrap());
        assert!(config.tools_refreshed_at.is_some());
        assert!(config.updated_at > old_updated_at);
    }

    #[test]
    fn test_update_config() {
        let mut config = DbMcpConfig {
            id: "test-id".to_string(),
            company_slug: "test-company".to_string(),
            config: "{}".to_string(),
            tools: "[]".to_string(),
            tools_refreshed_at: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        let mut new_mcp_config = McpConfig::new();
        let server_config =
            McpServerConfig::new("http://updated:3000/mcp".to_string(), McpServerType::Http);
        new_mcp_config.add_server("updated-server".to_string(), server_config);

        let old_updated_at = config.updated_at.clone();

        config.update_config(new_mcp_config).unwrap();

        assert!(config.updated_at > old_updated_at);

        // Verify the config was updated
        let updated_config = config.to_mcp_config().unwrap();
        assert!(updated_config.get_server("updated-server").is_some());
    }
}
