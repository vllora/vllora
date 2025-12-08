use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use vllora_core::events::callback_handler::GatewayCallbackHandlerFn;
use vllora_core::events::callback_handler::GatewayEvent;
use vllora_core::events::callback_handler::GlobalBreakpointStateEvent;
use vllora_core::executor::chat_completion::breakpoint::{
    BreakpointAction, BreakpointError, BreakpointManager,
};
use vllora_core::types::metadata::project::Project;
use vllora_core::GatewayApiError;
use vllora_llm::types::gateway::ChatCompletionRequest;

#[derive(Debug, Clone, Deserialize)]
pub struct ContinueRequest {
    pub breakpoint_id: String,
    pub action: BreakpointAction,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContinueRequestWithThreadId {
    pub breakpoint_id: String,
    pub request: ChatCompletionRequest,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContinueRequestWithThreadIdResponse {
    pub breakpoints: Vec<ContinueRequestWithThreadId>,
    pub intercept_all: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GlobalBreakpointRequest {
    /// When true, intercept all requests regardless of tags.
    /// When false, fall back to tag-based interception.
    pub intercept_all: bool,
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

/// Continue all pending breakpoints with the original request
pub async fn continue_all_breakpoints(
    breakpoint_manager: web::Data<BreakpointManager>,
) -> Result<HttpResponse, GatewayApiError> {
    breakpoint_manager.continue_all().await;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "continued": "all"
    })))
}

/// List all pending breakpoints and their stored requests
pub async fn list_breakpoints(
    breakpoint_manager: web::Data<BreakpointManager>,
) -> Result<HttpResponse, GatewayApiError> {
    let breakpoints = breakpoint_manager.list_breakpoints().await;

    let breakpoints = breakpoints
        .into_iter()
        .map(|(breakpoint_id, (request, thread_id))| {
            ContinueRequestWithThreadId {
                breakpoint_id,
                request,
                thread_id
            }
        })
        .collect();
    let intercept_all = breakpoint_manager.intercept_all();

    Ok(HttpResponse::Ok().json(ContinueRequestWithThreadIdResponse {
        breakpoints,
        intercept_all,
    }))
}

pub async fn set_global_breakpoint(
    breakpoint_manager: web::Data<BreakpointManager>,
    request: web::Json<GlobalBreakpointRequest>,
    callback_handler: web::Data<GatewayCallbackHandlerFn>,
    project: web::ReqData<Project>,
) -> Result<HttpResponse, GatewayApiError> {
    let GlobalBreakpointRequest { intercept_all } = request.into_inner();

    breakpoint_manager.set_intercept_all(intercept_all).await;

    callback_handler
        .on_message(GatewayEvent::GlobalBreakpointEvent(
            GlobalBreakpointStateEvent {
                intercept_all,
                tenant_name: "vllora".to_string(),
                project_id: project.slug.clone(),
            },
        ))
        .await;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "intercept_all": intercept_all
    })))
}
