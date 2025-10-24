use crate::metadata::error::DatabaseError;
use crate::metadata::models::provider_credentials::{
    DbInsertProviderCredentials, DbProviderCredentials, DbUpdateProviderCredentials,
};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::provider_credentials as pc;
use crate::metadata::schema::provider_credentials::dsl::provider_credentials;
use crate::types::credentials::Credentials;
use diesel::dsl::count;
use diesel::BoolExpressionMethods;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::{QueryDsl, RunQueryDsl};
use std::collections::HashMap;

pub trait ProviderCredentialsService {
    /// Get provider credentials by provider name and optional project ID
    fn get_provider_credentials(
        &self,
        provider_name: &str,
        project_id: Option<&str>,
    ) -> Result<Option<DbProviderCredentials>, DatabaseError>;

    /// Save or update provider credentials
    fn save_provider(&self, provider: DbInsertProviderCredentials) -> Result<(), DatabaseError>;

    /// Update existing provider credentials
    fn update_provider(
        &self,
        provider_name: &str,
        project_id: Option<&str>,
        update: DbUpdateProviderCredentials,
    ) -> Result<(), DatabaseError>;

    /// Delete provider credentials
    fn delete_provider(
        &self,
        provider_name: &str,
        project_id: Option<&str>,
    ) -> Result<(), DatabaseError>;

    /// List all providers with their credential status
    fn list_providers(
        &self,
        project_id: Option<&str>,
    ) -> Result<Vec<ProviderCredentialsInfo>, DatabaseError>;

    /// List all available providers from models with their credential status
    fn list_available_providers(
        &self,
        project_id: Option<&str>,
        available_models: &[crate::models::ModelMetadata],
    ) -> Result<Vec<ProviderCredentialsInfo>, DatabaseError>;

    /// Check if provider has credentials configured
    fn has_provider_credentials(
        &self,
        provider_name: &str,
        project_id: Option<&str>,
    ) -> Result<bool, DatabaseError>;

    /// Get all provider credentials for a project (including global fallbacks)
    fn get_all_provider_credentials(
        &self,
        project_id: Option<&str>,
    ) -> Result<HashMap<String, Credentials>, DatabaseError>;
}

/// Information about a provider's credential status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderCredentialsInfo {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub has_credentials: bool,
}

impl ProviderCredentialsService for ProviderCredentialsServiceImpl {
    fn get_provider_credentials(
        &self,
        provider_name_param: &str,
        project_id_param: Option<&str>,
    ) -> Result<Option<DbProviderCredentials>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let query = provider_credentials
            .filter(pc::provider_name.eq(provider_name_param))
            .filter(pc::project_id.eq(project_id_param))
            .filter(pc::is_active.eq(1));

