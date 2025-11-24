use crate::model::bedrock::pricing;
use crate::model::ModelProviderInstance;
use crate::GatewayApiError;
use async_trait::async_trait;
use aws_sdk_bedrock::Client as BedrockClient;
use vllora_llm::client::error::ModelError;
use vllora_llm::error::LLMError;
use vllora_llm::provider::bedrock::get_sdk_config;
use vllora_llm::types::credentials::BedrockCredentials;
use vllora_llm::types::models::InferenceProvider;
use vllora_llm::types::models::Limits;
use vllora_llm::types::models::ModelCapability;
use vllora_llm::types::models::ModelIOFormats;
use vllora_llm::types::models::ModelMetadata;
use vllora_llm::types::models::ModelType;
use vllora_llm::types::provider::{CompletionModelPrice, InferenceModelProvider, ModelPrice};

pub struct BedrockModelProvider {
    client: BedrockClient,
}

impl BedrockModelProvider {
    pub async fn new(credentials: BedrockCredentials) -> Result<Self, LLMError> {
        let config = get_sdk_config(Some(&credentials))
            .await
            .map_err(|e| LLMError::CustomError(e.to_string()))?;
        let client = BedrockClient::new(&config);
        Ok(Self { client })
    }
}

#[async_trait]
impl ModelProviderInstance for BedrockModelProvider {
    async fn get_private_models(&self) -> Result<Vec<ModelMetadata>, GatewayApiError> {
        // List foundation models
        let response = self
            .client
            .list_foundation_models()
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to list Bedrock models: {:?}", e);
                GatewayApiError::LLMError(LLMError::ModelError(Box::new(ModelError::CustomError(
                    format!("Failed to list Bedrock models: {}", e),
                ))))
            })?;

        let mut models = Vec::new();

        let mut region_prefix = "";

        if let Some(region) = self.client.config().region() {
            let region = region.to_string();
            region_prefix = if region.starts_with("us") {
                "us."
            } else if region.starts_with("ap") {
                "apac."
            } else if region.starts_with("eu") {
                "eu."
            } else {
                ""
            };
        }

        let prices = pricing::fetch_pricing().await?;

        if let Some(model_summaries) = response.model_summaries {
            for model_summary in &model_summaries {
                // Extract model information
                let model_id = model_summary.model_id.clone();
                let model_arn = model_summary.model_arn.clone();
                if model_arn.ends_with("k") || model_arn.ends_with("m") {
                    continue;
                }

                let first_modality =
                    model_summary
                        .output_modalities
                        .as_ref()
                        .and_then(|output_modalities| {
                            output_modalities.iter().find(|m| {
                                [
                                    aws_sdk_bedrock::types::ModelModality::Embedding,
                                    aws_sdk_bedrock::types::ModelModality::Text,
                                ]
                                .contains(m)
                            })
                        });

                let model_type = match first_modality {
                    Some(aws_sdk_bedrock::types::ModelModality::Embedding) => ModelType::Embeddings,
                    Some(aws_sdk_bedrock::types::ModelModality::Text) => ModelType::Completions,
                    _ => continue,
                };

                let provider_name = model_summary.provider_name.clone().unwrap_or_default();
                let model_name = model_summary.model_name.clone().unwrap_or_default();

                // Determine capabilities based on modalities
                let mut capabilities = Vec::new();

                // Determine input/output formats from modalities
                let input_formats =
                    if let Some(input_modalities) = model_summary.input_modalities.as_ref() {
                        input_modalities
                            .iter()
                            .filter_map(|m| match m.as_str() {
                                "TEXT" => Some(ModelIOFormats::Text),
                                "IMAGE" => Some(ModelIOFormats::Image),
                                "VIDEO" => Some(ModelIOFormats::Video),
                                _ => None,
                            })
                            .collect()
                    } else {
                        vec![ModelIOFormats::Text]
                    };

                if input_formats.len() == 1 {
                    if let Some(input_format) = input_formats.first() {
                        if input_format == &ModelIOFormats::Video {
                            continue;
                        }
                    }
                }

                let output_formats =
                    if let Some(output_modalities) = model_summary.output_modalities.as_ref() {
                        output_modalities
                            .iter()
                            .filter_map(|m| match m.as_str() {
                                "TEXT" => Some(ModelIOFormats::Text),
                                "IMAGE" => Some(ModelIOFormats::Image),
                                _ => None,
                            })
                            .collect()
                    } else {
                        vec![ModelIOFormats::Text]
                    };

                let inference_provider_model_name =
                    if let Some(types) = model_summary.inference_types_supported.as_ref() {
                        if types.iter().any(|t| t.as_str() == "INFERENCE_PROFILE") {
                            format!("{region_prefix}{model_id}")
                        } else {
                            model_arn.clone()
                        }
                    } else {
                        model_arn.clone()
                    };

                let mut price = prices.get(&format!("{region_prefix}{model_id}"));

                if price.is_none() {
                    price = prices.get(&model_id);
                }

                if price.is_none() {
                    tracing::error!("Model is missing in pricing: {:#?}", model_summary);
                }

                // Check if model supports tools/functions based on known models
                if model_id.contains("claude") || model_id.contains("mistral") {
                    capabilities.push(ModelCapability::Tools);
                } else if let Some(price) = price {
                    if price.supports_function_calling.unwrap_or(false) {
                        capabilities.push(ModelCapability::Tools);
                    }
                }

                // Create ModelMetadata
                let metadata = ModelMetadata {
                    model: model_id.clone(),
                    model_provider: provider_name.clone().to_lowercase(),
                    inference_provider: InferenceProvider {
                        provider: InferenceModelProvider::Bedrock,
                        model_name: inference_provider_model_name,
                        endpoint: None,
                    },
                    price: ModelPrice::Completion(CompletionModelPrice {
                        per_input_token: price
                            .and_then(|p| {
                                p.input_cost_per_token
                                    .map(|c| ((c * 1000000.0) * 1000.0).round() / 1000.0)
                            })
                            .unwrap_or(0.0),
                        per_output_token: price
                            .and_then(|p| {
                                p.output_cost_per_token
                                    .map(|c| ((c * 1000000.0) * 1000.0).round() / 1000.0)
                            })
                            .unwrap_or(0.0),
                        per_cached_input_token: None,
                        per_cached_input_write_token: None,
                        valid_from: None,
                    }),
                    input_formats,
                    output_formats,
                    capabilities,
                    r#type: model_type,
                    limits: Limits::new(price.map(|p| p.max_tokens.unwrap_or(0)).unwrap_or(0)), // Default context size, would need model-specific values
                    description: model_name,
                    parameters: None,
                    benchmark_info: None,
                    virtual_model_id: None,
                    min_service_level: 0,
                    release_date: None,
                    license: None,
                    knowledge_cutoff_date: None,
                    langdb_release_date: None,
                    is_private: true,
                };

                models.push(metadata);
            }
        }

        Ok(models)
    }
}
