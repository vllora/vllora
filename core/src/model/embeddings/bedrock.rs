use super::CredentialsIdent;
use crate::model::bedrock::bedrock_client;
use crate::model::error::ModelError;
use crate::model::types::{
    LLMFinishEvent, LLMStartEvent, ModelEvent, ModelEventType, ModelFinishReason,
};
use crate::telemetry::events::{JsonValue, SPAN_BEDROCK};
use crate::types::credentials::BedrockCredentials;
use crate::types::embed::EmbeddingResult;
use crate::types::gateway::{CompletionModelUsage, CreateEmbeddingRequest, Input};
use crate::{create_model_span, GatewayResult};
use async_openai::types::{CreateEmbeddingResponse, Embedding, EmbeddingUsage};
use aws_sdk_bedrockruntime::Client;
use aws_smithy_types::Blob;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use tracing::field;
use tracing::Span;
use tracing_futures::Instrument;
use valuable::Valuable;

use super::EmbeddingsModelInstance;

#[derive(Debug, Deserialize)]
pub struct AmazonTitanEmbeddingResponse {
    embedding: Vec<f32>,
    #[serde(alias = "inputTextTokenCount")]
    input_text_token_count: u32,
}

#[derive(Debug, Deserialize)]
pub struct CohereEmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Serialize)]
pub struct CohereEmbeddingRequest {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    texts: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<String>,
    input_type: CohereEmbeddingInputType,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CohereEmbeddingInputType {
    #[default]
    SearchDocument,
    SearchQuery,
    Classification,
    Clustering,
    Image,
}

#[derive(Debug, Serialize)]
pub struct AmazonTitanEmbeddingRequest {
    #[serde(rename = "inputText")]
    input_text: String,
}

pub enum BedrockEmbeddingProvider {
    Cohere,
    Other(String),
}

pub struct BedrockEmbeddings {
    pub client: Client,
    pub credentials_ident: CredentialsIdent,
}

macro_rules! target {
    () => {
        "langdb::user_tracing::models::bedrock"
    };
    ($subtgt:literal) => {
        concat!("langdb::user_tracing::models::bedrock::", $subtgt)
    };
}

impl BedrockEmbeddings {
    pub async fn new(credentials: Option<&BedrockCredentials>) -> Result<Self, ModelError> {
        let client = bedrock_client(credentials).await?;
        Ok(BedrockEmbeddings {
            client,
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Langdb),
        })
    }

    async fn execute(
        &self,
        request: &CreateEmbeddingRequest,
        outer_tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
    ) -> GatewayResult<EmbeddingResult> {
        let span = Span::current();
        let _ = outer_tx.try_send(Some(ModelEvent::new(
            &span,
            ModelEventType::LlmStart(LLMStartEvent {
                provider_name: "bedrock".to_string(),
                model_name: request.model.clone(),
                input: serde_json::to_string(&request)?,
            }),
        )));

        let builder = self.client.invoke_model();

        let provider = match_provider(&request.model);
        let (blob, input_tokens) = generate_invoke_model_input(&request.input, &provider)?;

        let invoke = builder
            .model_id(request.model.clone())
            .body(blob)
            .send()
            .await
            .map_err(|e| ModelError::CustomError(e.to_string()))?;

        let response = map_response(invoke.body, &request.model, &provider, input_tokens)?;

        let _ = outer_tx
            .send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStop(LLMFinishEvent {
                    provider_name: SPAN_BEDROCK.to_string(),
                    model_name: request.model.clone(),
                    output: None,
                    usage: Some(CompletionModelUsage {
                        input_tokens: response.usage().prompt_tokens,
                        output_tokens: 0,
                        total_tokens: response.usage().total_tokens,
                        ..Default::default()
                    }),
                    finish_reason: ModelFinishReason::Stop,
                    tool_calls: vec![],
                    credentials_ident: self.credentials_ident.clone(),
                }),
            )))
            .await;

        span.record(
            "raw_usage",
            JsonValue(&serde_json::to_value(response.usage())?).as_value(),
        );
        span.record(
            "usage",
            JsonValue(&serde_json::to_value(Self::map_usage(response.usage()))?).as_value(),
        );

        Ok(response)
    }

    fn map_usage(usage: &EmbeddingUsage) -> CompletionModelUsage {
        CompletionModelUsage {
            input_tokens: usage.prompt_tokens,
            total_tokens: usage.total_tokens,
            ..Default::default()
        }
    }
}

