use crate::metadata::error::DatabaseError;
use crate::metadata::models::provider_credential::{
    DbInsertProviderCredentials, DbProviderCredentials, DbUpdateProviderCredentials,
};
use crate::types::metadata::provider_credential::ProviderCredentialsInfo;
use std::collections::HashMap;
use vllora_llm::types::credentials::Credentials;

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
        available_models: &[vllora_llm::types::models::ModelMetadata],
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
