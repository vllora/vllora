use crate::{
    client::error::{AuthorizationError, ModelError},
    types::credentials::ApiKeyCredentials,
};
use async_openai::{
    config::{AzureConfig, OpenAIConfig},
    Client,
};

pub mod completions;
pub mod responses;

/// Helper function to determine if an endpoint is for Azure OpenAI
pub fn is_azure_endpoint(endpoint: &str) -> bool {
    endpoint.contains("azure.com")
}

/// Create an OpenAI client with standard OpenAI configuration
/// Note: This does not handle Azure OpenAI endpoints. Use azure_openai_client for Azure endpoints.
pub fn openai_client(
    credentials: Option<&ApiKeyCredentials>,
    endpoint: Option<&str>,
) -> Result<Client<OpenAIConfig>, ModelError> {
    let api_key = if let Some(credentials) = credentials {
        credentials.api_key.clone()
    } else {
        std::env::var("VLLORA_OPENAI_API_KEY").map_err(|_| AuthorizationError::InvalidApiKey)?
    };

    let mut config = OpenAIConfig::new();
    config = config.with_api_key(api_key);

    if let Some(endpoint) = endpoint {
        // Do not handle Azure endpoints here
        if is_azure_endpoint(endpoint) {
            return Err(ModelError::CustomError(format!(
                "Azure endpoints should be handled by azure_openai_client, not openai_client: {endpoint}"
            )));
        }

        // For custom non-Azure endpoints
        config = config.with_api_base(endpoint);
    }

    Ok(Client::with_config(config))
}

/// Create an Azure OpenAI client from endpoint URL
pub fn azure_openai_client(
    api_key: String,
    endpoint: &str,
    deployment_id: &str,
) -> Client<AzureConfig> {
    let azure_config = AzureConfig::new()
        .with_api_base(endpoint)
        .with_api_version("2024-10-21".to_string())
        .with_api_key(api_key)
        .with_deployment_id(deployment_id.to_string());

    Client::with_config(azure_config)
}
