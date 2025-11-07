use crate::metadata::error::DatabaseError;
use crate::metadata::models::providers::{DbInsertProvider, DbProvider, DbUpdateProvider};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::providers as p;
use crate::metadata::schema::providers::dsl::providers;
use diesel::dsl::count;
use diesel::BoolExpressionMethods;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::{QueryDsl, RunQueryDsl};
use uuid::Uuid;

pub trait ProviderService {
    fn new(db_pool: DbPool) -> Self;
    /// Get provider by ID
    fn get_provider_by_id(&self, provider_id: &str) -> Result<Option<ProviderInfo>, DatabaseError>;

    /// Get provider by name
    fn get_provider_by_name(
        &self,
        provider_name: &str,
    ) -> Result<Option<ProviderInfo>, DatabaseError>;

    /// List all active providers
    fn list_providers(&self) -> Result<Vec<ProviderInfo>, DatabaseError>;

    /// Create a new provider
    fn create_provider(&self, provider: DbInsertProvider) -> Result<(), DatabaseError>;

    /// Update an existing provider
    fn update_provider(
        &self,
        provider_id: &str,
        update: DbUpdateProvider,
    ) -> Result<(), DatabaseError>;

    /// Delete a provider (soft delete by setting is_active to 0)
    fn delete_provider(&self, provider_id: &str) -> Result<(), DatabaseError>;

    /// Check if provider exists
    fn provider_exists(&self, provider_name: &str) -> Result<bool, DatabaseError>;

    /// Get providers with their credential status for a project
    fn list_providers_with_credential_status(
        &self,
        project_id: Option<&Uuid>,
    ) -> Result<Vec<ProviderInfo>, DatabaseError>;
}

/// Information about a provider with credential status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub priority: i32,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    pub provider_type: String,
    pub has_credentials: bool,
}

impl From<DbProvider> for ProviderInfo {
    fn from(provider: DbProvider) -> Self {
        let provider_type = provider.get_provider_type();
        Self {
            id: provider.id,
            name: provider.provider_name,
            description: provider.description,
            endpoint: provider.endpoint,
            priority: provider.priority,
            privacy_policy_url: provider.privacy_policy_url,
            terms_of_service_url: provider.terms_of_service_url,
            provider_type,
            has_credentials: false, // Will be set separately
        }
    }
}

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

        let query = providers
            .filter(p::provider_name.eq(provider_name))
            .filter(p::is_active.eq(1));

        Ok(query
            .first::<DbProvider>(&mut conn)
            .optional()?
            .map(|p| p.into()))
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
            let has_credentials = if let Some(pid) = project_id {
                // Check for project-specific credentials first, then global
                crate::metadata::schema::provider_credentials::dsl::provider_credentials
                    .select(count(crate::metadata::schema::provider_credentials::id))
                    .filter(crate::metadata::schema::provider_credentials::provider_name.eq(&provider_info.name))
                    .filter(crate::metadata::schema::provider_credentials::project_id.eq(pid.to_string()).or(crate::metadata::schema::provider_credentials::project_id.is_null()))
                    .filter(crate::metadata::schema::provider_credentials::is_active.eq(1))
                    .first::<i64>(&mut conn)? > 0
            } else {
                // Check for global credentials only
                crate::metadata::schema::provider_credentials::dsl::provider_credentials
                    .select(count(crate::metadata::schema::provider_credentials::id))
                    .filter(
                        crate::metadata::schema::provider_credentials::provider_name
                            .eq(&provider_info.name),
                    )
                    .filter(crate::metadata::schema::provider_credentials::project_id.is_null())
                    .filter(crate::metadata::schema::provider_credentials::is_active.eq(1))
                    .first::<i64>(&mut conn)?
                    > 0
            };

            provider_info.has_credentials = has_credentials;
            result.push(provider_info);
        }

        Ok(result)
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
