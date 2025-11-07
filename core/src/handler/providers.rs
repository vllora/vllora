use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use crate::credentials::KeyStorage;
use crate::credentials::ProviderCredentialsId;
use crate::metadata::pool::DbPool;
use crate::metadata::services::providers::{
    ProviderInfo as ProvidersProviderInfo, ProviderService as ProvidersService
};
use crate::types::credentials::Credentials;
use crate::types::metadata::project::Project;
use actix_web::HttpRequest;
use actix_web::HttpMessage;

use crate::ok_json;

#[derive(Deserialize)]
pub struct UpdateProviderRequest {
    pub credentials: Option<Credentials>,
}

#[derive(Serialize)]
pub struct ProviderResponse {
    pub provider: ProvidersProviderInfo,
}

/// List all providers with their credential status for the current project
pub async fn list_providers<T: ProvidersService>(
    req: HttpRequest,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let providers_service = T::new(db_pool.get_ref().clone());
    let project_id = req.extensions().get::<Project>().cloned().map(|p| p.id);

    ok_json!(providers_service.list_providers_with_credential_status(project_id.as_ref()))
}

/// Update provider credentials for the current project
pub async fn update_provider<T: ProvidersService>(
    path: web::Path<String>,
    req: web::Json<UpdateProviderRequest>,
    project: web::ReqData<Project>,
    db_pool: web::Data<DbPool>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let provider_name = path.into_inner();
    let project = project.into_inner();

    let providers_service = T::new(db_pool.get_ref().clone());

    let provider_credentials_id = ProviderCredentialsId::new(
        "default".to_string(),
        provider_name.clone(),
        project.id.to_string(),
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
                    match providers_service
                        .list_providers_with_credential_status(Some(&project.id))
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
                    match providers_service
                        .list_providers_with_credential_status(Some(&project.id))
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
            project.id.to_string(),
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
