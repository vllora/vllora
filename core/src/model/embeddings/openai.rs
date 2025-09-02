use std::collections::HashMap;

use async_openai::{config::OpenAIConfig, Client};
use tracing::Span;

use crate::{
    events::SPAN_OPENAI,
    model::{
        embeddings::EmbeddingsModelInstance,
        error::ModelError,
        openai::openai_client,
        types::{LLMFinishEvent, ModelEvent, ModelEventType, ModelFinishReason},
        CredentialsIdent,
    },
    types::{
        credentials::ApiKeyCredentials,
        gateway::{CompletionModelUsage, CreateEmbeddingRequest, EncodingFormat, Input},
    },
    GatewayResult,
};

pub struct OpenAIEmbeddings {
    client: Client<OpenAIConfig>,
    credentials_ident: CredentialsIdent,
}

impl OpenAIEmbeddings {
    pub fn new(
        credentials: Option<&ApiKeyCredentials>,
        client: Option<Client<OpenAIConfig>>,
        endpoint: Option<&str>,
    ) -> Result<Self, ModelError> {
        Ok(OpenAIEmbeddings {
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Langdb),
            client: client.unwrap_or(openai_client(credentials, endpoint)?),
        })
    }
}

#[async_trait::async_trait]
impl EmbeddingsModelInstance for OpenAIEmbeddings {
    async fn embed(
        &self,
        request: &CreateEmbeddingRequest,
        outer_tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        _tags: HashMap<String, String>,
    ) -> GatewayResult<async_openai::types::CreateEmbeddingResponse> {
        let embedding_request = async_openai::types::CreateEmbeddingRequest {
            model: request.model.clone(),
            input: match &request.input {
                Input::String(s) => s.into(),
                Input::Array(vec) => vec.into(),
            },
            user: request.user.clone(),
            dimensions: request.dimensions.map(|d| d as u32),
            encoding_format: Some(match request.encoding_format {
                EncodingFormat::Float => async_openai::types::EncodingFormat::Float,
                EncodingFormat::Base64 => async_openai::types::EncodingFormat::Base64,
            }),
        };

        let response = self
            .client
            .embeddings()
            .create(embedding_request)
            .await
            .map_err(|e| ModelError::CustomError(e.to_string()))?;

        let span = Span::current();
        let _ = outer_tx
            .send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStop(LLMFinishEvent {
                    provider_name: SPAN_OPENAI.to_string(),
                    model_name: request.model.clone(),
                    output: None,
                    usage: Some(CompletionModelUsage {
                        input_tokens: response.usage.prompt_tokens,
                        output_tokens: 0,
                        total_tokens: response.usage.total_tokens,
                        ..Default::default()
                    }),
                    finish_reason: ModelFinishReason::Stop,
                    tool_calls: vec![],
                    credentials_ident: self.credentials_ident.clone(),
                }),
            )))
            .await;

        Ok(response)
    }
}
