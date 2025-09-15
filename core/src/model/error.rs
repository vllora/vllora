use async_openai::error::OpenAIError;
use aws_sdk_bedrock::error::DisplayErrorContext;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ModelError {
    #[error("Credentials for '{0}' are invalid or missing")]
    CredentialsError(String),

    #[error("Model stopped with error: {0}")]
    FinishError(ModelFinishError),

    #[error("OpenAI tool not found: {0}")]
    ToolNotFoundError(String),

    #[error("Stream error: {0:?}")]
    StreamError(String),

    #[error("Custom error: {0:?}")]
    CustomError(String),

    #[error("Missing role {0}")]
    RoleIsMissing(String),

    #[error(transparent)]
    OpenAIApi(#[from] OpenAIError),

    #[error(transparent)]
    Bedrock(#[from] Box<BedrockError>),

    #[error(transparent)]
    Anthropic(#[from] AnthropicError),

    #[error("Max retries reached")]
    MaxRetriesReached,

    #[error("View should return a model name, {0}")]
    RoutingError(String),

    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),

    #[error("Invalid Dynamic provider Base URL")]
    InvalidDynamicProviderBaseUrl,

    #[error("Tool call id not found in request")]
    ToolCallIdNotFound,

    #[error(transparent)]
    AuthorizationError(#[from] AuthorizationError),

    #[error("Model {0} not found")]
    ModelNotFound(String),

    #[error("Cannot calculate input tokens")]
    CannotCalculateInputTokens,
}

impl From<BedrockError> for ModelError {
    fn from(value: BedrockError) -> Self {
        ModelError::Bedrock(Box::new(value))
    }
}

#[derive(Error, Debug)]
pub enum ModelFinishError {
    #[error("Content filter blocked the completion")]
    ContentFilter,

    #[error("The maximum number of tokens specified in the request was reached")]
    MaxTokens,

    #[error("Guardrail intervened and stopped this execution")]
    GuardrailIntervened,

    #[error("Tool missing content")]
    ToolMissingContent,

    #[error("Tool use doesnt have message")]
    ToolUseDoesntHaveMessage,

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("No output provided")]
    NoOutputProvided,

    #[error("Content block is not in a text format. Currently only TEXT format supported")]
    ContentBlockNotInTextFormat,

    #[error("{0}")]
    Custom(String),
}

#[derive(Error, Debug)]
pub enum AuthorizationError {
    #[error("Invalid API Key")]
    InvalidApiKey,
}

#[derive(Error, Debug)]
pub enum AnthropicError {
    #[error(transparent)]
    ClustError(#[from] clust::ClientError),

    #[error("Error building request: {0}")]
    RequestError(String),
}

#[derive(Error, Debug)]
pub enum BedrockError {
    #[error("Custom Error: {0}")]
    CustomError(String),

    #[error("Validation Error: {0}")]
    ValidationError(String),

    #[error("Timeout occurred: {0}")]
    TimeoutError(String), // Adding a more specific error for timeout issues

    #[error("Invalid credentials: {0}")]
    AuthenticationError(String), // Adding a specific error for authentication failures

    #[error("{}", DisplayErrorContext(.0))]
    SmithyError(
        #[from]
        aws_smithy_runtime_api::client::result::SdkError<
            aws_sdk_bedrockruntime::types::error::ConverseStreamOutputError,
            aws_smithy_types::event_stream::RawMessage,
        >,
    ),

    #[error("{}", DisplayErrorContext(.0))]
    ConverseError(
        #[from]
        aws_smithy_runtime_api::client::result::SdkError<
            aws_sdk_bedrockruntime::operation::converse::ConverseError,
            aws_smithy_runtime_api::http::Response,
        >,
    ),

    #[error("{}", DisplayErrorContext(.0))]
    ResponseError(
        #[from]
        aws_smithy_runtime_api::client::result::SdkError<
            aws_sdk_bedrockruntime::operation::converse_stream::ConverseStreamError,
            aws_smithy_runtime_api::http::Response,
        >,
    ),
}