        Ok(query.first(&mut conn).optional()?)
    }

    fn save_provider(&self, provider: DbInsertProviderCredentials) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // First, check if a record already exists for this provider/project combination
        let existing: Option<DbProviderCredentials> = provider_credentials
            .filter(pc::provider_name.eq(&provider.provider_name))
            .filter(pc::project_id.eq(&provider.project_id))
            .filter(pc::is_active.eq(1))
            .first(&mut conn)
            .optional()?;

        if let Some(existing) = existing {
            // Update existing record
            diesel::update(provider_credentials.filter(pc::id.eq(&existing.id)))
                .set(&DbUpdateProviderCredentials {
                    provider_name: None,
                    provider_type: None,
                    credentials: Some(provider.credentials),
                    updated_at: provider.updated_at,
                    is_active: None,
                })
                .execute(&mut conn)?;
        } else {
            // Insert new record
            diesel::insert_into(provider_credentials)
                .values(&provider)
                .execute(&mut conn)?;
        }

        Ok(())
    }

    fn update_provider(
        &self,
        provider_name_param: &str,
        project_id_param: Option<&str>,
        update: DbUpdateProviderCredentials,
    ) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let query = provider_credentials
            .filter(pc::provider_name.eq(provider_name_param))
            .filter(pc::project_id.eq(project_id_param));

        diesel::update(query).set(&update).execute(&mut conn)?;

        Ok(())
    }

    fn delete_provider(
        &self,
        provider_name_param: &str,
        project_id_param: Option<&str>,
    ) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Soft delete by setting is_active to 0
        let query = provider_credentials
            .filter(pc::provider_name.eq(provider_name_param))
            .filter(pc::project_id.eq(project_id_param));

        diesel::update(query)
            .set(pc::is_active.eq(0))
            .execute(&mut conn)?;

        Ok(())
    }

    fn list_providers(
        &self,
        project_id_param: Option<&str>,
    ) -> Result<Vec<ProviderCredentialsInfo>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Get all unique provider names that have credentials
        let providers_with_creds = if let Some(pid) = project_id_param {
            provider_credentials
                .select((pc::provider_name, pc::provider_type))
                .filter(pc::is_active.eq(1))
                .filter(pc::project_id.eq(pid).or(pc::project_id.is_null()))
                .distinct()
                .load::<(String, String)>(&mut conn)?
        } else {
            provider_credentials
                .select((pc::provider_name, pc::provider_type))
                .filter(pc::is_active.eq(1))
                .filter(pc::project_id.is_null())
                .distinct()
                .load::<(String, String)>(&mut conn)?
        };

        let mut providers = Vec::new();

        for (provider_name_str, provider_type_str) in providers_with_creds {
            providers.push(ProviderCredentialsInfo {
                id: format!(
                    "{}-{}",
                    provider_name_str,
                    project_id_param.unwrap_or("global")
                ),
                name: provider_name_str.clone(),
                provider_type: provider_type_str,
                has_credentials: true,
            });
        }

        Ok(providers)
    }

    fn has_provider_credentials(
        &self,
        provider_name_param: &str,
        project_id_param: Option<&str>,
    ) -> Result<bool, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        Ok(provider_credentials
            .select(count(pc::id))
            .filter(pc::provider_name.eq(provider_name_param))
            .filter(pc::project_id.eq(project_id_param))
            .filter(pc::is_active.eq(1))
            .first::<i64>(&mut conn)?
            > 0)
    }

    fn get_all_provider_credentials(
        &self,
        project_id_param: Option<&str>,
    ) -> Result<HashMap<String, Credentials>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Get all active credentials for the project (including global ones)
        let credentials_records = if let Some(pid) = project_id_param {
            provider_credentials
                .filter(pc::is_active.eq(1))
                .filter(pc::project_id.eq(pid).or(pc::project_id.is_null()))
                .load::<DbProviderCredentials>(&mut conn)?
        } else {
            provider_credentials
                .filter(pc::is_active.eq(1))
                .filter(pc::project_id.is_null())
                .load::<DbProviderCredentials>(&mut conn)?
        };

        let mut result = HashMap::new();

        for record in credentials_records {
            match record.parse_credentials() {
                Ok(parsed_credentials) => {
                    result.insert(record.provider_name, parsed_credentials);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse credentials for provider {}: {}",
                        record.provider_name,
                        e
                    );
                }
            }
        }

        Ok(result)
    }

    fn list_available_providers(
        &self,
        project_id_param: Option<&str>,
        available_models: &[crate::models::ModelMetadata],
    ) -> Result<Vec<ProviderCredentialsInfo>, DatabaseError> {
        // Extract unique providers from available models
        let mut unique_providers = std::collections::HashSet::new();

        for model in available_models {
            let provider_name = model.inference_provider.provider.to_string().to_lowercase();
            unique_providers.insert(provider_name);
        }

        // Convert to sorted vector and check credential status for each provider
        let mut providers = Vec::new();
        for provider_name in unique_providers {
            let has_credentials =
                self.has_provider_credentials(&provider_name, project_id_param)?;

            providers.push(ProviderCredentialsInfo {
                id: format!("provider-{}", provider_name),
                name: provider_name.clone(),
                provider_type: get_provider_type(&provider_name),
                has_credentials,
            });
        }

        // Sort providers by name for consistent ordering
        providers.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(providers)
    }
}

/// Get the provider type based on provider name
fn get_provider_type(provider_name: &str) -> String {
    match provider_name.to_lowercase().as_str() {
        "openai" => "api_key".to_string(),
        "anthropic" => "api_key".to_string(),
        "gemini" => "api_key".to_string(),
        "bedrock" => "aws".to_string(),
        "vertex-ai" => "vertex".to_string(),
        "langdb" => "langdb".to_string(),
        _ => "api_key".to_string(), // Default to api_key for unknown providers
    }
}

pub struct ProviderCredentialsServiceImpl {
    db_pool: DbPool,
}

impl ProviderCredentialsServiceImpl {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::models::provider_credentials::NewProviderCredentialsDTO;
    use crate::metadata::models::provider_credentials::UpdateProviderCredentialsDTO;
    use crate::metadata::test_utils::setup_test_database;
    use crate::types::credentials::{ApiKeyCredentials, Credentials};

    fn create_test_provider_service() -> ProviderCredentialsServiceImpl {
        let db_pool = setup_test_database();
        ProviderCredentialsServiceImpl::new(db_pool)
    }

