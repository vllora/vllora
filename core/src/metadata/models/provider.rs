use crate::metadata::schema::provider_credentials;
use crate::types::credentials::Credentials;
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
#[diesel(table_name = provider_credentials)]
pub struct DbProviderCredentials {
    pub id: String,
    pub provider_name: String,
    pub provider_type: String,
    pub credentials: String, // JSON serialized credentials
    pub project_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: i32,
}

#[cfg(feature = "sqlite")]
type All = Select<provider_credentials::table, AsSelect<DbProviderCredentials, Sqlite>>;
#[cfg(feature = "postgres")]
type All = Select<provider_credentials::table, AsSelect<DbProviderCredentials, Pg>>;

impl DbProviderCredentials {
    pub fn all() -> All {
        diesel::QueryDsl::select(
            provider_credentials::table,
            DbProviderCredentials::as_select(),
        )
    }

    /// Parse the JSON credentials string into a Credentials enum
    pub fn parse_credentials(&self) -> Result<Credentials, serde_json::Error> {
        serde_json::from_str(&self.credentials)
    }

    /// Set the credentials by serializing a Credentials enum to JSON
    pub fn set_credentials(&mut self, credentials: &Credentials) -> Result<(), serde_json::Error> {
        self.credentials = serde_json::to_string(credentials)?;
        Ok(())
    }

    /// Check if this is a global credential (not project-specific)
    pub fn is_global(&self) -> bool {
        self.project_id.is_none()
    }

    /// Check if this credential is active
    pub fn is_active_credential(&self) -> bool {
        self.is_active != 0
    }
}

/// Insertable struct for creating new provider credentials
#[derive(Insertable, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = provider_credentials)]
pub struct DbInsertProviderCredentials {
    pub id: String,
    pub provider_name: String,
    pub provider_type: String,
    pub credentials: String,
    pub project_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: i32,
}

impl DbInsertProviderCredentials {
    pub fn new(
        provider_name: String,
        provider_type: String,
        credentials: String,
        project_id: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            provider_name,
            provider_type,
            credentials,
            project_id,
            created_at: now.clone(),
            updated_at: now,
            is_active: 1,
        }
    }

    /// Create a new global provider credential
    pub fn new_global(provider_name: String, provider_type: String, credentials: String) -> Self {
        Self::new(provider_name, provider_type, credentials, None)
    }

    /// Create a new project-specific provider credential
    pub fn new_project(
        provider_name: String,
        provider_type: String,
        credentials: String,
        project_id: String,
    ) -> Self {
        Self::new(provider_name, provider_type, credentials, Some(project_id))
    }
}

/// Updateable struct for modifying provider credentials
#[derive(AsChangeset, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = provider_credentials)]
pub struct DbUpdateProviderCredentials {
    pub provider_name: Option<String>,
    pub provider_type: Option<String>,
    pub credentials: Option<String>,
    pub updated_at: String,
    pub is_active: Option<i32>,
}

impl DbUpdateProviderCredentials {
    pub fn new() -> Self {
        Self {
            provider_name: None,
            provider_type: None,
            credentials: None,
            updated_at: chrono::Utc::now().to_rfc3339(),
            is_active: None,
        }
    }

    /// Update credentials with automatic timestamp
    pub fn with_credentials(mut self, credentials: String) -> Self {
        self.credentials = Some(credentials);
        self.updated_at = chrono::Utc::now().to_rfc3339();
        self
    }

    /// Update provider type with automatic timestamp
    pub fn with_provider_type(mut self, provider_type: String) -> Self {
        self.provider_type = Some(provider_type);
        self.updated_at = chrono::Utc::now().to_rfc3339();
        self
    }

    /// Deactivate the credential
    pub fn deactivate(mut self) -> Self {
        self.is_active = Some(0);
        self.updated_at = chrono::Utc::now().to_rfc3339();
        self
    }

    /// Activate the credential
    pub fn activate(mut self) -> Self {
        self.is_active = Some(1);
        self.updated_at = chrono::Utc::now().to_rfc3339();
        self
    }
}

impl Default for DbUpdateProviderCredentials {
    fn default() -> Self {
        Self::new()
    }
}

