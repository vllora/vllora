#[cfg(feature = "database")]
pub mod database;
pub mod embed_mod;
pub mod error;
pub mod events;
pub mod executor;
pub mod handler;
pub mod http;
pub mod llm_gateway;
pub mod model;
pub mod models;
pub mod otel;
pub mod pricing;
pub mod routing;
pub mod types;

use crate::error::GatewayError;
use crate::http::status::GuardValidationFailed;
use crate::types::gateway::CostCalculatorError;
use crate::types::guardrails::GuardError;
use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use executor::chat_completion::routed_executor::RoutedExecutorError;
use serde_json::json;
use thiserror::Error;

pub use dashmap;

pub mod usage;

pub use bytes;

pub type GatewayResult<T> = Result<T, GatewayError>;

#[derive(Error, Debug)]
pub enum GatewayApiError {
    #[error("Failed to parse JSON")]
    JsonParseError(#[from] serde_json::Error),

    #[error(transparent)]
    GatewayError(#[from] GatewayError),

    #[error("{0}")]
    CustomError(String),

    #[error(transparent)]
    CostCalculatorError(#[from] CostCalculatorError),

    #[error(transparent)]
    ModelError(#[from] model::error::ModelError),

    #[error("Token usage limit exceeded")]
    TokenUsageLimit,

    #[error(transparent)]
    RouteError(#[from] routing::RouterError),

    #[error(transparent)]
    RoutedExecutorError(#[from] RoutedExecutorError),

    #[error(transparent)]
    GuardError(#[from] GuardError),
}

impl GatewayApiError {
    pub fn is_countable_error(&self) -> bool {
        !matches!(
            self,
            GatewayApiError::GuardError(GuardError::GuardNotPassed(_, _))
        )
    }
}
impl actix_web::error::ResponseError for GatewayApiError {
    fn error_response(&self) -> HttpResponse {
        tracing::error!("API error: {:?}", self);
        match self {
            GatewayApiError::GuardError(e) => e.error_response(),
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
            GatewayApiError::JsonParseError(_) => StatusCode::BAD_REQUEST,
            GatewayApiError::GatewayError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayApiError::CustomError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayApiError::CostCalculatorError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayApiError::ModelError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayApiError::RouteError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayApiError::RoutedExecutorError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayApiError::TokenUsageLimit => StatusCode::BAD_REQUEST,
            GatewayApiError::GuardError(e) => match e {
                GuardError::GuardNotPassed(_, _) => GuardValidationFailed::status_code(),
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
        }
    }
}
