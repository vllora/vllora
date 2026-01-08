use crate::credentials::KeyStorage;
use crate::credentials::ProviderCredentialsId;
use crate::metadata::models::provider::{DbInsertProvider, DbUpdateProvider};
use crate::metadata::pool::DbPool;
use crate::types::metadata::project::Project;
use crate::types::metadata::provider::ProviderInfo;
use crate::types::metadata::services::provider::ProviderService;
use crate::types::GatewayTenant;
use actix_web::HttpMessage;
use actix_web::HttpRequest;
use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;
use vllora_llm::types::credentials::Credentials;
use vllora_llm::types::engine::CustomInferenceApiType;

use crate::ok_json;

#[derive(Deserialize)]
pub struct UpdateProviderRequest {
    pub credentials: Option<Credentials>,
}

#[derive(Serialize)]
pub struct ProviderResponse {
    pub provider: ProviderInfo,
}

/// List all providers with their credential status for the current project
pub async fn list_providers<T: ProviderService>(
    req: HttpRequest,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let providers_service = T::new(db_pool.get_ref().clone());
    let project_id = req.extensions().get::<Project>().cloned().map(|p| p.id);

    ok_json!(providers_service.list_providers_with_credential_status(project_id.as_ref()))
}

/// Update provider credentials for the current project
pub async fn update_provider_key<T: ProviderService>(
    path: web::Path<String>,
    req: web::Json<UpdateProviderRequest>,
    project: web::ReqData<Project>,
    db_pool: web::Data<DbPool>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    tenant: web::ReqData<GatewayTenant>,
) -> Result<HttpResponse> {
    let provider_name = path.into_inner();
    let project = project.into_inner();

    let providers_service = T::new(db_pool.get_ref().clone());

    let provider_credentials_id = ProviderCredentialsId::new(
        tenant.name.clone(),
        provider_name.clone(),
        Some(project.id.to_string())
    );
    let storage = key_storage.into_inner();
    // Check if provider already exists
    let existing_provider = storage.get_key(provider_credentials_id.clone()).await;

    match existing_provider {
        Ok(Some(_)) => {
            match storage
                .update_key(
                    provider_credentials_id.clone(),
                    Some(serde_json::to_string(&req.credentials.clone()).unwrap_or_default()),
                )
                .await
            {
                Ok(_) => {
                    tracing::info!(
                        "Successfully updated provider {} for project {}",
                        provider_name,
                        project.id
                    );

                    // Return updated provider info
                    match providers_service.list_providers_with_credential_status(Some(&project.id))
                    {
                        Ok(providers) => {
                            if let Some(updated_provider) =
                                providers.iter().find(|p| p.name == provider_name)
                            {
                                let response = ProviderResponse {
                                    provider: updated_provider.clone(),
                                };
                                Ok(HttpResponse::Ok().json(response))
                            } else {
                                Ok(HttpResponse::Ok().json(serde_json::json!({
                                    "message": "Provider updated successfully"
                                })))
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Provider updated but failed to fetch updated info: {:?}",
                                e
                            );
                            Ok(HttpResponse::Ok().json(serde_json::json!({
                                "message": "Provider updated successfully"
                            })))
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to update provider {} for project {}: {:?}",
                        provider_name,
                        project.id,
                        e
                    );
                    Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to update provider",
                        "message": e.to_string()
                    })))
                }
            }
        }
        Ok(None) => {
            match storage
                .insert_key(
                    provider_credentials_id.clone(),
                    Some(serde_json::to_string(&req.credentials.clone()).unwrap_or_default()),
                )
                .await
            {
                Ok(_) => {
                    tracing::info!(
                        "Successfully created provider {} for project {}",
                        provider_name,
                        project.id
                    );

                    // Return created provider info
                    match providers_service.list_providers_with_credential_status(Some(&project.id))
                    {
                        Ok(providers) => {
                            if let Some(created_provider) =
                                providers.iter().find(|p| p.name == provider_name)
                            {
                                let response = ProviderResponse {
                                    provider: created_provider.clone(),
                                };
                                Ok(HttpResponse::Created().json(response))
                            } else {
                                Ok(HttpResponse::Created().json(serde_json::json!({
                                    "message": "Provider created successfully"
                                })))
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Provider created but failed to fetch created info: {:?}",
                                e
                            );
                            Ok(HttpResponse::Created().json(serde_json::json!({
                                "message": "Provider created successfully"
                            })))
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to create provider {} for project {}: {:?}",
                        provider_name,
                        project.id,
                        e
                    );
                    Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to create provider",
                        "message": e.to_string()
                    })))
                }
            }
        }
        Err(e) => {
            tracing::error!(
                "Failed to check if provider {} exists for project {}: {:?}",
                provider_name,
                project.id,
                e
            );
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to check provider",
                "message": e.to_string()
            })))
        }
    }
}

