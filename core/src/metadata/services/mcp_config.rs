use chrono::Utc;
use diesel::prelude::*;

use crate::metadata::models::mcp_config::{DbMcpConfig, NewMcpConfig, UpdateMcpConfig};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::mcp_configs;
use crate::types::mcp::McpConfig;

use crate::metadata::error::DatabaseError;

use crate::rmcp::model::Tool;
use std::collections::HashMap;

pub struct McpConfigService {
    db_pool: DbPool,
}

impl McpConfigService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    /// Creates a new MCP configuration
    pub fn create(&self, company_slug: String, config: &McpConfig) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let new_config = NewMcpConfig::from_mcp_config(company_slug, config)?;

        Ok(diesel::insert_into(mcp_configs::table)
            .values(&new_config)
            .execute(&mut conn)?)
    }

    /// Creates a new MCP configuration with tools
    pub fn create_with_tools(
        &self,
        company_slug: String,
        config: &McpConfig,
        tools: &HashMap<String, Vec<Tool>>,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let new_config = NewMcpConfig::from_mcp_config_with_tools(company_slug, config, tools)?;

        Ok(diesel::insert_into(mcp_configs::table)
            .values(&new_config)
            .execute(&mut conn)?)
    }

    /// Gets an MCP configuration by ID
    pub fn get_by_id(&self, id: &str) -> Result<DbMcpConfig, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(mcp_configs::table
            .filter(mcp_configs::id.eq(id))
            .first::<DbMcpConfig>(&mut conn)?)
    }

    /// Gets an MCP configuration by company slug
    pub fn get_by_company_slug(&self, company_slug: &str) -> Result<DbMcpConfig, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(mcp_configs::table
            .filter(mcp_configs::company_slug.eq(company_slug))
            .first::<DbMcpConfig>(&mut conn)?)
    }

    /// Gets all MCP configurations
    pub fn get_all(&self) -> Result<Vec<DbMcpConfig>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(mcp_configs::table
            .order(mcp_configs::created_at.desc())
            .load::<DbMcpConfig>(&mut conn)?)
    }

    /// Upserts an MCP configuration for a company (insert or update)
    /// This ensures one setting per company by using company_slug as the unique key
    pub fn upsert(
        &self,
        company_slug: String,
        config: &McpConfig,
    ) -> Result<DbMcpConfig, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Try to get existing config
        let existing = mcp_configs::table
            .filter(mcp_configs::company_slug.eq(&company_slug))
            .first::<DbMcpConfig>(&mut conn)
            .optional()?;

        match existing {
            Some(_existing_config) => {
                // Update existing config
                let update = UpdateMcpConfig::from_mcp_config(config)?;
                diesel::update(
                    mcp_configs::table.filter(mcp_configs::company_slug.eq(&company_slug)),
                )
                .set(&update)
                .execute(&mut conn)?;

                // Fetch the updated config
                Ok(mcp_configs::table
                    .filter(mcp_configs::company_slug.eq(&company_slug))
                    .first::<DbMcpConfig>(&mut conn)?)
            }
            None => {
                // Insert new config
                let new_config = NewMcpConfig::from_mcp_config(company_slug.clone(), config)?;
                diesel::insert_into(mcp_configs::table)
                    .values(&new_config)
                    .execute(&mut conn)?;

                // Fetch the inserted config
                Ok(mcp_configs::table
                    .filter(mcp_configs::company_slug.eq(&company_slug))
                    .first::<DbMcpConfig>(&mut conn)?)
            }
        }
    }

    /// Upserts an MCP configuration with tools for a company
    pub fn upsert_with_tools(
        &self,
        company_slug: String,
        config: &McpConfig,
        tools: &HashMap<String, Vec<Tool>>,
    ) -> Result<DbMcpConfig, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Try to get existing config
        let existing = mcp_configs::table
            .filter(mcp_configs::company_slug.eq(&company_slug))
            .first::<DbMcpConfig>(&mut conn)
            .optional()?;

        match existing {
            Some(_) => {
                // Update existing config with both config and tools
                let update = UpdateMcpConfig::from_config_and_tools(config, tools)?;
                diesel::update(
                    mcp_configs::table.filter(mcp_configs::company_slug.eq(&company_slug)),
                )
                .set(&update)
                .execute(&mut conn)?;

                // Fetch the updated config
                Ok(mcp_configs::table
                    .filter(mcp_configs::company_slug.eq(&company_slug))
                    .first::<DbMcpConfig>(&mut conn)?)
            }
            None => {
                // Insert new config with tools
                let new_config =
                    NewMcpConfig::from_mcp_config_with_tools(company_slug.clone(), config, tools)?;
                diesel::insert_into(mcp_configs::table)
                    .values(&new_config)
                    .execute(&mut conn)?;

                // Fetch the inserted config
                Ok(mcp_configs::table
                    .filter(mcp_configs::company_slug.eq(&company_slug))
                    .first::<DbMcpConfig>(&mut conn)?)
            }
        }
    }

    /// Gets MCP configurations that need tool refresh
    pub fn get_configs_needing_tool_refresh(
        &self,
        max_age_minutes: i64,
    ) -> Result<Vec<DbMcpConfig>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let cutoff_time = (Utc::now() - chrono::Duration::minutes(max_age_minutes)).to_rfc3339();

        Ok(mcp_configs::table
            .filter(
                mcp_configs::tools_refreshed_at
                    .is_null()
                    .or(mcp_configs::tools_refreshed_at.lt(cutoff_time)),
            )
            .order(mcp_configs::created_at.desc())
            .load::<DbMcpConfig>(&mut conn)?)
    }

    /// Updates an MCP configuration    
    pub fn update_config(&self, id: &str, config: &McpConfig) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let update = UpdateMcpConfig::from_mcp_config(config)?;
        Ok(
            diesel::update(mcp_configs::table.filter(mcp_configs::id.eq(id)))
                .set(&update)
                .execute(&mut conn)?,
        )
    }

    /// Updates tools for an MCP configuration
    pub fn update_tools(
        &self,
        id: &str,
        tools: &HashMap<String, Vec<Tool>>,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let update = UpdateMcpConfig::from_tools(tools)?;

        Ok(
            diesel::update(mcp_configs::table.filter(mcp_configs::id.eq(id)))
                .set(&update)
                .execute(&mut conn)?,
        )
    }

    /// Updates both config and tools for an MCP configuration
    pub fn update_config_and_tools(
        &self,
        id: &str,
        config: &McpConfig,
        tools: &HashMap<String, Vec<Tool>>,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let update = UpdateMcpConfig::from_config_and_tools(config, tools)?;

        Ok(
            diesel::update(mcp_configs::table.filter(mcp_configs::id.eq(id)))
                .set(&update)
                .execute(&mut conn)?,
        )
    }

    /// Deletes an MCP configuration
    pub fn delete(&self, id: &str) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(diesel::delete(mcp_configs::table.filter(mcp_configs::id.eq(id))).execute(&mut conn)?)
    }

    /// Gets the count of MCP configurations
    pub fn count(&self) -> Result<i64, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(mcp_configs::table.count().get_result(&mut conn)?)
    }

    /// Gets MCP configurations created within a time range
    pub fn get_by_created_at_range(
        &self,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> Result<Vec<DbMcpConfig>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let start_str = start.to_rfc3339();
        let end_str = end.to_rfc3339();
        Ok(mcp_configs::table
            .filter(mcp_configs::created_at.between(start_str, end_str))
            .order(mcp_configs::created_at.desc())
            .load::<DbMcpConfig>(&mut conn)?)
    }

    /// Gets MCP configurations updated within a time range
    pub fn get_by_updated_at_range(
        &self,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> Result<Vec<DbMcpConfig>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let start_str = start.to_rfc3339();
        let end_str = end.to_rfc3339();
        Ok(mcp_configs::table
            .filter(mcp_configs::updated_at.between(start_str, end_str))
            .order(mcp_configs::updated_at.desc())
            .load::<DbMcpConfig>(&mut conn)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::mcp::{McpConfig, McpServerConfig};
    use rmcp::model::JsonObject;
    use std::collections::HashMap;
    use serde_json::Value;
    use crate::types::mcp::McpServerType;
    
    #[test]
    fn test_mcp_config_creation_validation() {
        let mut config = McpConfig::new();
        let server_config = McpServerConfig::new("http://localhost:3000/mcp".to_string(), McpServerType::Http);
        config.add_server("test-server".to_string(), server_config);

        // Test that we can create a new config struct
        let new_config =
            NewMcpConfig::from_mcp_config("test-company".to_string(), &config).unwrap();
        assert_eq!(new_config.company_slug, "test-company");
        let tools: Value = serde_json::from_str(&new_config.tools).unwrap();
        assert!(tools.is_array());
        assert_eq!(tools.as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_update_mcp_config_validation() {
        let mut config = McpConfig::new();
        let server_config = McpServerConfig::new("http://localhost:3000/mcp".to_string(), McpServerType::Http);
        config.add_server("test-server".to_string(), server_config);

        let update = UpdateMcpConfig::from_mcp_config(&config).unwrap();
        assert!(update.config.is_some());
        assert!(update.updated_at.is_some());
        assert!(update.tools.is_none());
    }

    #[test]
    fn test_update_tools_validation() {
        let tools = HashMap::from([(
            "test-tool".to_string(),
            vec![Tool::new("test-tool", "A test tool", JsonObject::new())],
        )]);

        let update = UpdateMcpConfig::from_tools(&tools).unwrap();
        assert_eq!(update.tools, Some(serde_json::to_string(&tools).unwrap()));
        assert!(update.tools_refreshed_at.is_some());
        assert!(update.updated_at.is_some());
        assert!(update.config.is_none());
        assert!(update.company_slug.is_none());
    }

    #[test]
    fn test_upsert_functionality() {
        let mut config = McpConfig::new();
        let server_config = McpServerConfig::new("http://localhost:3000/mcp".to_string(), McpServerType::Http);
        config.add_server("test-server".to_string(), server_config);

        // Test upsert creation (should create new)
        let new_config =
            NewMcpConfig::from_mcp_config("test-company".to_string(), &config).unwrap();
        assert_eq!(new_config.company_slug, "test-company");

        // Test upsert update (should update existing)
        let update_config = UpdateMcpConfig::from_mcp_config(&config).unwrap();
        assert!(update_config.company_slug.is_none()); // Company slug shouldn't change in updates
        assert!(update_config.config.is_some());
        assert!(update_config.updated_at.is_some());
    }
}
