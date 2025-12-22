use std::collections::HashMap;

use actix_web::{web, HttpResponse, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use vllora_llm::types::gateway::ChatModel;
use vllora_llm::types::models::{ModelCapability, ModelIOFormats, ModelMetadata, ModelType};
use vllora_llm::types::provider::{CompletionModelPrice, ModelPrice};

use crate::metadata::models::model::DbNewModel;
use crate::types::metadata::services::model::ModelService;
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

#[derive(Deserialize)]
pub struct CreateModelRequest {
    pub model_name: String,
    pub description: Option<String>,
    pub provider_name: String,
    pub model_type: ModelType,
    pub input_token_price: Option<f64>,
    pub output_token_price: Option<f64>,
    pub context_size: Option<u32>,
    pub capabilities: Option<Vec<ModelCapability>>,
    pub input_types: Option<Vec<ModelIOFormats>>,
    pub output_types: Option<Vec<ModelIOFormats>>,
    pub parameters: Option<serde_json::Value>,
    pub benchmark_info: Option<serde_json::Value>,
    pub release_date: Option<NaiveDate>,
    pub langdb_release_date: Option<NaiveDate>,
    pub knowledge_cutoff_date: Option<NaiveDate>,
    pub license: Option<String>,
    pub endpoint: Option<String>,
    pub tags: Option<Vec<String>>,
    pub priority: Option<i32>,
    pub cached_input_token_price: Option<f64>,
    pub cached_input_write_token_price: Option<f64>,
    pub model_name_in_provider: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateModelRequest {
    pub model_name: Option<String>,
    pub description: Option<String>,
    pub provider_name: Option<String>,
    pub model_type: Option<ModelType>,
    pub input_token_price: Option<f64>,
    pub output_token_price: Option<f64>,
    pub context_size: Option<u32>,
    pub capabilities: Option<Vec<ModelCapability>>,
    pub input_types: Option<Vec<ModelIOFormats>>,
    pub output_types: Option<Vec<ModelIOFormats>>,
    pub parameters: Option<serde_json::Value>,
    pub benchmark_info: Option<serde_json::Value>,
    pub release_date: Option<NaiveDate>,
    pub langdb_release_date: Option<NaiveDate>,
    pub knowledge_cutoff_date: Option<NaiveDate>,
    pub license: Option<String>,
    pub endpoint: Option<String>,
    pub tags: Option<Vec<String>>,
    pub priority: Option<i32>,
    pub cached_input_token_price: Option<f64>,
    pub cached_input_write_token_price: Option<f64>,
    pub model_name_in_provider: Option<String>,
    pub is_custom: Option<bool>,
}

#[derive(Serialize)]
pub struct ModelResponse {
    pub model: ModelMetadata,
}

/// Get a model by ID
pub async fn get_model<T: ModelService>(
    path: web::Path<String>,
    model_service: web::Data<Box<dyn ModelService>>,
) -> Result<HttpResponse, GatewayApiError> {
    let model_id = path.into_inner();

    let db_model = model_service
        .get_by_id(model_id.clone())
        .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

    let model: ModelMetadata = db_model.into();

    Ok(HttpResponse::Ok().json(ModelResponse { model }))
}

/// Create a new model
pub async fn create_model<T: ModelService>(
    req: web::Json<CreateModelRequest>,
    model_service: web::Data<Box<dyn ModelService>>,
) -> Result<HttpResponse, GatewayApiError> {
    // Build ModelMetadata
    let model_metadata = ModelMetadata {
        model: req.model_name.clone(),
        model_provider: req.provider_name.clone(), // Using provider_name as owner_name for now
        inference_provider: vllora_llm::types::models::InferenceProvider {
            provider: vllora_llm::types::provider::InferenceModelProvider::from(
                req.provider_name.clone(),
            ),
            model_name: req
                .model_name_in_provider
                .clone()
                .unwrap_or_else(|| req.model_name.clone()),
            endpoint: req.endpoint.clone(),
            custom_inference_api_type: None, // Will be populated from provider join when needed
        },
        price: ModelPrice::Completion(CompletionModelPrice {
            per_input_token: req.input_token_price.unwrap_or(0.0),
            per_output_token: req.output_token_price.unwrap_or(0.0),
            per_cached_input_token: req.cached_input_token_price,
            per_cached_input_write_token: req.cached_input_write_token_price,
            valid_from: None,
        }),
        input_formats: req.input_types.clone().unwrap_or_default(),
        output_formats: req.output_types.clone().unwrap_or_default(),
        capabilities: req.capabilities.clone().unwrap_or_default(),
        r#type: req.model_type.clone(),
        limits: vllora_llm::types::models::Limits::new(req.context_size.unwrap_or(0)),
        description: req.description.clone().unwrap_or_default(),
        parameters: req.parameters.clone(),
        benchmark_info: req.benchmark_info.clone(),
        virtual_model_id: None, // Will be generated by database
        min_service_level: req.priority.unwrap_or(0),
        release_date: req.release_date,
        license: req.license.clone(),
        knowledge_cutoff_date: req.knowledge_cutoff_date,
        langdb_release_date: req.langdb_release_date,
        is_private: false,
        is_custom: true,
    };

    // Convert to DbNewModel
    let mut db_model: DbNewModel = model_metadata.into();
    // All models created via /models endpoint are custom
    db_model.is_custom = 1;

    model_service
        .upsert(db_model)
        .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

    // Fetch and return the created model
    // Note: We'd need to fetch by provider_name + model_name since we don't have the ID yet
    // For now, return success
    Ok(HttpResponse::Created().json(serde_json::json!({
        "message": "Model created successfully"
    })))
}

/// Update a model
pub async fn update_model<T: ModelService>(
    path: web::Path<String>,
    req: web::Json<UpdateModelRequest>,
    model_service: web::Data<Box<dyn ModelService>>,
) -> Result<HttpResponse, GatewayApiError> {
    let model_id = path.into_inner();

    // Fetch existing model
    let existing_model = model_service
        .get_by_id(model_id.clone())
        .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

    // Build updated ModelMetadata
    let mut model_metadata: ModelMetadata = existing_model.clone().into();

    // Apply updates
    if let Some(model_name) = &req.model_name {
        model_metadata.model = model_name.clone();
    }
    if let Some(description) = &req.description {
        model_metadata.description = description.clone();
    }
    if let Some(provider_name) = &req.provider_name {
        model_metadata.model_provider = provider_name.clone();
        model_metadata.inference_provider.provider =
            vllora_llm::types::provider::InferenceModelProvider::from(provider_name.clone());
    }
    if let Some(model_type) = &req.model_type {
        model_metadata.r#type = model_type.clone();
    }
    if let Some(input_price) = req.input_token_price {
        if let ModelPrice::Completion(ref mut price) = model_metadata.price {
            price.per_input_token = input_price;
        }
    }
    if let Some(output_price) = req.output_token_price {
        if let ModelPrice::Completion(ref mut price) = model_metadata.price {
            price.per_output_token = output_price;
        }
    }
    if let Some(context_size) = req.context_size {
        model_metadata.limits.max_context_size = context_size;
    }
    if let Some(capabilities) = &req.capabilities {
        model_metadata.capabilities = capabilities.clone();
    }
    if let Some(input_types) = &req.input_types {
        model_metadata.input_formats = input_types.clone();
    }
    if let Some(output_types) = &req.output_types {
        model_metadata.output_formats = output_types.clone();
    }
    if req.parameters.is_some() {
        model_metadata.parameters = req.parameters.clone();
    }
    if req.benchmark_info.is_some() {
        model_metadata.benchmark_info = req.benchmark_info.clone();
    }
    if let Some(release_date) = &req.release_date {
        model_metadata.release_date = Some(*release_date);
    }
    if let Some(langdb_release_date) = &req.langdb_release_date {
        model_metadata.langdb_release_date = Some(*langdb_release_date);
    }
    if let Some(knowledge_cutoff_date) = &req.knowledge_cutoff_date {
        model_metadata.knowledge_cutoff_date = Some(*knowledge_cutoff_date);
    }
    if req.license.is_some() {
        model_metadata.license = req.license.clone();
    }
    if let Some(endpoint) = &req.endpoint {
        model_metadata.inference_provider.endpoint = Some(endpoint.clone());
    }
    if let Some(priority) = req.priority {
        model_metadata.min_service_level = priority;
    }
    if let Some(cached_input_price) = req.cached_input_token_price {
        if let ModelPrice::Completion(ref mut price) = model_metadata.price {
            price.per_cached_input_token = Some(cached_input_price);
        }
    }
    if let Some(cached_write_price) = req.cached_input_write_token_price {
        if let ModelPrice::Completion(ref mut price) = model_metadata.price {
            price.per_cached_input_write_token = Some(cached_write_price);
        }
    }
    if let Some(model_name_in_provider) = &req.model_name_in_provider {
        model_metadata.inference_provider.model_name = model_name_in_provider.clone();
    }

    // Preserve the ID from existing model
    model_metadata.virtual_model_id = existing_model.id.clone();

    // Convert to DbNewModel and upsert
    let db_model: DbNewModel = model_metadata.into();
    model_service
        .upsert(db_model)
        .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

    // Fetch and return updated model
    let updated_model = model_service
        .get_by_id(model_id)
        .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

    let model: ModelMetadata = updated_model.into();
    Ok(HttpResponse::Ok().json(ModelResponse { model }))
}

/// Delete a model (soft delete)
/// Only models with is_custom = true can be deleted
pub async fn delete_model<T: ModelService>(
    path: web::Path<String>,
    model_service: web::Data<Box<dyn ModelService>>,
) -> Result<HttpResponse, GatewayApiError> {
    let model_id = path.into_inner();

    // Get the model to check if it's custom
    let db_model = model_service
        .get_by_id(model_id.clone())
        .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

    // Check if model is custom
    if db_model.is_custom != 1 {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Cannot delete model",
            "message": "Only custom models can be deleted"
        })));
    }

    model_service
        .mark_as_deleted(model_id.clone())
        .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "Model deleted successfully"
    })))
}
