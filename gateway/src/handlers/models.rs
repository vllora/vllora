use actix_web::{web, HttpResponse, Result};
use langdb_core::handler::models::ChatModelsResponse;
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::model::{ModelService, ModelServiceImpl};
use langdb_core::metadata::services::project_model_restriction::ProjectModelRestrictionService;
use langdb_core::models::ModelMetadata;
use langdb_core::types::gateway::ChatModel;
use langdb_core::GatewayApiError;
use uuid::Uuid;

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

/// Handler to get filtered models for a specific project
pub async fn get_project_models(
    project_id: web::Path<Uuid>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let project_id = project_id.into_inner();
    
    // Fetch all models for the project
    let model_service = ModelServiceImpl::new(db_pool.get_ref().clone());
    let db_models = model_service
        .list(Some(project_id))
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to fetch models: {}", e))
        })?;

    // Convert DbModel to ModelMetadata
    let mut models: Vec<ModelMetadata> = db_models
        .into_iter()
        .map(|m| m.into())
        .collect();

    // Fetch and apply restrictions
    let restriction_service = ProjectModelRestrictionService::new(db_pool.get_ref().clone());
    let restrictions = restriction_service
        .get_by_project_id(&project_id.to_string())
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to fetch restrictions: {}",
                e
            ))
        })?;

    // Apply filtering
    models = ProjectModelRestrictionService::apply_restrictions(models, restrictions);

    // Return in the same format as list_gateway_models (ChatModelsResponse)
    let response = ChatModelsResponse {
        object: "list".to_string(),
        data: models
            .iter()
            .map(|v| ChatModel {
                id: v.qualified_model_name(),
                object: "model".to_string(),
                created: 1686935002,
                owned_by: v.model_provider.clone(),
            })
            .collect(),
    };

    Ok(HttpResponse::Ok().json(response))
}
