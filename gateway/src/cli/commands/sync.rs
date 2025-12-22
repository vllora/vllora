use crate::run;
use crate::CliError;
use ::tracing::info;
use vllora_core::metadata::pool::DbPool;

pub async fn handle_sync(db_pool: DbPool, models: bool, providers: bool) -> Result<(), CliError> {
    // If no specific flags are provided, sync both
    let sync_models = models || !providers;
    let sync_providers = providers || !models;

    if sync_models {
        info!("Syncing models from API to database...");
        let models = run::models::fetch_and_store_models(db_pool.clone()).await?;
        info!("Successfully synced {} models to database", models.len());
    }

    if sync_providers {
        info!("Syncing providers from API to database...");
        run::providers::sync_providers(db_pool.clone()).await?;
        info!("Successfully synced providers to database");
    }

    Ok(())
}
