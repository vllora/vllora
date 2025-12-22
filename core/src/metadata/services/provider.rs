use crate::metadata::error::DatabaseError;
use crate::metadata::models::provider::{DbInsertProvider, DbProvider, DbUpdateProvider};
use crate::metadata::models::provider_credential::DbProviderCredentials;
use crate::metadata::pool::DbPool;
use crate::metadata::schema::providers as p;
use crate::metadata::schema::providers::dsl::providers;
use crate::types::metadata::provider::ProviderInfo;
use crate::types::metadata::services::provider::ProviderService;
use diesel::dsl::count;
use diesel::BoolExpressionMethods;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::{QueryDsl, RunQueryDsl};
use uuid::Uuid;
use vllora_llm::types::credentials::Credentials;

pub struct ProvidersServiceImpl {
    db_pool: DbPool,
}

impl ProviderService for ProvidersServiceImpl {
    fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    fn get_provider_by_id(&self, provider_id: &str) -> Result<Option<ProviderInfo>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let query = providers
            .filter(p::id.eq(provider_id))
            .filter(p::is_active.eq(1));

        Ok(query
            .first::<DbProvider>(&mut conn)
            .optional()?
            .map(|p| p.into()))
    }

    fn get_provider_by_name(
        &self,
        provider_name: &str,
    ) -> Result<Option<ProviderInfo>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let query = providers.filter(p::provider_name.eq(provider_name));

        Ok(query
            .first::<DbProvider>(&mut conn)
            .optional()?
            .map_or_else(
                || {
                    tracing::warn!("provider not found");
                    None
                },
                |p| Some(p.into()),
            ))
    }

    fn list_providers(&self) -> Result<Vec<ProviderInfo>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let query = providers
            .filter(p::is_active.eq(1))
            .order(p::priority.desc())
            .then_order_by(p::provider_name.asc());

        Ok(query
            .load::<DbProvider>(&mut conn)?
            .into_iter()
            .map(|p| p.into())
            .collect())
    }

    fn create_provider(&self, provider: DbInsertProvider) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;

        diesel::insert_into(providers)
            .values(provider)
            .execute(&mut conn)?;

        Ok(())
    }

    fn update_provider(
        &self,
        provider_id: &str,
        update: DbUpdateProvider,
    ) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;

        diesel::update(providers.filter(p::id.eq(provider_id)))
            .set(update)
            .execute(&mut conn)?;

        Ok(())
    }

    fn delete_provider(&self, provider_id: &str) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;

        diesel::update(providers.filter(p::id.eq(provider_id)))
            .set(p::is_active.eq(0))
            .execute(&mut conn)?;

        Ok(())
    }

    fn provider_exists(&self, provider_name: &str) -> Result<bool, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        Ok(providers
            .select(count(p::id))
            .filter(p::provider_name.eq(provider_name))
            .filter(p::is_active.eq(1))
            .first::<i64>(&mut conn)?
            > 0)
    }

    fn list_providers_with_credential_status(
        &self,
        project_id: Option<&Uuid>,
    ) -> Result<Vec<ProviderInfo>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Get all active providers
        let active_providers = providers
            .filter(p::is_active.eq(1))
            .order(p::priority.desc())
            .then_order_by(p::provider_name.asc())
            .load::<DbProvider>(&mut conn)?;

        // Check which providers have credentials configured
        let mut result = Vec::new();
        for provider in active_providers {
            let mut provider_info: ProviderInfo = provider.into();

            // Check if credentials exist for this provider
            let credentials = if let Some(pid) = project_id {
                // Check for project-specific credentials first, then global
                crate::metadata::schema::provider_credentials::dsl::provider_credentials
                    .filter(crate::metadata::schema::provider_credentials::provider_name.eq(&provider_info.name))
                    .filter(crate::metadata::schema::provider_credentials::project_id.eq(pid.to_string()).or(crate::metadata::schema::provider_credentials::project_id.is_null()))
                    .filter(crate::metadata::schema::provider_credentials::is_active.eq(1))
                    .first::<DbProviderCredentials>(&mut conn).optional()?
            } else {
                // Check for global credentials only
                crate::metadata::schema::provider_credentials::dsl::provider_credentials
                    .filter(
                        crate::metadata::schema::provider_credentials::provider_name
                            .eq(&provider_info.name),
                    )
                    .filter(crate::metadata::schema::provider_credentials::project_id.is_null())
                    .filter(crate::metadata::schema::provider_credentials::is_active.eq(1))
                    .first::<DbProviderCredentials>(&mut conn)
                    .optional()?
            };

            provider_info.has_credentials = credentials.is_some();
            provider_info.custom_endpoint = credentials.and_then(|c| match c.parse_credentials() {
                Ok(Credentials::ApiKeyWithEndpoint { endpoint, .. }) => Some(endpoint),
                _ => None,
            });
            result.push(provider_info);
        }

        Ok(result)
    }

    fn is_provider_custom(&self, provider_id: &str) -> Result<Option<bool>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let is_custom: Option<i32> = providers
            .filter(p::id.eq(provider_id))
            .select(p::is_custom)
            .first(&mut conn)
            .optional()?;

        Ok(is_custom.map(|val| val == 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_info_from_db_provider() {
        let db_provider = DbProvider {
            id: "test-id".to_string(),
            provider_name: "openai".to_string(),
            description: Some("Test description".to_string()),
            endpoint: None,
            priority: 100,
            privacy_policy_url: Some("https://openai.com/privacy".to_string()),
            terms_of_service_url: Some("https://openai.com/terms".to_string()),
            created_at: "2023-01-01T00:00:00Z".to_string(),
            updated_at: "2023-01-01T00:00:00Z".to_string(),
            is_active: 1,
            custom_inference_api_type: None,
            is_custom: 0,
        };

        let provider_info: ProviderInfo = db_provider.into();

        assert_eq!(provider_info.id, "test-id");
        assert_eq!(provider_info.name, "openai");
        assert_eq!(
            provider_info.description,
            Some("Test description".to_string())
        );
        assert_eq!(provider_info.priority, 100);
        assert_eq!(provider_info.provider_type, "api_key");
        assert_eq!(provider_info.has_credentials, false); // Default value
    }
}