    #[test]
    fn test_save_and_get_provider_credentials() {
        let service = create_test_provider_service();

        let credentials = Credentials::ApiKey(ApiKeyCredentials {
            api_key: "sk-test123".to_string(),
        });

        let dto = NewProviderCredentialsDTO {
            provider_name: "openai".to_string(),
            provider_type: "api_key".to_string(),
            credentials,
            project_id: None,
        };

        let insert = dto.to_db_insert().unwrap();
        service.save_provider(insert).unwrap();

        let retrieved = service
            .get_provider_credentials("openai", None)
            .unwrap()
            .unwrap();

        assert_eq!(retrieved.provider_name, "openai");
        assert_eq!(retrieved.is_global(), true);
    }

    #[test]
    fn test_save_and_get_project_specific_credentials() {
        let service = create_test_provider_service();

        let credentials = Credentials::ApiKey(ApiKeyCredentials {
            api_key: "sk-project123".to_string(),
        });

        let dto = NewProviderCredentialsDTO {
            provider_name: "anthropic".to_string(),
            provider_type: "api_key".to_string(),
            credentials,
            project_id: Some("project-123".to_string()),
        };

        let insert = dto.to_db_insert().unwrap();
        service.save_provider(insert).unwrap();

        let retrieved = service
            .get_provider_credentials("anthropic", Some("project-123"))
            .unwrap()
            .unwrap();

        assert_eq!(retrieved.provider_name, "anthropic");
        assert_eq!(retrieved.project_id, Some("project-123".to_string()));
        assert_eq!(retrieved.is_global(), false);
    }

    #[test]
    fn test_update_provider_credentials() {
        let service = create_test_provider_service();

        // First, save some credentials
        let credentials = Credentials::ApiKey(ApiKeyCredentials {
            api_key: "sk-old".to_string(),
        });

        let dto = NewProviderCredentialsDTO {
            provider_name: "openai".to_string(),
            provider_type: "api_key".to_string(),
            credentials,
            project_id: None,
        };

        let insert = dto.to_db_insert().unwrap();
        service.save_provider(insert).unwrap();

        // Now update them
        let new_credentials = Credentials::ApiKey(ApiKeyCredentials {
            api_key: "sk-new".to_string(),
        });

        let update_dto = UpdateProviderCredentialsDTO {
            credentials: Some(new_credentials),
            is_active: None,
        };

        let update = update_dto.to_db_update().unwrap();
        service.update_provider("openai", None, update).unwrap();

        // Verify the update
        let retrieved = service
            .get_provider_credentials("openai", None)
            .unwrap()
            .unwrap();

        match retrieved.parse_credentials().unwrap() {
            Credentials::ApiKey(api_key_creds) => {
                assert_eq!(api_key_creds.api_key, "sk-new");
            }
            _ => panic!("Expected ApiKey credentials"),
        }
    }

    #[test]
    fn test_delete_provider_credentials() {
        let service = create_test_provider_service();

        // First, save some credentials
        let credentials = Credentials::ApiKey(ApiKeyCredentials {
            api_key: "sk-test".to_string(),
        });

        let dto = NewProviderCredentialsDTO {
            provider_name: "openai".to_string(),
            provider_type: "api_key".to_string(),
            credentials,
            project_id: None,
        };

        let insert = dto.to_db_insert().unwrap();
        service.save_provider(insert).unwrap();

        // Verify they exist
        assert!(service.has_provider_credentials("openai", None).unwrap());

        // Delete them
        service.delete_provider("openai", None).unwrap();

        // Verify they're gone
        assert!(!service.has_provider_credentials("openai", None).unwrap());
    }

    #[test]
    fn test_list_providers() {
        let service = create_test_provider_service();

        // Save some credentials
        let credentials1 = Credentials::ApiKey(ApiKeyCredentials {
            api_key: "sk-openai".to_string(),
        });

        let dto1 = NewProviderCredentialsDTO {
            provider_name: "openai".to_string(),
            provider_type: "api_key".to_string(),
            credentials: credentials1,
            project_id: None,
        };

        let credentials2 = Credentials::ApiKey(ApiKeyCredentials {
            api_key: "sk-anthropic".to_string(),
        });

        let dto2 = NewProviderCredentialsDTO {
            provider_name: "anthropic".to_string(),
            provider_type: "api_key".to_string(),
            credentials: credentials2,
            project_id: Some("project-123".to_string()),
        };

        service.save_provider(dto1.to_db_insert().unwrap()).unwrap();
        service.save_provider(dto2.to_db_insert().unwrap()).unwrap();

        // List global providers
        let global_providers = service.list_providers(None).unwrap();
        assert_eq!(global_providers.len(), 1);
        assert_eq!(global_providers[0].name, "openai");

        // List project providers
        let project_providers = service.list_providers(Some("project-123")).unwrap();
        assert_eq!(project_providers.len(), 1);
        assert_eq!(project_providers[0].name, "anthropic");
    }
}
