use crate::run;
use crate::CliError;
use ::tracing::info;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::model::ModelServiceImpl;
use vllora_core::types::metadata::services::model::ModelService;

pub async fn handle_list(db_pool: DbPool) -> Result<(), CliError> {
    // Query models from database
    let model_service = ModelServiceImpl::new(db_pool.clone());
    let db_models = model_service.list(None)?;

    info!("Found {} models in database\n", db_models.len());

    // Convert DbModel to ModelMetadata and display as table
    let models: Vec<vllora_llm::types::models::ModelMetadata> =
        db_models.into_iter().map(|m| m.into()).collect();

    run::table::pretty_print_models(models);
    Ok(())
}
