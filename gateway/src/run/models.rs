use directories::BaseDirs;
use langdb_core::models::ModelDefinition;
use reqwest;
use serde_yaml;
use std::fs;

#[derive(Debug, thiserror::Error)]
pub enum ModelsLoadError {
    #[error("Failed to fetch models: {0}")]
    FetchError(#[from] reqwest::Error),
    #[error("Failed to store models: {0}")]
    StoreError(#[from] std::io::Error),
    #[error("Failed to parse models config: {0}")]
    ParseError(#[from] serde_yaml::Error),
    #[error("Could not determine home directory")]
    NoHomeDir,
}

/// Load models configuration from the filesystem, fetching it first if it doesn't exist
pub async fn load_models(force_update: bool) -> Result<Vec<ModelDefinition>, ModelsLoadError> {
    let models_yaml = if force_update {
        // Force fetch and store new models
        fetch_and_store_models().await?
    } else {
        get_models_path()?
    };
    let models: Vec<ModelDefinition> = serde_yaml::from_str(&models_yaml)?;
    Ok(models)
}

pub async fn fetch_and_store_models() -> Result<String, ModelsLoadError> {
    // Create .langdb directory in home folder
    let base_dirs = BaseDirs::new().ok_or(ModelsLoadError::NoHomeDir)?;
    let langdb_dir = base_dirs.home_dir().join(".langdb");
    fs::create_dir_all(&langdb_dir)?;

    // Fetch models from API
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.us-east-1.langdb.ai/pricing")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    // Convert to YAML
    let yaml = serde_yaml::to_string(&response)?;

    // Store in models.yaml
    let models_path = langdb_dir.join("models.yaml");
    fs::write(&models_path, &yaml)?;

    Ok(yaml)
}

pub fn get_models_path() -> Result<String, std::io::Error> {
    if let Some(base_dirs) = BaseDirs::new() {
        let user_models = base_dirs.home_dir().join(".langdb").join("models.yaml");
        if user_models.exists() {
            return std::fs::read_to_string(user_models);
        }
    }
    Ok(include_str!("../../../core/models.yaml").to_string())
}
