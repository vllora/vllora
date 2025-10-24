use actix_web::{web, HttpResponse, Result};
use langdb_core::metadata::services::model::ModelService;
use langdb_core::models::ModelMetadata;
use langdb_core::GatewayApiError;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct PricingQueryParams {
    #[serde(default)]
    pub include_parameters: bool,
    #[serde(default)]
    pub include_benchmark: bool,
}

/// Handler to list gateway pricing models
pub async fn list_gateway_pricing(
    query: web::Query<PricingQueryParams>,
    model_service: web::Data<Box<dyn ModelService>>,
) -> Result<HttpResponse, GatewayApiError> {
    // Query all models (project_id = None means global models)
    let project_id = None;

    let result = model_service.list(project_id).map(|db_models| {
        // Convert DbModel to ModelMetadata and filter fields based on query parameters
        db_models
            .into_iter()
            .map(|m| {
                let mut model_metadata: ModelMetadata = m.into();

                // Filter out parameters if not requested
                if !query.include_parameters {
                    model_metadata.parameters = None;
                }

                // Filter out benchmark_info if not requested
                if !query.include_benchmark {
                    model_metadata.benchmark_info = None;
                }

                model_metadata
            })
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
