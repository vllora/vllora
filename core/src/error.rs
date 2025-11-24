use crate::http::status::GuardValidationFailed;
use crate::types::guardrails::GuardError;
use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use serde_json::json;
use thiserror::Error;
use vllora_llm::client::error::ModelError;
use vllora_llm::error::LLMError;
use vllora_llm::mcp::McpServerError;
use vllora_llm::types::ModelEvent;

#[derive(Error, Debug)]
pub enum GatewayError {
    #[error("Missing variable {0}")]
    MissingVariable(String),
    #[error(transparent)]
    StdIOError(#[from] std::io::Error),
    #[error(transparent)]
    ParseError(#[from] serde_json::Error),
    #[error("Error decoding argument: {0}")]
    DecodeError(#[from] base64::DecodeError),
    #[error("Custom Error: {0}")]
    CustomError(String),
    #[error("Function get is not implemented")]
    FunctionGetNotImplemented,
    #[error(transparent)]
    ModelError(#[from] Box<ModelError>),
    #[error("Tool call id not found in request")]
    ToolCallIdNotFound,
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error(transparent)]
    BoxedError(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error(transparent)]
    ValidationF32Error(#[from] clust::ValidationError<f32>),
    #[error(transparent)]
    ValidationU32Error(#[from] clust::ValidationError<u32>),
    #[error(transparent)]
    GuardError(#[from] GuardError),
    #[error(transparent)]
    McpServerError(#[from] Box<McpServerError>),
    #[error(transparent)]
    SendError(#[from] Box<tokio::sync::mpsc::error::SendError<Option<ModelEvent>>>),
    #[error("Unsupported provider: {0}")]
    UnsupportedProvider(String),
    #[error(transparent)]
    LLMError(#[from] LLMError),
}

impl From<ModelError> for GatewayError {
    fn from(value: ModelError) -> Self {
        GatewayError::ModelError(Box::new(value))
    }
}

impl From<McpServerError> for GatewayError {
    fn from(value: McpServerError) -> Self {
        GatewayError::McpServerError(Box::new(value))
    }
}

impl From<tokio::sync::mpsc::error::SendError<Option<ModelEvent>>> for GatewayError {
    fn from(value: tokio::sync::mpsc::error::SendError<Option<ModelEvent>>) -> Self {
        GatewayError::SendError(Box::new(value))
    }
}

impl actix_web::error::ResponseError for GatewayError {
    fn error_response(&self) -> HttpResponse {
        tracing::error!("API error: {:?}", self);
        match self {
            GatewayError::GuardError(e) => e.error_response(),
            e => {
                let json_error = json!({
                    "error": e.to_string(),
                });

                HttpResponse::build(e.status_code())
                    .insert_header(ContentType::json())
                    .json(json_error)
            }
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            GatewayError::GuardError(GuardError::GuardNotPassed(_, _)) => {
                GuardValidationFailed::status_code()
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
