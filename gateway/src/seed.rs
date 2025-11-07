use tracing::info;
use tracing::warn;
use uuid::Uuid;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::project::NewProjectDTO;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::model::{ModelService, ModelServiceImpl};
use vllora_core::metadata::services::project::{ProjectService, ProjectServiceImpl};
use vllora_core::metadata::services::providers::{ProviderService, ProvidersServiceImpl};

use crate::run;

/// Seeds the database with a default project if no projects exist
pub fn seed_database(db_pool: &DbPool) -> Result<(), DatabaseError> {
    let project_service = ProjectServiceImpl::new(db_pool.clone());

    // Use a dummy owner_id for seeding (you might want to change this)
    let dummy_owner_id = Uuid::nil();

    // Check if any projects exist
    let project_count = project_service.count(dummy_owner_id)?;

    if project_count == 0 {
        info!("No projects found in database. Creating default project...");

        let default_project = NewProjectDTO {
            name: "Default Project".to_string(),
            description: Some("Default project created during database seeding".to_string()),
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let created_project = project_service.create(default_project, dummy_owner_id)?;
        // set this project as default
        project_service.set_default(created_project.id, dummy_owner_id)?;
        info!(
            "Created default project: {} (ID: {})",
            created_project.name, created_project.id
        );
    } else {
        info!("Found {} existing projects in database", project_count);
    }

    Ok(())
}

/// Seeds the database with models if the models table is empty
pub async fn seed_models(db_pool: &DbPool) -> Result<(), run::models::ModelsLoadError> {
    let model_service = ModelServiceImpl::new(db_pool.clone());
    let models = model_service.list(None)?;

    if models.is_empty() {
        info!("Loading embedded models data...");

        // Load from embedded JSON data first for instant availability
        match load_embedded_models(db_pool.clone()).await {
            Ok(embedded_count) => {
                info!(
                    "✓ Successfully loaded {} models from embedded data",
                    embedded_count
                );

                // Spawn background task to fetch fresh models from API
                let db_pool_clone = db_pool.clone();
                tokio::spawn(async move {
                    match run::models::fetch_and_store_models(db_pool_clone).await {
                        Ok(_fresh_models) => {}
                        Err(e) => {
                            info!(
                                "⚠ Background update failed: {}. Continuing with embedded data.",
                                e
                            );
                        }
                    }
                });
            }
            Err(e) => {
                warn!("⚠ Warning: Failed to load embedded models: {}", e);
                warn!("  Falling back to API sync...");

                // Fallback to API sync
                match run::models::fetch_and_store_models(db_pool.clone()).await {
                    Ok(synced_models) => {
                        info!(
                            "✓ Successfully synced {} models to database",
                            synced_models.len()
                        );
                    }
                    Err(e) => {
                        warn!("⚠ Warning: Failed to sync models: {}", e);
                        warn!(
                            "  Continuing with empty models table. You can manually sync with: langdb sync"
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

/// Loads models from embedded JSON data into the database
async fn load_embedded_models(db_pool: DbPool) -> Result<usize, run::models::ModelsLoadError> {
    use crate::MODELS_DATA_JSON;

    // Parse embedded JSON
    let models: Vec<vllora_core::models::ModelMetadata> =
        run::models::load_models_from_json(MODELS_DATA_JSON)?;

    // Convert to DbNewModel and insert into database
    let db_models: Vec<vllora_core::metadata::models::model::DbNewModel> = models
        .iter()
        .map(|m| vllora_core::metadata::models::model::DbNewModel::from(m.clone()))
        .collect();

    let model_service = vllora_core::metadata::services::model::ModelServiceImpl::new(db_pool);
    model_service.insert_many(db_models)?;

    Ok(models.len())
}

/// Seeds the database with providers if the providers table is empty
pub async fn seed_providers(db_pool: &DbPool) -> Result<(), run::providers::ProvidersLoadError> {
    let provider_service = ProvidersServiceImpl::new(db_pool.clone());
    let providers = provider_service.list_providers()?;

    if providers.is_empty() {
        info!("Syncing providers from API...");
        match run::providers::sync_providers(db_pool.clone()).await {
            Ok(()) => {
                info!("✓ Successfully synced providers to database");
            }
            Err(e) => {
                warn!("⚠ Warning: Failed to sync providers: {}", e);
                warn!("  Continuing with empty providers table.");
            }
        }
    } else {
        info!("Found {} existing providers in database", providers.len());
    }

    Ok(())
}
