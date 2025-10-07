use langdb_core::metadata::error::DatabaseError;
use langdb_core::metadata::models::project::NewProjectDTO;
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::model::{ModelService, ModelServiceImpl};
use langdb_core::metadata::services::project::{ProjectService, ProjectServiceImpl};
use langdb_core::metadata::services::providers::{ProviderService, ProviderServiceImpl};
use tracing::info;
use uuid::Uuid;

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
        println!("Models table is empty. Syncing models from API...");
        match run::models::fetch_and_store_models(db_pool.clone()).await {
            Ok(synced_models) => {
                println!(
                    "✓ Successfully synced {} models to database",
                    synced_models.len()
                );
            }
            Err(e) => {
                eprintln!("⚠ Warning: Failed to sync models: {}", e);
                eprintln!(
                    "  Continuing with empty models table. You can manually sync with: langdb sync"
                );
            }
        }
    }

    Ok(())
}

/// Seeds the database with providers if the providers table is empty
pub async fn seed_providers(db_pool: &DbPool) -> Result<(), run::providers::ProvidersLoadError> {
    let provider_service = ProviderServiceImpl::new(db_pool.clone());
    let providers = provider_service.list_providers()?;

    if providers.is_empty() {
        println!("Providers table is empty. Syncing providers from API...");
        match run::providers::sync_providers(db_pool.clone()).await {
            Ok(()) => {
                println!("✓ Successfully synced providers to database");
            }
            Err(e) => {
                eprintln!("⚠ Warning: Failed to sync providers: {}", e);
                eprintln!("  Continuing with empty providers table.");
            }
        }
    } else {
        println!("Found {} existing providers in database", providers.len());
    }

    Ok(())
}
