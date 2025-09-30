use directories::BaseDirs;
use langdb_core::models::ModelMetadata;
use reqwest;
use serde_yaml;
use std::collections::HashSet;
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
pub async fn load_models(force_update: bool) -> Result<Vec<ModelMetadata>, ModelsLoadError> {
    let models_yaml = if force_update {
        // Force fetch and store new models
        fetch_and_store_models().await?
    } else {
        get_models_path()?
    };
    let models: Vec<ModelMetadata> = serde_yaml::from_str(&models_yaml)?;
    Ok(models)
}

pub async fn load_models_filtered(
    force_update: bool,
    configured_providers: Option<&HashSet<String>>,
) -> Result<Vec<ModelMetadata>, ModelsLoadError> {
    let mut models = load_models(force_update).await?;
    
    if let Some(providers) = configured_providers {
        models = filter_models_by_providers(models, providers);
    }
    
    Ok(models)
}

pub fn filter_models_by_providers(
    models: Vec<ModelMetadata>,
    configured_providers: &HashSet<String>,
) -> Vec<ModelMetadata> {
    models
        .into_iter()
        .filter(|model| {
            // Use the actual provider name from the model's inference_provider
            let provider_name = model.inference_provider.provider.to_string();
            configured_providers.contains(&provider_name)
        })
        .collect()
}

pub fn get_configured_providers(config_path: &str) -> Result<HashSet<String>, ModelsLoadError> {
    let config_content = std::fs::read_to_string(config_path)
        .map_err(|_| ModelsLoadError::StoreError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Config file not found: {}", config_path)
        )))?;
    
    let config: serde_yaml::Value = serde_yaml::from_str(&config_content)?;
    
    let mut providers = HashSet::new();
    
    if let Some(providers_config) = config.get("providers") {
        if let Some(providers_map) = providers_config.as_mapping() {
            for (provider_name, provider_config) in providers_map {
                if let Some(provider_name_str) = provider_name.as_str() {
                    if let Some(api_key) = provider_config.get("api_key") {
                        if let Some(api_key_str) = api_key.as_str() {
                            if !api_key_str.is_empty() && !api_key_str.starts_with("{{") {
                                providers.insert(provider_name_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(providers)
}

pub async fn fetch_and_store_models() -> Result<String, ModelsLoadError> {
    // Create .langdb directory in home folder
    let base_dirs = BaseDirs::new().ok_or(ModelsLoadError::NoHomeDir)?;
    let langdb_dir = base_dirs.home_dir().join(".langdb");
    fs::create_dir_all(&langdb_dir)?;

    // Fetch models from API
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.us-east-1.langdb.ai/pricing?include_parameters=true")
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
    Ok(include_str!("../../models.yaml").to_string())
}
