use crate::metadata::error::DatabaseError;
use crate::metadata::models::provider::{DbInsertProvider, DbUpdateProvider};
use crate::metadata::pool::DbPool;
use crate::types::metadata::provider::ProviderInfo;
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
