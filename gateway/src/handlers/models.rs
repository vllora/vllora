use actix_web::{web, HttpResponse, Result};
use langdb_core::metadata::services::model::ModelService;
use langdb_core::models::ModelMetadata;
use langdb_core::GatewayApiError;

/// Handler to list gateway pricing models
pub async fn list_gateway_pricing(
    model_service: web::Data<Box<dyn ModelService>>,
) -> Result<HttpResponse, GatewayApiError> {
    // Query all models (project_id = None means global models)
    let project_id = None;

    let result = model_service.list(project_id).map(|db_models| {
        // Convert DbModel to ModelMetadata
        db_models
            .into_iter()
            .map(|m| m.into())
            .collect::<Vec<ModelMetadata>>()
    });
    // TODO: This needs to be refactored to use endpoints

    match result {
        Ok(models) => Ok(HttpResponse::Ok().json(models)),
        Err(e) => Err(GatewayApiError::CustomError(format!(
            "Failed to fetch models: {}",
            e
        ))),
    }
}
