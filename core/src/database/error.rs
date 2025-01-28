use thiserror::Error;

use super::ssh_tunnel::SshTunnelError;
use std::error::Error;
use tokio_util::codec::LinesCodecError;

#[derive(Debug, Error)]
pub enum QueryError {
    #[error("Row not found")]
    RowNotFound,
    #[error("Error executing query: {0}")]
    ClickhouseError(String),
    #[error("Transport error: {0}")]
    TransportError(Box<dyn Error + Send + Sync>),
    #[error("Ssh tunnel error : {0:?}")]
    SshConnectionError(#[from] SshTunnelError),
    #[error("RequestError: {0}")]
    RequestError(#[from] reqwest::Error),
}

#[derive(Error, Debug)]

pub enum ConnectionError {
    #[error("TcpConnection failed: {0:?}")]
    TcpConnection(#[from] std::io::Error),
    #[error("SessionCreation failed: {0:?}")]
    SshFailed(#[from] SshTunnelError),
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

    #[error(transparent)]
    LinesCodec(#[from] LinesCodecError),
}

impl From<HttpTransportError> for QueryError {
    fn from(value: HttpTransportError) -> Self {
        Self::TransportError(Box::new(value))
    }
}
