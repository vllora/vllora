use thiserror::Error;
use tokio::task::JoinError;
pub mod completions;
mod tracing;
mod types;
pub use tracing::init_tracing;
pub type Result<T> = core::result::Result<T, InvokeError>;
#[derive(Error, Debug)]
pub enum InvokeError {
    #[error(transparent)]
    StdIOError(#[from] std::io::Error),
    #[error(transparent)]
    ParseError(#[from] serde_json::Error),
    #[error("{0}")]
    CustomError(String),
    #[error("Unsupported function: {0}")]
    Unsupported(String),
    #[error("Task join error: {0}")]
    JoinError(#[from] JoinError),
    #[error("Other error: {0}")]
    Other(String),
}

impl From<async_openai::error::OpenAIError> for InvokeError {
    fn from(err: async_openai::error::OpenAIError) -> Self {
        InvokeError::Other(err.to_string())
    }
}
