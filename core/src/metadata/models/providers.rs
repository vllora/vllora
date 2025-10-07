use crate::metadata::schema::providers;
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
#[diesel(table_name = providers)]
pub struct DbProvider {
    pub id: String,
    pub provider_name: String,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub priority: i32,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: i32,
}

#[cfg(feature = "sqlite")]
type All = Select<providers::table, AsSelect<DbProvider, Sqlite>>;
#[cfg(feature = "postgres")]
type All = Select<providers::table, AsSelect<DbProvider, Pg>>;

impl DbProvider {
    pub fn all() -> All {
        diesel::QueryDsl::select(providers::table, DbProvider::as_select())
    }

    /// Check if this provider is active
    pub fn is_active_provider(&self) -> bool {
        self.is_active != 0
    }

    /// Get provider type based on provider name
    pub fn get_provider_type(&self) -> String {
        match self.provider_name.to_lowercase().as_str() {
            "openai" => "api_key".to_string(),
            "anthropic" => "api_key".to_string(),
            "gemini" => "api_key".to_string(),
            "bedrock" => "aws".to_string(),
            "vertex-ai" => "vertex".to_string(),
            "azure" => "azure".to_string(),
            "openrouter" => "api_key".to_string(),
            "parasail" => "api_key".to_string(),
            "togetherai" => "api_key".to_string(),
            "xai" => "api_key".to_string(),
            "zai" => "api_key".to_string(),
            "mistralai" => "api_key".to_string(),
            "groq" => "api_key".to_string(),
            "deepinfra" => "api_key".to_string(),
            "deepseek" => "api_key".to_string(),
            "fireworksai" => "api_key".to_string(),
            _ => "api_key".to_string(), // Default to api_key for unknown providers
        }
    }
}

/// Insertable struct for creating new providers
#[derive(Insertable, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = providers)]
pub struct DbInsertProvider {
    pub id: String,
    pub provider_name: String,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub priority: i32,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: i32,
}

impl DbInsertProvider {
    pub fn new(
        id: String,
        provider_name: String,
        description: Option<String>,
        endpoint: Option<String>,
        priority: i32,
        privacy_policy_url: Option<String>,
        terms_of_service_url: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            provider_name,
            description,
            endpoint,
            priority,
            privacy_policy_url,
            terms_of_service_url,
            created_at: now.clone(),
            updated_at: now,
            is_active: 1,
        }
    }
}

/// Updateable struct for modifying providers
#[derive(AsChangeset, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = providers)]
pub struct DbUpdateProvider {
    pub provider_name: Option<String>,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub priority: Option<i32>,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    pub updated_at: String,
    pub is_active: Option<i32>,
}

impl DbUpdateProvider {
    pub fn new() -> Self {
        Self {
            provider_name: None,
            description: None,
            endpoint: None,
            priority: None,
            privacy_policy_url: None,
            terms_of_service_url: None,
            updated_at: chrono::Utc::now().to_rfc3339(),
            is_active: None,
        }
    }

    /// Update description with automatic timestamp
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self.updated_at = chrono::Utc::now().to_rfc3339();
        self
    }

    /// Update endpoint with automatic timestamp
    pub fn with_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = Some(endpoint);
        self.updated_at = chrono::Utc::now().to_rfc3339();
        self
    }

    /// Update priority with automatic timestamp
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = Some(priority);
        self.updated_at = chrono::Utc::now().to_rfc3339();
        self
    }

    /// Deactivate the provider
    pub fn deactivate(mut self) -> Self {
        self.is_active = Some(0);
        self.updated_at = chrono::Utc::now().to_rfc3339();
        self
    }

    /// Activate the provider
    pub fn activate(mut self) -> Self {
        self.is_active = Some(1);
        self.updated_at = chrono::Utc::now().to_rfc3339();
        self
    }
}

impl Default for DbUpdateProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_type_mapping() {
        let provider = DbProvider {
            id: "test-id".to_string(),
            provider_name: "openai".to_string(),
            description: None,
            endpoint: None,
            priority: 100,
            privacy_policy_url: None,
            terms_of_service_url: None,
            created_at: "2023-01-01T00:00:00Z".to_string(),
            updated_at: "2023-01-01T00:00:00Z".to_string(),
            is_active: 1,
        };

        assert_eq!(provider.get_provider_type(), "api_key");

        let bedrock_provider = DbProvider {
            provider_name: "bedrock".to_string(),
            ..provider.clone()
        };
        assert_eq!(bedrock_provider.get_provider_type(), "aws");

        let vertex_provider = DbProvider {
            provider_name: "vertex-ai".to_string(),
            ..provider
        };
        assert_eq!(vertex_provider.get_provider_type(), "vertex");
    }

    #[test]
    fn test_provider_activation() {
        let provider = DbProvider {
            id: "test-id".to_string(),
            provider_name: "test".to_string(),
            description: None,
            endpoint: None,
            priority: 0,
            privacy_policy_url: None,
            terms_of_service_url: None,
            created_at: "2023-01-01T00:00:00Z".to_string(),
            updated_at: "2023-01-01T00:00:00Z".to_string(),
            is_active: 1,
        };

        assert!(provider.is_active_provider());

        let inactive_provider = DbProvider {
            is_active: 0,
            ..provider
        };
        assert!(!inactive_provider.is_active_provider());
    }

    #[test]
    fn test_db_insert_provider() {
        let provider = DbInsertProvider::new(
            "test-id".to_string(),
            "test-provider".to_string(),
            Some("Test description".to_string()),
            Some("https://api.test.com/v1".to_string()),
            50,
            Some("https://test.com/privacy".to_string()),
            Some("https://test.com/terms".to_string()),
        );

        assert_eq!(provider.provider_name, "test-provider");
        assert_eq!(provider.priority, 50);
        assert_eq!(provider.is_active, 1);
    }

    #[test]
    fn test_db_update_provider() {
        let update = DbUpdateProvider::new()
            .with_description("Updated description".to_string())
            .with_priority(75)
            .activate();

        assert_eq!(update.description, Some("Updated description".to_string()));
        assert_eq!(update.priority, Some(75));
        assert_eq!(update.is_active, Some(1));
    }
}
