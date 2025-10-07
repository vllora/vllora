use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Failed to connect to database: {0:?}")]
    ConnectionError(#[from] r2d2::Error),

    #[error("Query error: {0:?}")]
    QueryError(#[from] diesel::result::Error),

    #[error("Failed to convert JSON: {0:?}")]
    JsonError(#[from] serde_json::Error),

    #[error("Unique violation: {0}")]
    UniqueViolation(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}

impl ResponseError for DatabaseError {
    fn error_response(&self) -> HttpResponse {
        match self {
            DatabaseError::QueryError(diesel::result::Error::NotFound) => {
                HttpResponse::NotFound().finish()
            }
            _ => HttpResponse::InternalServerError().json(serde_json::json!({
                "error": self.to_string(),
            })),
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            DatabaseError::QueryError(diesel::result::Error::NotFound) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
