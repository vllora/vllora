use std::collections::HashMap;

use async_openai::types::{CreateEmbeddingResponse, Embedding, EmbeddingUsage};
use tracing::field;
use tracing::Span;
use tracing_futures::Instrument;
use valuable::Valuable;
use vllora_llm::types::LLMFinishEvent;

use crate::{
    model::{embeddings::EmbeddingsModelInstance, CredentialsIdent},
    types::embed::EmbeddingResult,
};
use vllora_llm::client::error::ModelError;
use vllora_llm::error::LLMResult;
use vllora_llm::provider::gemini::{
    client::Client,
    model::gemini_client,
    types::{Part, PartWithThought},
};
use vllora_llm::types::credentials::ApiKeyCredentials;
use vllora_llm::types::gateway::{CompletionModelUsage, CreateEmbeddingRequest, Input};
use vllora_llm::types::LLMStartEvent;
use vllora_llm::types::ModelEvent;
use vllora_llm::types::ModelEventType;
use vllora_llm::types::ModelFinishReason;
use vllora_telemetry::create_model_span;
use vllora_telemetry::events::JsonValue;
use vllora_telemetry::events::SPAN_GEMINI;

macro_rules! target {
    () => {
        "vllora::user_tracing::models::gemini"
    };
    ($subtgt:literal) => {
        concat!("vllora::user_tracing::models::gemini::", $subtgt)
    };
}

pub struct GeminiEmbeddings {
    client: Client,
    credentials_ident: CredentialsIdent,
}

impl GeminiEmbeddings {
    pub fn new(credentials: Option<&ApiKeyCredentials>) -> Result<Self, ModelError> {
        let client = gemini_client(credentials)?;
        Ok(GeminiEmbeddings {
            client,
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Vllora),
        })
    }

    async fn execute(
        &self,
        embedding_request: vllora_llm::provider::gemini::types::CreateEmbeddingRequest,
        token_count_request: vllora_llm::provider::gemini::types::CountTokensRequest,
        model_name: &str,
        outer_tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
    ) -> LLMResult<EmbeddingResult> {
        let span = Span::current();
        let _ = outer_tx.try_send(Some(ModelEvent::new(
            &span,
            ModelEventType::LlmStart(LLMStartEvent {
                provider_name: "gemini".to_string(),
                model_name: model_name.to_string(),
                input: serde_json::to_string(&embedding_request)?,
            }),
        )));

        let response = self
            .client
            .embeddings(model_name, embedding_request)
            .await?;

        let tokens_count = self
            .client
            .count_tokens(model_name, token_count_request)
            .await?;

        let _ = outer_tx
            .send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStop(LLMFinishEvent {
                    provider_name: SPAN_GEMINI.to_string(),
                    model_name: model_name.to_string(),
                    output: None,
                    usage: Some(CompletionModelUsage {
                        input_tokens: tokens_count.total_tokens as u32,
                        output_tokens: 0,
                        total_tokens: tokens_count.total_tokens as u32,
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
            JsonValue(&serde_json::to_value(tokens_count.clone())?).as_value(),
        );
        span.record(
            "usage",
            JsonValue(&serde_json::to_value(Self::map_usage(&tokens_count))?).as_value(),
        );

        Ok(EmbeddingResult::Float(CreateEmbeddingResponse {
            object: "list".to_string(),
            data: vec![Embedding {
                object: "embedding".to_string(),
                embedding: response.embedding.values,
                index: 0,
            }],
            model: model_name.to_string(),
            usage: EmbeddingUsage {
                prompt_tokens: tokens_count.total_tokens as u32,
                total_tokens: tokens_count.total_tokens as u32,
            },
        }))
    }

    fn map_usage(
        usage: &vllora_llm::provider::gemini::types::CountTokensResponse,
    ) -> CompletionModelUsage {
        CompletionModelUsage {
            input_tokens: usage.total_tokens as u32,
            total_tokens: usage.total_tokens as u32,
            ..Default::default()
        }
    }
}

#[async_trait::async_trait]
impl EmbeddingsModelInstance for GeminiEmbeddings {
    async fn embed(
        &self,
        request: &CreateEmbeddingRequest,
        outer_tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> LLMResult<EmbeddingResult> {
        let contents = match &request.input {
            Input::String(s) => vec![Part::Text(s.clone())],
            Input::Array(vec) => vec.iter().map(|s| Part::Text(s.clone())).collect(),
        };

        let embedding_request = vllora_llm::provider::gemini::types::CreateEmbeddingRequest {
            content: vllora_llm::provider::gemini::types::ContentPart {
                parts: contents.clone(),
            },
            task_type: None,
            title: None,
            output_dimensionality: request.dimensions,
        };

        let token_count_request = vllora_llm::provider::gemini::types::CountTokensRequest {
            contents: vllora_llm::provider::gemini::types::Content::user_with_multiple_parts(
                contents
                    .iter()
                    .map(|c| PartWithThought::from(c.clone()))
                    .collect(),
            ),
        };

        let span = create_model_span!(
            SPAN_GEMINI,
            target!("embedding"),
            tags,
            0,
            input = serde_json::to_string(&embedding_request)?
        );

        self.execute(
            embedding_request,
            token_count_request,
            &request.model,
            &outer_tx,
        )
        .instrument(span.clone())
        .await
    }
}
