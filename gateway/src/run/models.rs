use langdb_core::metadata::error::DatabaseError;
use langdb_core::metadata::models::model::DbNewModel;
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::model::{ModelService, ModelServiceImpl};
use langdb_core::models::ModelMetadata;
use langdb_core::types::LANGDB_API_URL;
use reqwest;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ModelsLoadError {
    #[error("Failed to fetch models: {0}")]
    FetchError(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    DatabaseError(#[from] DatabaseError),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub async fn fetch_and_store_models(
    db_pool: DbPool,
) -> Result<Vec<ModelMetadata>, ModelsLoadError> {
    let langdb_api_url = std::env::var("LANGDB_API_URL")
        .ok()
        .unwrap_or(LANGDB_API_URL.to_string())
        .replace("/v1", "");

    // Fetch models from API
    let client = reqwest::Client::new();
    let models: Vec<ModelMetadata> = client
        .get(format!(
            "{langdb_api_url}/pricing?include_parameters=true&include_benchmark=true"
        ))
        .send()
        .await?
        .json()
        .await?;

    // Build set of identifiers from API response
    // Use (model_name, provider_info_id) as unique identifier
    let mut synced_model_identifiers: HashSet<(String, String)> = models
        .iter()
        .map(|m| (m.model.clone(), m.inference_provider.provider.to_string()))
        .collect();

    // Convert ModelMetadata to DbNewModel
    let mut db_models: Vec<DbNewModel> =
        models.iter().map(|m| DbNewModel::from(m.clone())).collect();

    let mut langdb_models = HashMap::<String, DbNewModel>::new();
    for model in &db_models {
        let mut new_model = model.clone();
        if langdb_models.contains_key(&new_model.model_name) {
            continue;
        }
        synced_model_identifiers.insert((new_model.model_name.clone(), "langdb".to_string()));

        new_model.id = Some(Uuid::new_v4().to_string());
        new_model.endpoint = Some(format!("{langdb_api_url}/v1"));
        new_model.provider_name = "langdb".to_string();
        new_model.model_name_in_provider = Some(new_model.model_name.clone());
        langdb_models.insert(new_model.model_name.clone(), new_model);
    }

    db_models.extend(langdb_models.values().cloned());

    // Store in database using ModelService
    let model_service = ModelServiceImpl::new(db_pool);
    model_service.insert_many(db_models)?;

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

/// Fetches models from API and saves them as JSON to a file
pub async fn fetch_and_save_models_json(output_path: &Path) -> Result<Vec<ModelMetadata>, ModelsLoadError> {
    let langdb_api_url = std::env::var("LANGDB_API_URL")
        .ok()
        .unwrap_or(LANGDB_API_URL.to_string())
        .replace("/v1", "");

    // Fetch models from API
    let client = reqwest::Client::new();
    let models: Vec<ModelMetadata> = client
        .get(format!(
            "{langdb_api_url}/pricing?include_parameters=true&include_benchmark=true"
        ))
        .send()
        .await?
        .json()
        .await?;

    // Serialize to JSON and save to file
    let json_content = serde_json::to_string_pretty(&models)?;
    
    fs::write(output_path, json_content)?;

    println!("Successfully saved {} models to {}", models.len(), output_path.display());
    Ok(models)
}

/// Loads models from embedded JSON data
pub fn load_models_from_json(json_content: &str) -> Result<Vec<ModelMetadata>, ModelsLoadError> {
    let models: Vec<ModelMetadata> = serde_json::from_str(json_content)?;
    Ok(models)
}
