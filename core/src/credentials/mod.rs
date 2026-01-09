mod storage;

use async_trait::async_trait;
pub use storage::ProviderKeyResolver;
use vllora_llm::types::models::ModelMetadata;

/// Error type for key storage operations
#[derive(Debug, thiserror::Error)]
pub enum KeyStorageError {
    #[error("Key not found")]
    KeyNotFound,
    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Trait defining operations for storing and retrieving API keys
#[async_trait]
pub trait KeyStorage: Send + Sync {
    /// Store a key with the given identifier
    async fn insert_key(
        &self,
        key_id: ProviderCredentialsId,
        key: Option<String>,
    ) -> Result<(), KeyStorageError>;

    /// Retrieve a key by its identifier
    async fn get_key(
        &self,
        key_id: ProviderCredentialsId,
    ) -> Result<Option<String>, KeyStorageError>;

    async fn get_batch_keys(
        &self,
        key_ids: Vec<ProviderCredentialsId>,
    ) -> Result<Vec<(ProviderCredentialsId, Option<String>)>, KeyStorageError>;

    async fn update_key(
        &self,
        key_id: ProviderCredentialsId,
        key: Option<String>,
    ) -> Result<(), KeyStorageError>;

    async fn delete_key(&self, key_id: ProviderCredentialsId) -> Result<(), KeyStorageError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProviderCredentialsId {
    value: String,
    project_slug: Option<String>,
    provider_name: String,
    #[allow(dead_code)]
    tenant_name: String,
}

impl ProviderCredentialsId {
    pub fn new(tenant_name: String, provider_name: String, project_slug: Option<String>) -> Self {
        let value = match project_slug.as_ref() {
            Some(project_slug) => format!("{tenant_name}_{provider_name}_{project_slug}"),
            None => format!("{tenant_name}_{provider_name}"),
        };
        Self {
            value,
            project_slug,
            provider_name,
            tenant_name,
        }
    }

    pub fn value(&self) -> String {
        self.value.clone()
    }

    pub fn project_slug(&self) -> Option<String> {
        self.project_slug.clone()
    }

    pub fn provider_name(&self) -> String {
        self.provider_name.clone()
    }

    pub fn from_string(value: String) -> Self {
        Self {
            value: value.clone(),
            tenant_name: "".to_string(),
            provider_name: "".to_string(),
            project_slug: None,
        }
    }
}

/// Helper function to construct a key ID for provider credentials
pub fn construct_key_id(
    tenant_name: &str,
    provider_name: &str,
    project_slug: &str,
) -> ProviderCredentialsId {
    ProviderCredentialsId::new(
        tenant_name.to_string(),
        provider_name.to_string(),
        Some(project_slug.to_string()),
    )
}

pub struct GatewayCredentials {}

impl GatewayCredentials {
    pub async fn extract_key_from_model<T: serde::de::DeserializeOwned>(
        model: &ModelMetadata,
        project_slug: &str,
        tenant_name: &str,
        key_storage: &dyn KeyStorage,
    ) -> Result<Option<T>, KeyStorageError> {
        let provider_str: &str = &model
            .inference_provider
            .provider
            .to_string()
            .replace("\"", "")
            .replace("\\", "");

        Self::extract_key(provider_str, project_slug, tenant_name, key_storage).await
    }

    pub async fn extract_key<T: serde::de::DeserializeOwned>(
        provider_name: &str,
        project_slug: &str,
        tenant_name: &str,
        key_storage: &dyn KeyStorage,
    ) -> Result<Option<T>, KeyStorageError> {
        let key_id = construct_key_id(tenant_name, provider_name, project_slug);
        let key_result = key_storage.get_key(key_id).await;
        match key_result {
            Ok(Some(key)) => {
                let k = serde_json::from_str::<T>(&key);
                match k {
                    Ok(k) => Ok(Some(k)),
                    Err(e) => Err(KeyStorageError::StorageError(e.to_string())),
                }
            }
            Ok(None) | Err(KeyStorageError::KeyNotFound) => Ok(None),
            Err(e) => {
                tracing::error!("Failed to get key: {}", e);
                Ok(None)
            }
        }
    }
}
