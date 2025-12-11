use std::collections::HashMap;

use async_openai::{
    config::{AzureConfig, Config, OpenAIConfig},
    types::EmbeddingUsage,
    Client,
};
use tracing::field;
use tracing::Span;
use tracing_futures::Instrument;
use valuable::Valuable;

use crate::{model::embeddings::EmbeddingsModelInstance, types::embed::EmbeddingResult};
use vllora_llm::client::error::AuthorizationError;
use vllora_llm::client::error::ModelError;
use vllora_llm::error::LLMError;
use vllora_llm::error::LLMResult;
use vllora_llm::provider::openai::{azure_openai_client, openai_client};
use vllora_llm::types::credentials::ApiKeyCredentials;
use vllora_llm::types::credentials_ident::CredentialsIdent;
use vllora_llm::types::gateway::{
    CreateEmbeddingRequest, EncodingFormat, GatewayModelUsage, Input,
};
use vllora_llm::types::LLMFinishEvent;
use vllora_llm::types::LLMStartEvent;
use vllora_llm::types::ModelEvent;
use vllora_llm::types::ModelEventType;
use vllora_llm::types::ModelFinishReason;
use vllora_telemetry::create_model_span;
use vllora_telemetry::events::JsonValue;
use vllora_telemetry::events::SPAN_OPENAI;

macro_rules! target {
    () => {
        "vllora::user_tracing::models::openai"
    };
    ($subtgt:literal) => {
        concat!("vllora::user_tracing::models::openai::", $subtgt)
    };
}

pub struct OpenAIEmbeddings<C: Config = OpenAIConfig> {
    client: Client<C>,
    credentials_ident: CredentialsIdent,
}

impl OpenAIEmbeddings<OpenAIConfig> {
    pub fn new(
        credentials: Option<&ApiKeyCredentials>,
        client: Option<Client<OpenAIConfig>>,
        endpoint: Option<&str>,
    ) -> Result<Self, ModelError> {
        Ok(OpenAIEmbeddings {
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Vllora),
            client: client.unwrap_or(openai_client(credentials, endpoint)?),
        })
    }
}

impl OpenAIEmbeddings<AzureConfig> {
    pub fn new_azure(
        credentials: Option<&ApiKeyCredentials>,
        endpoint: &str,
        deployment_id: &str,
    ) -> Result<Self, ModelError> {
        let api_key = if let Some(credentials) = credentials {
            credentials.api_key.clone()
        } else {
            std::env::var("VLLORA_OPENAI_API_KEY").map_err(|_| AuthorizationError::InvalidApiKey)?
        };
        Ok(OpenAIEmbeddings {
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Vllora),
            client: azure_openai_client(api_key, endpoint, deployment_id),
        })
    }
}

impl<C: Config> OpenAIEmbeddings<C> {
    async fn execute(
        &self,
        embedding_request: async_openai::types::CreateEmbeddingRequest,
        encoding_format: &EncodingFormat,
        model_name: &str,
        outer_tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
    ) -> LLMResult<EmbeddingResult> {
        let span = Span::current();
        let _ = outer_tx.try_send(Some(ModelEvent::new(
            &span,
            ModelEventType::LlmStart(LLMStartEvent {
                provider_name: "openai".to_string(),
                model_name: model_name.to_string(),
                input: serde_json::to_string(&embedding_request)?,
            }),
        )));

        let response: EmbeddingResult = match encoding_format {
            EncodingFormat::Float => self
                .client
                .embeddings()
                .create(embedding_request)
                .await
                .map(|r| r.into())
                .map_err(|e| ModelError::OpenAIApi(Box::new(e)))?,
            EncodingFormat::Base64 => self
                .client
                .embeddings()
                .create_base64(embedding_request)
                .await
                .map(|r| r.into())
                .map_err(|e| ModelError::OpenAIApi(Box::new(e)))?,
        };

        outer_tx
            .try_send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStop(LLMFinishEvent {
                    provider_name: SPAN_OPENAI.to_string(),
                    model_name: model_name.to_string(),
                    output: None,
                    usage: Some(GatewayModelUsage {
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
            .map_err(|e| LLMError::CustomError(e.to_string()))?;

        span.record(
            "raw_usage",
            JsonValue(&serde_json::to_value(response.usage()).unwrap()).as_value(),
        );
        span.record(
            "usage",
            JsonValue(&serde_json::to_value(Self::map_usage(response.usage())).unwrap()).as_value(),
        );

        Ok(response)
    }

    fn map_usage(usage: &EmbeddingUsage) -> GatewayModelUsage {
        GatewayModelUsage {
            input_tokens: usage.prompt_tokens,
            total_tokens: usage.total_tokens,
            ..Default::default()
        }
    }
}

#[async_trait::async_trait]
impl<C: Config + std::marker::Sync + std::marker::Send> EmbeddingsModelInstance
    for OpenAIEmbeddings<C>
{
    async fn embed(
        &self,
        request: &CreateEmbeddingRequest,
        outer_tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> LLMResult<EmbeddingResult> {
        let embedding_request = async_openai::types::CreateEmbeddingRequest {
            model: request.model.clone(),
            input: match &request.input {
                Input::String(s) => s.into(),
                Input::Array(vec) => vec.into(),
            },
            user: request.user.clone(),
            dimensions: request.dimensions.map(|d| d as u32),
            encoding_format: Some(match &request.encoding_format {
                EncodingFormat::Float => async_openai::types::EncodingFormat::Float,
                EncodingFormat::Base64 => async_openai::types::EncodingFormat::Base64,
            }),
        };

        let span = create_model_span!(
            SPAN_OPENAI,
            target!("embedding"),
            tags,
            0,
            input = serde_json::to_string(&embedding_request)?
        );

        self.execute(
            embedding_request,
            &request.encoding_format,
            &request.model,
            &outer_tx,
        )
        .instrument(span.clone())
        .await
    }
}
