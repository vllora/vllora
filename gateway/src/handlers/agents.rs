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

/// POST /agents/register - Register all agents with Distri server
pub async fn register_agents(
    body: web::Json<RegisterAgentsRequest>,
) -> Result<HttpResponse, GatewayApiError> {
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