/// Delete provider credentials for the current project
pub async fn delete_provider(
    path: web::Path<String>,
    project: web::ReqData<Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let provider_name = path.into_inner();
    let project = project.into_inner();

    match key_storage
        .into_inner()
        .delete_key(ProviderCredentialsId::new(
            "default".to_string(),
            provider_name.clone(),
            None,
        ))
        .await
    {
        Ok(_) => {
            tracing::info!(
                "Successfully deleted provider {} for project {}",
                provider_name,
                project.id
            );
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "message": "Provider deleted successfully"
            })))
        }
        Err(e) => {
            tracing::error!(
                "Failed to delete provider {} for project {}: {:?}",
                provider_name,
                project.id,
                e
            );
            Ok(HttpResponse::NotFound().json(serde_json::json!({
                "error": "Provider not found",
                "message": format!("Provider '{}' not found or already deleted", provider_name)
            })))
        }
    }
}

// Provider definition CRUD handlers

#[derive(Deserialize)]
pub struct CreateProviderRequest {
    pub provider_name: String,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub priority: Option<i32>,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    #[serde(rename = "custom_inference_api_type", alias = "inference_api_type")]
    pub custom_inference_api_type: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateProviderDefinitionRequest {
    pub provider_name: Option<String>,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub priority: Option<i32>,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    #[serde(rename = "custom_inference_api_type", alias = "inference_api_type")]
    pub custom_inference_api_type: Option<String>,
}

/// Create a new provider definition
pub async fn create_provider_definition<T: ProviderService>(
    req: web::Json<CreateProviderRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let providers_service = T::new(db_pool.get_ref().clone());

    // Check if provider already exists
    if providers_service.provider_exists(&req.provider_name)? {
        return Ok(HttpResponse::Conflict().json(serde_json::json!({
            "error": "Provider already exists",
            "message": format!("Provider '{}' already exists", req.provider_name)
        })));
    }

    // Parse custom_inference_api_type
    let custom_inference_api_type = req
        .custom_inference_api_type
        .as_deref()
        .and_then(|s| CustomInferenceApiType::from_str(s).ok())
        .map(|t| t.to_string());

    let provider = DbInsertProvider::new_with_custom(
        Uuid::new_v4().to_string(),
        req.provider_name.clone(),
        req.description.clone(),
        req.endpoint.clone(),
        req.priority.unwrap_or(0),
        req.privacy_policy_url.clone(),
        req.terms_of_service_url.clone(),
        custom_inference_api_type,
        true, // All providers created via /providers endpoint are custom
    );

    if let Err(e) = providers_service.create_provider(provider) {
        return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to create provider",
            "message": e.to_string()
        })));
    }

    // Fetch and return the created provider
    match providers_service.get_provider_by_name(&req.provider_name) {
        Ok(Some(provider_info)) => Ok(HttpResponse::Created().json(ProviderResponse {
            provider: provider_info,
        })),
        Ok(None) => Ok(HttpResponse::Created().json(serde_json::json!({
            "message": "Provider created successfully"
        }))),
        Err(e) => {
            tracing::warn!("Provider created but failed to fetch: {:?}", e);
            Ok(HttpResponse::Created().json(serde_json::json!({
                "message": "Provider created successfully"
            })))
        }
    }
}

