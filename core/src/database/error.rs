use thiserror::Error;

use std::error::Error;

#[derive(Debug, Error)]
pub enum QueryError {
    #[error("Row not found")]
    RowNotFound,
    #[error("Transport error: {0}")]
    TransportError(Box<dyn Error + Send + Sync>),
    #[error("RequestError: {0}")]
    RequestError(#[from] reqwest::Error),
}

#[derive(Error, Debug)]

pub enum ConnectionError {
    #[error("TcpConnection failed: {0:?}")]
    TcpConnection(#[from] std::io::Error),
    #[error("Authenticate session failed: {0:?}")]
    AuthenticateSession(String),
}

#[derive(Debug, Error)]
pub enum HttpTransportError {
    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    #[error("Failed to read headers")]
    NoHeaders,
}

impl From<HttpTransportError> for QueryError {
    fn from(value: HttpTransportError) -> Self {
        Self::TransportError(Box::new(value))
    }
}
