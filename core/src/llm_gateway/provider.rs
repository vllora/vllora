use vllora_llm::types::models::ModelMetadata;

use vllora_llm::types::credentials::ApiKeyCredentials;
use vllora_llm::types::credentials::Credentials;
use vllora_llm::types::engine::{EmbeddingsEngineParams, ImageGenerationEngineParams};
use vllora_llm::types::gateway::{CreateEmbeddingRequest, CreateImageRequest};
use vllora_llm::types::provider::InferenceModelProvider;

use crate::error::GatewayError;

pub struct Provider {}

impl Provider {
    pub fn get_image_engine_for_model(
        model: &ModelMetadata,
        request: &CreateImageRequest,
        credentials: Option<&Credentials>,
    ) -> Result<ImageGenerationEngineParams, GatewayError> {
        match model.inference_provider.provider {
            InferenceModelProvider::OpenAI => {
                let mut custom_endpoint = None;
                Ok(ImageGenerationEngineParams::OpenAi {
                    credentials: credentials.and_then(|cred| match cred {
                        Credentials::ApiKey(key) => Some(key.clone()),
                        Credentials::ApiKeyWithEndpoint { api_key, endpoint } => {
                            custom_endpoint = Some(endpoint.clone());
                            Some(ApiKeyCredentials {
                                api_key: api_key.clone(),
                            })
                        }
                        _ => None,
                    }),
                    model_name: request.model.clone(),
                    endpoint: custom_endpoint,
                })
            }
            InferenceModelProvider::Proxy(_) => Ok(ImageGenerationEngineParams::VlloraOpen {
                credentials: credentials.and_then(|cred| match cred {
                    Credentials::ApiKey(key) => Some(key.clone()),
                    _ => None,
                }),
                model_name: request.model.clone(),
            }),
            InferenceModelProvider::VertexAI
            | InferenceModelProvider::Anthropic
            | InferenceModelProvider::Gemini
            | InferenceModelProvider::Bedrock => Err(GatewayError::UnsupportedProvider(
                model.inference_provider.provider.to_string(),
            )),
        }
    }

    pub fn get_embeddings_engine_for_model(
        model: &ModelMetadata,
        request: &CreateEmbeddingRequest,
        credentials: Option<&Credentials>,
    ) -> Result<EmbeddingsEngineParams, GatewayError> {
        match model.inference_provider.provider {
            InferenceModelProvider::OpenAI | InferenceModelProvider::Proxy(_) => {
                let mut custom_endpoint = None;
                Ok(EmbeddingsEngineParams::OpenAi {
                    credentials: credentials.and_then(|cred| match cred {
                        Credentials::ApiKey(key) => Some(key.clone()),
                        Credentials::ApiKeyWithEndpoint { api_key, endpoint } => {
                            custom_endpoint = Some(endpoint.clone());
                            Some(ApiKeyCredentials {
                                api_key: api_key.clone(),
                            })
                        }
                        _ => None,
                    }),
                    model_name: request.model.clone(),
                    endpoint: custom_endpoint,
                })
            }
            InferenceModelProvider::Gemini => Ok(EmbeddingsEngineParams::Gemini {
                credentials: credentials.and_then(|cred| match cred {
                    Credentials::ApiKey(key) => Some(key.clone()),
                    _ => None,
                }),
                model_name: request.model.clone(),
            }),
            InferenceModelProvider::Bedrock => Ok(EmbeddingsEngineParams::Bedrock {
                credentials: credentials.and_then(|cred| match cred {
                    Credentials::Aws(cred) => Some(cred.clone()),
                    _ => None,
                }),
                model_name: request.model.clone(),
            }),
            InferenceModelProvider::VertexAI | InferenceModelProvider::Anthropic => Err(
                GatewayError::UnsupportedProvider(model.inference_provider.provider.to_string()),
            ),
        }
    }
}
