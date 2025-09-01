use crate::model::ModelProviderInstance;
use crate::models::{InferenceProvider, ModelCapability, ModelIOFormats, ModelMetadata, ModelType};
use crate::types::credentials::{ApiKeyCredentials, Credentials};
use crate::types::provider::{CompletionModelPrice, InferenceModelProvider, ModelPrice};
use crate::GatewayApiError;
use async_trait::async_trait;

use super::gemini::client::Client as GeminiClient;
use super::gemini::model::gemini_client;

pub struct GoogleVertexModelProvider {
    client: GeminiClient,
}

impl GoogleVertexModelProvider {
    pub fn new(credentials: Credentials) -> Result<Self, GatewayApiError> {
        // Support API key credentials (Gemini API).
        // Vertex service-account flow is not yet supported here.
        let api_key = match credentials {
            Credentials::ApiKey(ApiKeyCredentials { api_key }) => ApiKeyCredentials { api_key },
            Credentials::ApiKeyWithEndpoint { api_key, .. } => ApiKeyCredentials { api_key },
            _ => {
                return Err(GatewayApiError::CustomError(
                    "Google Gemini requires an API key credential".to_string(),
                ))
            }
        };

        let client = gemini_client(Some(&api_key))
            .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

        Ok(Self { client })
    }
}

#[async_trait]
impl ModelProviderInstance for GoogleVertexModelProvider {
    async fn get_private_models(&self) -> Result<Vec<ModelMetadata>, GatewayApiError> {
        let resp = self
            .client
            .models()
            .await
            .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

        let mut out = Vec::new();

        for m in resp.models {
            // The Gemini REST API returns names like "models/gemini-1.5-pro-latest"
            let model_name = m
                .name
                .split('/')
                .last()
                .map(|s| s.to_string())
                .unwrap_or(m.name.clone());

            // Default assumptions similar to azure.rs: text-only IO and completions
            let input_formats = vec![ModelIOFormats::Text];
            let output_formats = vec![ModelIOFormats::Text];

            // Enable tools support for Gemini models
            let capabilities = vec![ModelCapability::Tools];

            let limits = m.input_token_limit.unwrap_or(0) as u32;

            let metadata = ModelMetadata {
                model: model_name.clone(),
                model_provider: "google".to_string(),
                inference_provider: InferenceProvider {
                    provider: InferenceModelProvider::Gemini,
                    model_name: model_name.clone(),
                    endpoint: None,
                },
                price: ModelPrice::Completion(CompletionModelPrice {
                    per_input_token: 0.0,
                    per_output_token: 0.0,
                    per_cached_input_token: None,
                    per_cached_input_write_token: None,
                    valid_from: None,
                }),
                input_formats,
                output_formats,
                capabilities,
                r#type: ModelType::Completions,
                limits: crate::models::Limits::new(limits),
                description: m.description,
                parameters: None,
                benchmark_info: None,
                virtual_model_id: None,
                min_service_level: 0,
                release_date: None,
                license: None,
                knowledge_cutoff_date: None,
            };

            out.push(metadata);
        }

        Ok(out)
    }
}

