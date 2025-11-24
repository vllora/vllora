pub mod experiments;
pub mod models;
pub mod projects;
pub mod session;
pub mod threads;

use actix_web::{web, HttpResponse};
use chrono::NaiveTime;
use vllora_core::handler::models::ChatModelsResponse;
use vllora_core::types::metadata::services::model::ModelService;
use vllora_core::GatewayApiError;
use vllora_llm::types::gateway::ChatModel;
use vllora_llm::types::models::ModelMetadata;

/// Handler to list models from SQLite database
pub async fn list_models_from_db(
    model_service: web::Data<Box<dyn ModelService>>,
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
                created: v
                    .release_date
                    .unwrap_or(chrono::Utc::now().date_naive())
                    .and_time(NaiveTime::from_hms_opt(0, 0, 0).expect("Invalid time"))
                    .and_utc()
                    .timestamp(),
                owned_by: v.model_provider.to_string(),
            })
            .collect(),
    };

    Ok(HttpResponse::Ok().json(response))
}
