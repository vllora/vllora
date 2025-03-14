use std::collections::HashMap;

use clust::messages::StopSequence;

use crate::{
    models::ModelMetadata,
    types::{
        credentials::{ApiKeyCredentials, Credentials},
        engine::{
            AnthropicModelParams, BedrockModelParams, ClaudeModel, CompletionEngineParams,
            GeminiModelParams, ImageGenerationEngineParams, OpenAiModelParams,
        },
        gateway::{ChatCompletionRequest, CreateImageRequest, ProviderSpecificRequest},
        provider::{BedrockProvider, InferenceModelProvider},
    },
};

use crate::error::GatewayError;

pub struct Provider {}

impl Provider {
    pub fn get_completion_engine_for_model(
        model: &ModelMetadata,
        request: &ChatCompletionRequest,
        credentials: Option<Credentials>,
        provider_specific: Option<&ProviderSpecificRequest>,
    ) -> Result<CompletionEngineParams, GatewayError> {
        match model.inference_provider.provider {
            InferenceModelProvider::OpenAI | InferenceModelProvider::Proxy(_) => {
                let params = OpenAiModelParams {
                    model: Some(model.inference_provider.model_name.clone()),
                    frequency_penalty: request.frequency_penalty,
                    logit_bias: request.logit_bias.clone(),
                    logprobs: None,
                    top_logprobs: None,
                    max_tokens: request.max_tokens,
                    presence_penalty: request.presence_penalty,
                    seed: request.seed,
                    stop: request.stop.clone(),
                    temperature: request.temperature,
                    top_p: request.top_p,
                    user: request.user.clone(),
                    response_format: request.response_format.clone(),
                };
                let mut custom_endpoint = None;
                let api_key_credentials = credentials.and_then(|cred| match cred {
                    Credentials::ApiKey(key) => Some(key),
                    Credentials::ApiKeyWithEndpoint {
                        api_key: key,
                        endpoint,
                    } => {
                        custom_endpoint = Some(endpoint);
                        Some(ApiKeyCredentials { api_key: key })
                    }
                    _ => None,
                });
                if model.inference_provider.provider == InferenceModelProvider::OpenAI {
                    Ok(CompletionEngineParams::OpenAi {
                        params,
                        execution_options: Default::default(),
                        credentials: api_key_credentials,
                        endpoint: custom_endpoint,
                    })
                } else {
                    Ok(CompletionEngineParams::Proxy {
                        params,
                        execution_options: Default::default(),
                        credentials: api_key_credentials,
                    })
                }
            }
            InferenceModelProvider::Bedrock => {
                let aws_creds = match credentials {
                    Some(Credentials::Aws(aws)) => Some(aws),
                    _ => None,
                };
                let provider = match model.model_provider.as_str() {
                    "cohere" => BedrockProvider::Cohere,
                    "meta" => BedrockProvider::Meta,
                    "mistral" => BedrockProvider::Mistral,
                    p => BedrockProvider::Other(p.to_string()),
                };
                Ok(CompletionEngineParams::Bedrock {
                    credentials: aws_creds,
                    execution_options: Default::default(),
                    params: BedrockModelParams {
                        model_id: Some(model.inference_provider.model_name.clone()),
                        max_tokens: request.max_tokens.map(|x| x as i32),
                        temperature: request.temperature,
                        top_p: request.top_p,
                        stop_sequences: request.stop.clone(),
                        additional_parameters: HashMap::new(),
                    },
                    provider,
                })
            }
            InferenceModelProvider::Anthropic => {
                let api_key_credentials = credentials.and_then(|cred| match cred {
                    Credentials::ApiKey(key) => Some(key),
                    _ => None,
                });
                let model_name = get_anthropic_model(&model.inference_provider.model_name);
                let model = serde_json::from_str::<ClaudeModel>(&format!("\"{model_name}\""))?;
                Ok(CompletionEngineParams::Anthropic {
                    credentials: api_key_credentials,
                    execution_options: Default::default(),
                    params: AnthropicModelParams {
                        model: Some(model.clone()),
                        max_tokens: match request.max_tokens {
                            Some(x) => Some(clust::messages::MaxTokens::new(x, model.model)?),
                            None => None,
                        },
                        stop_sequences: request
                            .stop
                            .as_ref()
                            .map(|s| s.iter().map(StopSequence::new).collect()),
                        stream: None,
                        temperature: match request.temperature {
                            Some(t) => Some(clust::messages::Temperature::new(t)?),
                            None => None,
                        },
                        top_p: match request.top_p {
                            Some(p) => Some(clust::messages::TopP::new(p)?),
                            None => None,
                        },
                        top_k: provider_specific
                            .and_then(|ps| ps.top_k.map(clust::messages::TopK::new)),
                        thinking: provider_specific.and_then(|ps| {
                            ps.thinking
                                .as_ref()
                                .map(|thinking| clust::messages::Thinking {
                                    r#type: thinking.r#type.clone(),
                                    budget_tokens: thinking.budget_tokens,
                                })
                        }),
                    },
                })
            }
            InferenceModelProvider::Gemini => {
                let api_key_credentials = credentials.and_then(|cred| match cred {
                    Credentials::ApiKey(key) => Some(key),
                    _ => None,
                });
                Ok(CompletionEngineParams::Gemini {
                    credentials: api_key_credentials,
                    execution_options: Default::default(),
                    params: GeminiModelParams {
                        model: Some(model.inference_provider.model_name.clone()),
                        max_output_tokens: request.max_tokens.map(|x| x as i32),
                        temperature: request.temperature,
                        top_p: request.top_p,
                        stop_sequences: request.stop.clone(),
                        candidate_count: request.n,
                        presence_penalty: request.presence_penalty,
                        frequency_penalty: request.frequency_penalty,
                        seed: request.seed,
                        // Not supported by request inteface
                        // response_logprobs: request.response_logprobs,
                        // logprobs: request.logprobs,
                        // top_k: request.top_k,
                        response_logprobs: None,
                        logprobs: None,
                        top_k: None,
                    },
                })
            }
        }
    }

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
            InferenceModelProvider::Proxy(_) => Ok(ImageGenerationEngineParams::LangdbOpen {
                credentials: credentials.and_then(|cred| match cred {
                    Credentials::ApiKey(key) => Some(key.clone()),
                    _ => None,
                }),
                model_name: request.model.clone(),
            }),
            InferenceModelProvider::Anthropic
            | InferenceModelProvider::Gemini
            | InferenceModelProvider::Bedrock => Err(GatewayError::CustomError(format!(
                "Unsupported provider: {}",
                model.inference_provider.model_name
            ))),
        }
    }
}

/// Handles Anthropic model names without versions.
///
/// This function attempts to parse the given model name into a `ClaudeModel` enum variant.
/// It's designed to handle model names that may not include specific version numbers.
///
/// # Arguments
///
/// * `model_name` - A string slice that holds the name of the Anthropic model.
fn get_anthropic_model(model_name: &str) -> &str {
    match model_name {
        "claude-3-opus" => "claude-3-opus-20240229",
        "claude-3-sonnet" => "claude-3-sonnet-20240229",
        "claude-3-haiku" => "claude-3-haiku-20240307",
        "claude-3-5-sonnet" => "claude-3-5-sonnet-20240620",
        n => n,
    }
}
