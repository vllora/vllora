pub mod credentials;
pub mod database;
pub mod embed_mod;
pub mod error;
pub mod events;
pub mod executor;
pub mod handler;
pub mod http;
pub mod llm_gateway;
pub mod mcp;
pub mod metadata;
pub mod model;
pub mod models;
pub mod pricing;
pub mod responses;
pub mod routing;
pub mod telemetry;
pub mod types;

use crate::error::GatewayError;
use crate::types::gateway::CostCalculatorError;
use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use executor::chat_completion::routed_executor::RoutedExecutorError;
use serde_json::json;
use thiserror::Error;
use tracing::Span;

pub use dashmap;

pub mod usage;

pub use bytes;
use types::guardrails::GuardError;

pub type GatewayResult<T> = Result<T, GatewayError>;

pub use either;
pub use rmcp;

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

    #[error("Token usage limit exceeded")]
    TokenUsageLimit,

    #[error(transparent)]
    RouteError(#[from] routing::RouterError),

    #[error(transparent)]
    RoutedExecutorError(#[from] RoutedExecutorError),
}

impl GatewayApiError {
    pub fn is_countable_error(&self) -> bool {
        !matches!(
            self,
            GatewayApiError::GatewayError(GatewayError::GuardError(GuardError::GuardNotPassed(
                _,
                _
            )))
        )
    }
}

impl actix_web::error::ResponseError for GatewayApiError {
    fn error_response(&self) -> HttpResponse {
        tracing::error!("API error: {:?}", self);

        let span = Span::current();
        span.record("error", self.to_string());

        match self {
            GatewayApiError::GatewayError(e) => e.error_response(),
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
            GatewayApiError::GatewayError(e) => e.status_code(),
            GatewayApiError::CustomError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayApiError::CostCalculatorError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayApiError::RouteError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayApiError::RoutedExecutorError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayApiError::TokenUsageLimit => StatusCode::BAD_REQUEST,
        }
    }
}
