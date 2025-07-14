use std::collections::HashMap;

use crate::{models::ModelCapability, types::gateway::ChatModel};
use actix_web::{web, HttpResponse};
use serde::Serialize;

use crate::GatewayApiError;

use super::AvailableModels;

#[derive(Serialize)]
pub struct ChatModelsResponse {
    pub object: String,
    pub data: Vec<ChatModel>,
}

pub async fn list_gateway_models(
    models: web::Data<AvailableModels>,
) -> Result<HttpResponse, GatewayApiError> {
    let response = ChatModelsResponse {
        object: "list".to_string(),
        data: models
            .into_inner()
            .0
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

pub async fn list_gateway_models_capabilities(
    models: web::Data<AvailableModels>,
) -> Result<HttpResponse, GatewayApiError> {
    let capabilities: HashMap<String, Vec<ModelCapability>> = models
        .into_inner()
        .0
        .iter()
        .map(|model| (model.model.to_string(), model.capabilities.clone()))
        .collect();

    Ok(HttpResponse::Ok().json(capabilities))
}

pub async fn list_gateway_pricing(
    models: web::Data<AvailableModels>,
) -> Result<HttpResponse, GatewayApiError> {
    Ok(HttpResponse::Ok().json(models.into_inner().0.clone()))
}
