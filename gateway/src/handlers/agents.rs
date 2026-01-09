use crate::agents::{self, ModelSettingsConfig};
use actix_web::{web, HttpResponse};
use serde::Deserialize;
use vllora_core::GatewayApiError;

#[derive(Debug, Deserialize)]
pub struct RegisterAgentsRequest {
    /// Optional Distri server URL (defaults to DISTRI_URL env var or http://localhost:8081)
    pub distri_url: Option<String>,

    /// Optional model settings to override agent defaults
    pub model_settings: Option<ModelSettingsConfig>,
}

pub async fn get_lucy_config() -> Result<HttpResponse, GatewayApiError> {
    let config = agents::load_lucy_config().unwrap_or_else(|e| {
        tracing::warn!("Failed to load lucy.json config: {}", e);
        Some(agents::LucyConfig::default())
    });
    Ok(HttpResponse::Ok().json(config))
}

/// POST /agents/register - Register all agents with Distri server
pub async fn register_agents(
    body: web::Json<RegisterAgentsRequest>,
) -> Result<HttpResponse, GatewayApiError> {
    // Save config to lucy.json if provided
    if body.distri_url.is_some() || body.model_settings.is_some() {
        let config = agents::LucyConfig {
            distri_url: body.distri_url.clone(),
            model_settings: body.model_settings.clone(),
        };
        if let Err(e) = agents::save_lucy_config(&config) {
            tracing::warn!("Failed to save lucy.json config: {}", e);
        }
    } else if let Err(e) = agents::delete_lucy_config() {
        tracing::warn!("Failed to delete lucy.json config: {}", e);
    }

    match agents::register_agents_with_status(
        body.distri_url.as_deref(),
        body.model_settings.as_ref(),
    )
    .await
    {
        Ok(result) => Ok(HttpResponse::Ok().json(result)),
        Err(e) => Err(GatewayApiError::CustomError(format!(
            "Failed to register agents: {}",
            e
        ))),
    }
}
