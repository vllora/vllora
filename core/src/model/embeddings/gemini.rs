use std::collections::HashMap;

use async_openai::types::{CreateEmbeddingResponse, Embedding, EmbeddingUsage};
use tracing::Span;

use crate::{
    events::SPAN_GEMINI,
    model::{
        embeddings::EmbeddingsModelInstance,
        error::ModelError,
        gemini::{
            client::Client,
            model::gemini_client,
            types::{Part, PartWithThought},
        },
        types::{LLMFinishEvent, ModelEvent, ModelEventType, ModelFinishReason},
        CredentialsIdent,
    },
    types::{
        credentials::ApiKeyCredentials,
        embed::EmbeddingResult,
        gateway::{CompletionModelUsage, CreateEmbeddingRequest, Input},
    },
    GatewayResult,
};

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
                .unwrap_or(CredentialsIdent::Langdb),
        })
    }
}

#[async_trait::async_trait]
impl EmbeddingsModelInstance for GeminiEmbeddings {
    async fn embed(
        &self,
        request: &CreateEmbeddingRequest,
        outer_tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        _tags: HashMap<String, String>,
    ) -> GatewayResult<EmbeddingResult> {
        let contents = match &request.input {
            Input::String(s) => vec![Part::Text(s.clone())],
            Input::Array(vec) => vec.iter().map(|s| Part::Text(s.clone())).collect(),
        };
        let response = self
            .client
            .embeddings(
                &request.model,
                crate::model::gemini::types::CreateEmbeddingRequest {
                    content: crate::model::gemini::types::ContentPart {
                        parts: contents.clone(),
                    },
                    task_type: None,
                    title: None,
                    output_dimensionality: request.dimensions,
                },
            )
            .await?;

        let tokens_count = self
            .client
            .count_tokens(
                &request.model,
                crate::model::gemini::types::CountTokensRequest {
                    contents: crate::model::gemini::types::Content::user_with_multiple_parts(
                        contents
                            .iter()
                            .map(|c| PartWithThought::from(c.clone()))
                            .collect(),
                    ),
                },
            )
            .await?;

        let span = Span::current();
        let _ = outer_tx
            .send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStop(LLMFinishEvent {
                    provider_name: SPAN_GEMINI.to_string(),
                    model_name: request.model.clone(),
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

        let response = EmbeddingResult::Float(CreateEmbeddingResponse {
            object: "list".to_string(),
            data: vec![Embedding {
                object: "embedding".to_string(),
                embedding: response.embedding.values,
                index: 0,
            }],
            model: request.model.clone(),
            usage: EmbeddingUsage {
                prompt_tokens: tokens_count.total_tokens as u32,
                total_tokens: tokens_count.total_tokens as u32,
            },
        });

        Ok(response)
    }
}
