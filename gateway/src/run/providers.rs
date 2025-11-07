use reqwest;
use std::collections::HashSet;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::providers::DbInsertProvider;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::providers::{ProviderService, ProvidersServiceImpl};
use vllora_core::types::LANGDB_API_URL;

#[derive(Debug, thiserror::Error)]
pub enum ProvidersLoadError {
    #[error("Failed to fetch providers: {0}")]
    FetchError(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    DatabaseError(#[from] DatabaseError),
}

/// Provider data structure from LangDB API
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LangDBProvider {
    pub id: String,
    pub provider_name: String,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub priority: i32,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
}

impl From<LangDBProvider> for DbInsertProvider {
    fn from(provider: LangDBProvider) -> Self {
        DbInsertProvider::new(
            provider.id,
            provider.provider_name,
            provider.description,
            provider.endpoint,
            provider.priority,
            provider.privacy_policy_url,
            provider.terms_of_service_url,
        )
    }
}

pub async fn fetch_and_store_providers(
    db_pool: DbPool,
) -> Result<Vec<LangDBProvider>, ProvidersLoadError> {
    // Fetch providers from LangDB API
    let langdb_api_url = std::env::var("LANGDB_API_URL")
        .ok()
        .unwrap_or(LANGDB_API_URL.to_string())
        .replace("/v1", "");

    let client = reqwest::Client::new();
    let mut providers: Vec<LangDBProvider> = client
        .get(format!("{langdb_api_url}/providers"))
        .send()
        .await?
        .json()
        .await?;

    providers.push(LangDBProvider {
        id: uuid::Uuid::new_v4().to_string(),
        provider_name: "langdb".to_string(),
        description: Some("LangDB".to_string()),
        endpoint: format!("{langdb_api_url}/v1").into(),
        priority: 100,
        privacy_policy_url: None,
        terms_of_service_url: None,
    });

    // Convert LangDBProvider to DbInsertProvider
    let db_providers: Vec<DbInsertProvider> = providers
        .iter()
        .map(|p| DbInsertProvider::from(p.clone()))
        .collect();

    // Store in database using ProviderService
    let provider_service = ProvidersServiceImpl::new(db_pool.clone());

    // Get existing providers to avoid duplicates
    let existing_providers = provider_service.list_providers()?;
    let existing_provider_names: HashSet<String> =
        existing_providers.iter().map(|p| p.name.clone()).collect();

    // Insert only new providers
    let mut inserted_count = 0;
    for db_provider in db_providers {
        if !existing_provider_names.contains(&db_provider.provider_name) {
            provider_service.create_provider(db_provider)?;
            inserted_count += 1;
        }
    }

    tracing::info!(
        "Successfully processed {} providers (inserted {} new ones)",
        providers.len(),
        inserted_count
    );

    // Build set of identifiers from API response
    let synced_provider_names: HashSet<String> =
        providers.iter().map(|p| p.provider_name.clone()).collect();

    // Get all active providers from database
    let db_providers = provider_service.list_providers()?;

    // Find providers in DB but not in API response (these should be deactivated)
    let providers_to_deactivate: Vec<String> = db_providers
        .iter()
        .filter(|db_provider| !synced_provider_names.contains(&db_provider.name))
        .map(|db_provider| db_provider.id.clone())
        .collect();

    // Deactivate obsolete providers
    let deactivate_count = providers_to_deactivate.len();
    if !providers_to_deactivate.is_empty() {
        for provider_id in providers_to_deactivate {
            provider_service.delete_provider(&provider_id)?;
        }
        tracing::info!("Deactivated {} obsolete providers", deactivate_count);
    }

    Ok(providers)
}

/// Main function to sync providers from LangDB API with fallback to hardcoded data
pub async fn sync_providers(db_pool: DbPool) -> Result<(), ProvidersLoadError> {
    // Try to fetch from API first
    match fetch_and_store_providers(db_pool.clone()).await {
        Ok(providers) => {
            tracing::info!("Successfully synced {} providers", providers.len());
        }
        Err(e) => {
            tracing::warn!("Failed to sync providers from API: {}.", e);
        }
    }

    Ok(())
}
