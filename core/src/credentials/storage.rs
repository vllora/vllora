use crate::credentials::{KeyStorage, KeyStorageError, ProviderCredentialsId};
use crate::metadata::models::provider::{DbInsertProviderCredentials, DbUpdateProviderCredentials};
use crate::metadata::pool::DbPool;
use crate::metadata::services::provider::{ProviderService, ProviderServiceImpl};

pub struct ProviderKeyResolver {
    provider_service: ProviderServiceImpl,
}

impl ProviderKeyResolver {
    pub fn new(db_pool: DbPool) -> Self {
        Self {
            provider_service: ProviderServiceImpl::new(db_pool),
        }
    }
}

#[async_trait::async_trait]
impl KeyStorage for ProviderKeyResolver {
    async fn insert_key(
        &self,
        key_id: ProviderCredentialsId,
        key: Option<String>,
    ) -> Result<(), KeyStorageError> {
        let provider_insert = DbInsertProviderCredentials::new(
            key_id.provider_name(),
            "api_key".to_string(), // Default type, can be enhanced later
            key.unwrap_or_default(),
            Some(key_id.project_id()),
        );

        self.provider_service
            .save_provider(provider_insert)
            .map_err(|e| KeyStorageError::StorageError(e.to_string()))?;

        Ok(())
    }

    async fn get_key(
        &self,
        key_id: ProviderCredentialsId,
    ) -> Result<Option<String>, KeyStorageError> {
        let provider_name = key_id.provider_name();
        let project_id = key_id.project_id();

        match self
            .provider_service
            .get_provider_credentials(&provider_name, Some(&project_id))
        {
            Ok(Some(creds)) if creds.is_active_credential() => {
                return Ok(Some(creds.credentials));
            }
            Ok(_) => {} // Not found or inactive, continue to next level
            Err(e) => {
                tracing::warn!(
                    "Error fetching project credentials for {}: {}",
                    provider_name,
                    e
                );
            }
        }

        Ok(None)
    }

    async fn get_batch_keys(
        &self,
        key_ids: Vec<ProviderCredentialsId>,
    ) -> Result<Vec<(ProviderCredentialsId, Option<String>)>, KeyStorageError> {
        let mut results = Vec::new();

        for key_id in key_ids {
            let key_value = self.get_key(key_id.clone()).await?;
            results.push((key_id, key_value));
        }

        Ok(results)
    }

    async fn update_key(
        &self,
        key_id: ProviderCredentialsId,
        key: Option<String>,
    ) -> Result<(), KeyStorageError> {
        let update = DbUpdateProviderCredentials {
            provider_name: None,
            provider_type: None,
            credentials: Some(key.unwrap_or_default()),
            updated_at: chrono::Utc::now().to_rfc3339(),
            is_active: None,
        };
        let provider_name = key_id.provider_name();
        let project_id = key_id.project_id();
        self.provider_service
            .update_provider(&provider_name, Some(&project_id), update)
            .map_err(|e| KeyStorageError::StorageError(e.to_string()))?;

        Ok(())
    }

    async fn delete_key(&self, key_id: ProviderCredentialsId) -> Result<(), KeyStorageError> {
        let provider_name = key_id.provider_name();
        let project_id = key_id.project_id();

        self.provider_service
            .delete_provider(&provider_name, Some(&project_id))
            .map_err(|e| KeyStorageError::StorageError(e.to_string()))?;

        Ok(())
    }
}
