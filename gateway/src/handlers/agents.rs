use crate::agents;
use actix_web::HttpResponse;
use vllora_core::GatewayApiError;

/// POST /agents/register - Register all agents with Distri server
pub async fn register_agents() -> Result<HttpResponse, GatewayApiError> {
    match agents::register_agents_with_status().await {
        Ok(result) => Ok(HttpResponse::Ok().json(result)),
        Err(e) => Err(GatewayApiError::CustomError(format!(
            "Failed to register agents: {}",
            e
        ))),
    }
}