/// DTO for creating provider credentials from API requests
#[derive(PartialEq, Debug, Serialize, Deserialize, Default, Clone)]
#[serde(crate = "serde")]
pub struct NewProviderCredentialsDTO {
    pub provider_name: String,
    pub provider_type: String,
    pub credentials: Credentials,
    pub project_id: Option<String>,
}

impl NewProviderCredentialsDTO {
    /// Convert to database insertable struct
    pub fn to_db_insert(&self) -> Result<DbInsertProviderCredentials, serde_json::Error> {
        let credentials_json = serde_json::to_string(&self.credentials)?;
        Ok(DbInsertProviderCredentials::new(
            self.provider_name.clone(),
            self.provider_type.clone(),
            credentials_json,
            self.project_id.clone(),
        ))
    }
}

/// DTO for updating provider credentials from API requests
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateProviderCredentialsDTO {
    pub credentials: Option<Credentials>,
    pub is_active: Option<bool>,
}

impl UpdateProviderCredentialsDTO {
    /// Convert to database updateable struct
    pub fn to_db_update(&self) -> Result<DbUpdateProviderCredentials, serde_json::Error> {
        let mut db_update = DbUpdateProviderCredentials::new();

        if let Some(credentials) = &self.credentials {
            let credentials_json = serde_json::to_string(credentials)?;
            db_update = db_update.with_credentials(credentials_json);
        }

        if let Some(is_active) = self.is_active {
            db_update = if is_active {
                db_update.activate()
            } else {
                db_update.deactivate()
            };
        }

        Ok(db_update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::credentials::{ApiKeyCredentials, Credentials};

    fn test_provider_credentials() -> DbProviderCredentials {
        DbProviderCredentials {
            id: "test-id".to_string(),
            provider_name: "openai".to_string(),
            provider_type: "api_key".to_string(),
            credentials: r#"{"api_key":"sk-test123"}"#.to_string(),
            project_id: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            is_active: 1,
        }
    }

    #[test]
    fn test_parse_credentials() {
        let provider = test_provider_credentials();
        let credentials = provider.parse_credentials().unwrap();

        match credentials {
            Credentials::ApiKey(api_key_creds) => {
                assert_eq!(api_key_creds.api_key, "sk-test123");
            }
            _ => panic!("Expected ApiKey credentials"),
        }
    }

    #[test]
    fn test_set_credentials() {
        let mut provider = test_provider_credentials();
        let new_credentials = Credentials::ApiKey(ApiKeyCredentials {
            api_key: "sk-newkey456".to_string(),
        });

        provider.set_credentials(&new_credentials).unwrap();
        assert_eq!(provider.credentials, r#"{"api_key":"sk-newkey456"}"#);
    }

    #[test]
    fn test_is_global() {
        let mut provider = test_provider_credentials();
        assert!(provider.is_global());

        provider.project_id = Some("project-123".to_string());
        assert!(!provider.is_global());
    }

    #[test]
    fn test_is_active_credential() {
        let mut provider = test_provider_credentials();
        assert!(provider.is_active_credential());

        provider.is_active = 0;
        assert!(!provider.is_active_credential());
    }

    #[test]
    fn test_new_provider_credentials_dto() {
        let dto = NewProviderCredentialsDTO {
            provider_name: "anthropic".to_string(),
            provider_type: "api_key".to_string(),
            credentials: Credentials::ApiKey(ApiKeyCredentials {
                api_key: "sk-ant-test".to_string(),
            }),
            project_id: Some("project-456".to_string()),
        };

        let db_insert = dto.to_db_insert().unwrap();
        assert_eq!(db_insert.provider_name, "anthropic");
        assert_eq!(db_insert.project_id, Some("project-456".to_string()));
        assert!(db_insert.credentials.contains("sk-ant-test"));
    }

    #[test]
    fn test_update_provider_credentials_dto() {
        let dto = UpdateProviderCredentialsDTO {
            credentials: Some(Credentials::ApiKey(ApiKeyCredentials {
                api_key: "sk-updated".to_string(),
            })),
            is_active: Some(false),
        };

        let db_update = dto.to_db_update().unwrap();
        assert!(db_update.credentials.is_some());
        assert_eq!(db_update.is_active, Some(0));
    }
}
