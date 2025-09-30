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
}
