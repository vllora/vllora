use crate::async_openai::{config::OpenAIConfig, Client};

use crate::types::credentials::ApiKeyCredentials;

use crate::client::error::ModelError;

pub fn openai_spec_client(
    credentials: Option<&ApiKeyCredentials>,
    endpoint: Option<&str>,
    provider_name: &str,
) -> Result<async_openai::Client<async_openai::config::OpenAIConfig>, ModelError> {
    let mut config = OpenAIConfig::new();

    if let Some(credentials) = credentials {
        config = config.with_api_key(credentials.api_key.clone());
    } else {
        let key_name = format!(
            "VLLORA_{provider_name}_API_KEY",
            provider_name = provider_name.to_uppercase()
        );
        if let Ok(api_key) = std::env::var(&key_name) {
            config = config.with_api_key(api_key);
        }
    }

    let api_base = match endpoint {
        Some(endpoint) => endpoint,
        None => {
            return Err(ModelError::InvalidDynamicProviderBaseUrl);
        }
    };

    config = config.with_api_base(api_base);

    Ok(Client::with_config(config))
}
