use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;
use vllora_core::executor::chat_completion::breakpoint::{
    BreakpointAction, BreakpointError, BreakpointManager,
};
use vllora_core::GatewayApiError;

#[derive(Debug, Clone, Deserialize)]
pub struct ContinueRequest {
    pub breakpoint_id: String,
    pub action: BreakpointAction,
}

pub async fn continue_breakpoint(
    breakpoint_manager: web::Data<BreakpointManager>,
    request: web::Json<ContinueRequest>,
) -> Result<HttpResponse, GatewayApiError> {
    let ContinueRequest {
        breakpoint_id,
        action,
    } = request.into_inner();

    match breakpoint_manager
        .resolve_breakpoint(&breakpoint_id, action)
        .await
    {
        Ok(()) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "status": "ok",
            "breakpoint_id": breakpoint_id
        }))),
        Err(BreakpointError::BreakpointNotFound(id)) => Err(GatewayApiError::CustomError(format!(
            "Breakpoint not found: {}",
            id
        ))),
        Err(BreakpointError::ChannelClosed) => Err(GatewayApiError::CustomError(
            "Breakpoint channel closed".to_string(),
        )),
    }
}