#[async_trait::async_trait]
impl EmbeddingsModelInstance for BedrockEmbeddings {
    async fn embed(
        &self,
        request: &CreateEmbeddingRequest,
        outer_tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<EmbeddingResult> {
        let span = create_model_span!(
            SPAN_BEDROCK,
            target!("embedding"),
            tags,
            0,
            input = serde_json::to_string(&request)?
        );

        self.execute(request, &outer_tx)
            .instrument(span.clone())
            .await
    }
}

fn match_provider(model_name: &str) -> BedrockEmbeddingProvider {
    let model_name = model_name
        .split("/")
        .collect::<Vec<&str>>()
        .last()
        .map_or(model_name, |p| p);
    let provider = model_name.split(".").next();
    match provider {
        Some("cohere") => BedrockEmbeddingProvider::Cohere,
        Some(name) => BedrockEmbeddingProvider::Other(name.to_string()),
        None => BedrockEmbeddingProvider::Other(model_name.to_string()),
    }
}

fn generate_invoke_model_input(
    input: &Input,
    provider: &BedrockEmbeddingProvider,
) -> Result<(Blob, u32), ModelError> {
    match provider {
        BedrockEmbeddingProvider::Cohere => match input {
            Input::String(s) => {
                let request = CohereEmbeddingRequest {
                    texts: vec![s.clone()],
                    images: vec![],
                    input_type: CohereEmbeddingInputType::default(),
                };
                Ok((
                    Blob::new(serde_json::to_string(&request)?),
                    (s.len() / 3)
                        .try_into()
                        .map_err(|_| ModelError::CannotCalculateInputTokens)?,
                ))
            }
            Input::Array(vec) => {
                let len = vec.iter().map(|s| s.len()).sum::<usize>();
                let request = CohereEmbeddingRequest {
                    texts: vec.clone(),
                    images: vec![],
                    input_type: CohereEmbeddingInputType::default(),
                };
                Ok((
                    Blob::new(serde_json::to_string(&request)?),
                    (len / 3)
                        .try_into()
                        .map_err(|_| ModelError::CannotCalculateInputTokens)?,
                ))
            }
        },
        BedrockEmbeddingProvider::Other(_) => match input {
            Input::String(s) => {
                let request = AmazonTitanEmbeddingRequest {
                    input_text: s.clone(),
                };
                Ok((Blob::new(serde_json::to_string(&request)?), 0))
            }
            Input::Array(vec) => {
                let request = AmazonTitanEmbeddingRequest {
                    input_text: vec.join("\n"),
                };
                Ok((Blob::new(serde_json::to_string(&request)?), 0))
            }
        },
    }
}

fn map_response(
    response: Blob,
    model: &str,
    provider: &BedrockEmbeddingProvider,
    input_tokens: u32,
) -> Result<EmbeddingResult, ModelError> {
    let bytes = response.into_inner();
    match provider {
        BedrockEmbeddingProvider::Cohere => {
            let response: CohereEmbeddingResponse = serde_json::from_slice(&bytes)?;
            Ok(EmbeddingResult::Float(CreateEmbeddingResponse {
                data: response
                    .embeddings
                    .into_iter()
                    .enumerate()
                    .map(|(index, embedding)| Embedding {
                        object: "embedding".to_string(),
                        embedding: embedding.clone(),
                        index: index as u32,
                    })
                    .collect(),
                model: model.to_string(),
                object: "list".to_string(),
                usage: EmbeddingUsage {
                    prompt_tokens: input_tokens,
                    total_tokens: input_tokens,
                },
            }))
        }
        BedrockEmbeddingProvider::Other(_) => {
            let response: AmazonTitanEmbeddingResponse = serde_json::from_slice(&bytes)?;
            Ok(EmbeddingResult::Float(CreateEmbeddingResponse {
                data: vec![Embedding {
                    object: "embedding".to_string(),
                    embedding: response.embedding,
                    index: 0,
                }],
                model: model.to_string(),
                object: "list".to_string(),
                usage: EmbeddingUsage {
                    prompt_tokens: response.input_text_token_count,
                    total_tokens: response.input_text_token_count,
                },
            }))
        }
    }
}
