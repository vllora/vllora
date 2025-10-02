use langdb_core::metadata::error::DatabaseError;
use langdb_core::metadata::models::model::DbNewModel;
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::model::{ModelService, ModelServiceImpl};
use langdb_core::models::ModelMetadata;
use reqwest;
use std::collections::HashSet;

#[derive(Debug, thiserror::Error)]
pub enum ModelsLoadError {
    #[error("Failed to fetch models: {0}")]
    FetchError(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    DatabaseError(#[from] DatabaseError),
}

pub async fn fetch_and_store_models(
    db_pool: DbPool,
) -> Result<Vec<ModelMetadata>, ModelsLoadError> {
    // Fetch models from API
    let client = reqwest::Client::new();
    let models: Vec<ModelMetadata> = client
        .get("https://api.us-east-1.langdb.ai/pricing?include_parameters=true")
        .send()
        .await?
        .json()
        .await?;

    // Convert ModelMetadata to DbNewModel
    let db_models: Vec<DbNewModel> = models.iter().map(|m| DbNewModel::from(m.clone())).collect();

    // Store in database using ModelService
    let model_service = ModelServiceImpl::new(db_pool);
    model_service.insert_many(db_models)?;

    // Build set of identifiers from API response
    // Use (model_name, provider_info_id) as unique identifier
    let synced_model_identifiers: HashSet<(String, String)> = models
        .iter()
        .map(|m| (m.model.clone(), m.inference_provider.provider.to_string()))
        .collect();

    // Get all non-deleted global models from database
    let db_models = model_service.list(None)?;

    // Find models in DB but not in API response (these should be soft-deleted)
    let models_to_delete: Vec<String> = db_models
        .iter()
        .filter(|db_model| {
            let identifier = (db_model.model_name.clone(), db_model.provider_name.clone());
            !synced_model_identifiers.contains(&identifier)
        })
        .filter_map(|db_model| db_model.id.clone())
        .collect();

    // Soft delete obsolete models
    if !models_to_delete.is_empty() {
        model_service.mark_models_as_deleted(models_to_delete)?;
    }

    Ok(models)
}