/// Get a provider definition by ID
pub async fn get_provider_definition<T: ProviderService>(
    path: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let provider_id = path.into_inner();
    let providers_service = T::new(db_pool.get_ref().clone());

    match providers_service.get_provider_by_id(&provider_id) {
        Ok(Some(provider)) => Ok(HttpResponse::Ok().json(ProviderResponse { provider })),
        Ok(None) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Provider not found",
            "message": format!("Provider with ID '{}' not found", provider_id)
        }))),
        Err(e) => {
            tracing::error!("Failed to get provider: {:?}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to get provider",
                "message": e.to_string()
            })))
        }
    }
}

/// Update a provider definition
pub async fn update_provider_definition<T: ProviderService>(
    path: web::Path<String>,
    req: web::Json<UpdateProviderDefinitionRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let provider_id = path.into_inner();
    let providers_service = T::new(db_pool.get_ref().clone());

    // Parse custom_inference_api_type if provided
    let custom_inference_api_type = req
        .custom_inference_api_type
        .as_deref()
        .and_then(|s| CustomInferenceApiType::from_str(s).ok())
        .map(|t| t.to_string());

    let mut update = DbUpdateProvider::new();
    if let Some(provider_name) = &req.provider_name {
        update.provider_name = Some(provider_name.clone());
    }
    if let Some(description) = &req.description {
        update.description = Some(description.clone());
    }
    if let Some(endpoint) = &req.endpoint {
        update.endpoint = Some(endpoint.clone());
    }
    if let Some(priority) = req.priority {
        update.priority = Some(priority);
    }
    if let Some(privacy_policy_url) = &req.privacy_policy_url {
        update.privacy_policy_url = Some(privacy_policy_url.clone());
    }
    if let Some(terms_of_service_url) = &req.terms_of_service_url {
        update.terms_of_service_url = Some(terms_of_service_url.clone());
    }
    if custom_inference_api_type.is_some() {
        update.custom_inference_api_type = custom_inference_api_type;
    }

    providers_service.update_provider(&provider_id, update)?;

    // Fetch and return the updated provider
    match providers_service.get_provider_by_id(&provider_id) {
        Ok(Some(provider)) => Ok(HttpResponse::Ok().json(ProviderResponse { provider })),
        Ok(None) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Provider not found",
            "message": format!("Provider with ID '{}' not found", provider_id)
        }))),
        Err(e) => {
            tracing::warn!("Provider updated but failed to fetch: {:?}", e);
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "message": "Provider updated successfully"
            })))
        }
    }
}

/// Delete a provider definition (soft delete)
/// Only providers with is_custom = true can be deleted
pub async fn delete_provider_definition<T: ProviderService>(
    path: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let provider_id = path.into_inner();
    let providers_service = T::new(db_pool.get_ref().clone());

    // Check if provider exists
    match providers_service.get_provider_by_id(&provider_id) {
        Ok(Some(_)) => {
            // Check if provider is custom
            match providers_service.is_provider_custom(&provider_id) {
                Ok(Some(true)) => {
                    // Provider is custom, allow deletion
                    match providers_service.delete_provider(&provider_id) {
                        Ok(_) => Ok(HttpResponse::Ok().json(serde_json::json!({
                            "message": "Provider deleted successfully"
                        }))),
                        Err(e) => {
                            tracing::error!("Failed to delete provider: {:?}", e);
                            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                                "error": "Failed to delete provider",
                                "message": e.to_string()
                            })))
                        }
                    }
                }
                Ok(Some(false)) | Ok(None) => {
                    // Provider is not custom or doesn't exist, deny deletion
                    Ok(HttpResponse::Forbidden().json(serde_json::json!({
                        "error": "Cannot delete provider",
                        "message": "Only custom providers can be deleted"
                    })))
                }
                Err(e) => {
                    tracing::error!("Failed to check if provider is custom: {:?}", e);
                    Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to check provider",
                        "message": e.to_string()
                    })))
                }
            }
        }
        Ok(None) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Provider not found",
            "message": format!("Provider with ID '{}' not found", provider_id)
        }))),
        Err(e) => {
            tracing::error!("Failed to get provider: {:?}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to get provider",
                "message": e.to_string()
            })))
        }
    }
}
