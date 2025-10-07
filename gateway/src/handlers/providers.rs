use actix_web::{web, HttpResponse, Result};
use langdb_core::metadata::models::provider::{
    NewProviderCredentialsDTO, UpdateProviderCredentialsDTO,
};
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::provider::{ProviderService, ProviderServiceImpl};
use langdb_core::metadata::services::providers::{
    ProviderInfo as ProvidersProviderInfo, ProviderService as ProvidersService,
    ProviderServiceImpl as ProvidersServiceImpl,
};
use langdb_core::types::credentials::Credentials;
use langdb_core::types::metadata::project::Project;
use serde::{Deserialize, Serialize};

use crate::ok_json;

#[derive(Deserialize)]
pub struct UpdateProviderRequest {
    pub provider_type: Option<String>,
    pub credentials: Option<Credentials>,
}

#[derive(Deserialize)]
pub struct CreateProviderRequest {
    pub provider_type: String,
    pub credentials: Credentials,
}

#[derive(Serialize)]
pub struct ProviderResponse {
    pub provider: ProvidersProviderInfo,
}

/// List all providers with their credential status for the current project
pub async fn list_providers(
    project: web::ReqData<Project>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let project = project.into_inner();

    let providers_service = ProvidersServiceImpl::new(db_pool.get_ref().clone());

    ok_json!(providers_service.list_providers_with_credential_status(Some(&project.id.to_string())))
}

/// Update provider credentials for the current project
pub async fn update_provider(
    path: web::Path<String>,
    req: web::Json<UpdateProviderRequest>,
    project: web::ReqData<Project>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let provider_name = path.into_inner();
    let project = project.into_inner();

    let provider_service = ProviderServiceImpl::new(db_pool.get_ref().clone());
    let providers_service = ProvidersServiceImpl::new(db_pool.get_ref().clone());

    // Check if provider already exists
    let existing_provider =
        provider_service.get_provider_credentials(&provider_name, Some(&project.id.to_string()));

    match existing_provider {
        Ok(Some(_)) => {
            // Update existing provider
            let update_data = UpdateProviderCredentialsDTO {
                credentials: req.credentials.clone(),
                is_active: None,
            };

            match provider_service.update_provider(
                &provider_name,
                Some(&project.id.to_string()),
                update_data.to_db_update().map_err(|e| {
                    tracing::error!("Failed to convert update data: {}", e);
                    actix_web::error::ErrorInternalServerError("Invalid credentials format")
                })?,
            ) {
                Ok(_) => {
                    tracing::info!(
                        "Successfully updated provider {} for project {}",
                        provider_name,
                        project.id
                    );

                    // Return updated provider info
                    match providers_service
                        .list_providers_with_credential_status(Some(&project.id.to_string()))
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
            // Create new provider if it doesn't exist
            let new_provider = NewProviderCredentialsDTO {
                provider_name: provider_name.clone(),
                provider_type: req
                    .provider_type
                    .clone()
                    .unwrap_or_else(|| "api_key".to_string()),
                credentials: req.credentials.clone().unwrap_or_default(),
                project_id: Some(project.id.to_string()),
            };

            match provider_service.save_provider(new_provider.to_db_insert().map_err(|e| {
                tracing::error!("Failed to convert new provider data: {}", e);
                actix_web::error::ErrorInternalServerError("Invalid credentials format")
            })?) {
                Ok(_) => {
                    tracing::info!(
                        "Successfully created provider {} for project {}",
                        provider_name,
                        project.id
                    );

                    // Return created provider info
                    match providers_service
                        .list_providers_with_credential_status(Some(&project.id.to_string()))
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
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let provider_name = path.into_inner();
    let project = project.into_inner();

    let provider_service = ProviderServiceImpl::new(db_pool.get_ref().clone());

    match provider_service.delete_provider(&provider_name, Some(&project.id.to_string())) {
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

/// Create a new provider for the current project
pub async fn create_provider(
    req: web::Json<CreateProviderRequest>,
    project: web::ReqData<Project>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let project = project.into_inner();

    let provider_service = ProviderServiceImpl::new(db_pool.get_ref().clone());
    let providers_service = ProvidersServiceImpl::new(db_pool.get_ref().clone());

    // Extract provider name from credentials or use a default based on provider type
    let provider_name = match &req.credentials {
        Credentials::ApiKey(_) => "openai",
        Credentials::ApiKeyWithEndpoint { .. } => "custom",
        Credentials::Aws(_) => "aws_bedrock",
        Credentials::Vertex(_) => "vertex",
        Credentials::LangDb => "langdb",
    }
    .to_string();

    // Check if provider already exists
    match provider_service.get_provider_credentials(&provider_name, Some(&project.id.to_string())) {
        Ok(Some(_)) => {
            return Ok(HttpResponse::Conflict().json(serde_json::json!({
                "error": "Provider already exists",
                "message": format!("Provider '{}' already exists for this project", provider_name)
            })));
        }
        Ok(None) => {
            // Provider doesn't exist, create it
        }
        Err(e) => {
            tracing::error!(
                "Failed to check if provider {} exists for project {}: {:?}",
                provider_name,
                project.id,
                e
            );
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to check provider",
                "message": e.to_string()
            })));
        }
    }

    let new_provider = NewProviderCredentialsDTO {
        provider_name: provider_name.clone(),
        provider_type: req.provider_type.clone(),
        credentials: req.credentials.clone(),
        project_id: Some(project.id.to_string()),
    };

    match provider_service.save_provider(new_provider.to_db_insert().map_err(|e| {
        tracing::error!("Failed to convert new provider data: {}", e);
        actix_web::error::ErrorInternalServerError("Invalid credentials format")
    })?) {
        Ok(_) => {
            tracing::info!(
                "Successfully created provider {} for project {}",
                provider_name,
                project.id
            );

            // Return created provider info
            match providers_service
                .list_providers_with_credential_status(Some(&project.id.to_string()))
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
                    tracing::warn!("Provider created but failed to fetch created info: {:?}", e);
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
