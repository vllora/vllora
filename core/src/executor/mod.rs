use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::credentials::{ApiKeyCredentials, Credentials};

pub mod chat_completion;
pub mod embeddings;
pub mod image_generation;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProvidersConfig(HashMap<String, ApiKeyCredentials>);

pub fn get_key_credentials(
    key_credentials: Option<&Credentials>,
    providers_config: Option<&ProvidersConfig>,
    provider_name: &str,
) -> Option<Credentials> {
    match key_credentials {
        Some(credentials) => Some(credentials.clone()),
        None => match providers_config {
            Some(providers_config) => providers_config
                .0
                .get(provider_name)
                .map(|credentials| Credentials::ApiKey(credentials.clone())),
            None => None,
        },
    }
}
