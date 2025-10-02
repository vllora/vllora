pub mod projects;
pub mod runs;
pub mod traces;
pub mod threads;

use actix_web::{web, HttpResponse};
use langdb_core::handler::models::ChatModelsResponse;
use langdb_core::metadata::services::model::ModelService;
use langdb_core::models::ModelMetadata;
use langdb_core::types::gateway::ChatModel;
use langdb_core::GatewayApiError;
use std::sync::Arc;

/// Handler to list models from SQLite database
pub async fn list_models_from_db(
    model_service: web::Data<Arc<Box<dyn ModelService + Send + Sync>>>,
) -> Result<HttpResponse, GatewayApiError> {
    // For now, we'll query all models (project_id = None means global models)
    // In the future, we can extract project context from the request
    let project_id = None;

    let db_models = model_service
        .list(project_id)
        .map_err(|e| GatewayApiError::CustomError(format!("Failed to fetch models: {}", e)))?;

    // Convert DbModel to ModelMetadata
    let models: Vec<ModelMetadata> = db_models.into_iter().map(|m| m.into()).collect();

    // Return in the same format as list_gateway_models
    let response = ChatModelsResponse {
        object: "list".to_string(),
        data: models
            .iter()
            .map(|v| ChatModel {
                id: v.qualified_model_name(),
                object: "model".to_string(),
                created: 1686935002,
                owned_by: v.model_provider.to_string(),
            })
            .collect(),
    };

    Ok(HttpResponse::Ok().json(response))
}
