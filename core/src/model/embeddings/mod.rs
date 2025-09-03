use std::{collections::HashMap, sync::Arc};

use async_openai::types::CreateEmbeddingResponse;
use serde::Serialize;
use serde_json::Value;
use tracing::info_span;
use tracing_futures::Instrument;
use valuable::Valuable;

use crate::{
    events::{JsonValue, RecordResult, SPAN_MODEL_CALL},
    model::{
        embeddings::{gemini::GeminiEmbeddings, openai::OpenAIEmbeddings},
        error::ModelError,
        types::{ModelEvent, ModelEventType},
        CredentialsIdent,
    },
    types::{
        engine::{EmbeddingsEngineParams, EmbeddingsModelDefinition},
        gateway::{CompletionModelUsage, CostCalculator, CreateEmbeddingRequest, Usage},
    },
    GatewayResult,
};

use tokio::sync::mpsc::channel;

pub mod gemini;
pub mod openai;

#[async_trait::async_trait]
pub trait EmbeddingsModelInstance: Sync + Send {
    async fn embed(
        &self,
        request: &CreateEmbeddingRequest,
        outer_tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<CreateEmbeddingResponse>;
}

pub fn initialize_embeddings_model_instance(
    definition: EmbeddingsModelDefinition,
    cost_calculator: Option<Arc<Box<dyn CostCalculator>>>,
    _endpoint: Option<&str>,
    _provider_name: Option<&str>,
) -> Result<Box<dyn EmbeddingsModelInstance>, ModelError> {
    match &definition.engine {
        EmbeddingsEngineParams::OpenAi {
            credentials,
            endpoint,
            ..
        } => Ok(Box::new(TracedEmbeddingsModel {
            inner: OpenAIEmbeddings::new(
                credentials.clone().as_ref(),
                None,
                endpoint.as_ref().map(|s| s.as_str()),
            )?,
            definition: definition.clone(),
            cost_calculator: cost_calculator.clone(),
        })),
        EmbeddingsEngineParams::Gemini { credentials, .. } => Ok(Box::new(TracedEmbeddingsModel {
            inner: GeminiEmbeddings::new(credentials.clone().as_ref())?,
            definition: definition.clone(),
            cost_calculator: cost_calculator.clone(),
        })),
    }
}

pub struct TracedEmbeddingsModel<Inner: EmbeddingsModelInstance> {
    inner: Inner,
    definition: EmbeddingsModelDefinition,
    cost_calculator: Option<Arc<Box<dyn CostCalculator>>>,
}

#[derive(Clone, Serialize)]
struct TracedEmbeddingsModelDefinition {
    pub name: String,
    pub provider_name: String,
    pub engine_name: String,
    pub prompt_name: Option<String>,
    pub model_params: EmbeddingsModelDefinition,
    pub model_name: String,
}

impl TracedEmbeddingsModelDefinition {
    pub fn sanitize_json(&self) -> GatewayResult<Value> {
        let mut model = self.clone();

        match &mut model.model_params.engine {
            EmbeddingsEngineParams::OpenAi {
                ref mut credentials,
                ..
            } => {
                credentials.take();
            }
            EmbeddingsEngineParams::Gemini {
                ref mut credentials,
                ..
            } => {
                credentials.take();
            }
        }
        let model = serde_json::to_value(&model)?;
        Ok(model)
    }

    pub fn get_credentials_owner(&self) -> CredentialsIdent {
        match &self.model_params.engine {
            EmbeddingsEngineParams::OpenAi { credentials, .. } => match &credentials {
                Some(_) => CredentialsIdent::Own,
                None => CredentialsIdent::Langdb,
            },
            EmbeddingsEngineParams::Gemini { credentials, .. } => match &credentials {
                Some(_) => CredentialsIdent::Own,
                None => CredentialsIdent::Langdb,
            },
        }
    }
}

impl From<EmbeddingsModelDefinition> for TracedEmbeddingsModelDefinition {
    fn from(value: EmbeddingsModelDefinition) -> Self {
        Self {
            model_name: value.db_model.inference_model_name.clone(),
            name: value.name.clone(),
            provider_name: value.db_model.provider_name.clone(),
            engine_name: value.engine.engine_name().to_string(),
            prompt_name: None,
            model_params: value.clone(),
        }
    }
}

#[async_trait::async_trait]
impl<Inner: EmbeddingsModelInstance> EmbeddingsModelInstance for TracedEmbeddingsModel<Inner> {
    async fn embed(
        &self,
        request: &CreateEmbeddingRequest,
        outer_tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<CreateEmbeddingResponse> {
        let traced_model: TracedEmbeddingsModelDefinition = self.definition.clone().into();
        let credentials_ident = traced_model.get_credentials_owner();
        let model = traced_model.sanitize_json()?;
        let model_str = serde_json::to_string(&model)?;
        let provider_name = self.definition.db_model.provider_name.clone();
        let request_str = serde_json::to_string(request)?;

        let (tx, mut rx) = channel::<Option<ModelEvent>>(outer_tx.max_capacity());
        let span = info_span!(
            target: "langdb::user_tracing::models", SPAN_MODEL_CALL,
            input = &request_str,
            model = model_str,
            provider_name = provider_name,
            output = tracing::field::Empty,
            error = tracing::field::Empty,
            credentials_identifier = credentials_ident.to_string(),
            cost = tracing::field::Empty,
            usage = tracing::field::Empty,
            tags = JsonValue(&serde_json::to_value(tags.clone())?).as_value(),
        );

        let cost_calculator = self.cost_calculator.clone();
        let price = self.definition.db_model.price.clone();
        tokio::spawn(
            async move {
                while let Some(Some(msg)) = rx.recv().await {
                    if let Some(cost_calculator) = cost_calculator.as_ref() {
                        if let ModelEventType::LlmStop(llm_finish_event) = &msg.event {
                            let s = tracing::Span::current();
                            let u = CompletionModelUsage {
                                input_tokens: llm_finish_event.usage.as_ref().unwrap().input_tokens,
                                output_tokens: llm_finish_event
                                    .usage
                                    .as_ref()
                                    .unwrap()
                                    .output_tokens,
                                total_tokens: llm_finish_event.usage.as_ref().unwrap().total_tokens,
                                prompt_tokens_details: None,
                                completion_tokens_details: None,
                                is_cache_used: false,
                            };
                            match cost_calculator
                                .calculate_cost(
                                    &price,
                                    &Usage::CompletionModelUsage(u.clone()),
                                    &credentials_ident,
                                )
                                .await
                            {
                                Ok(c) => {
                                    s.record("cost", serde_json::to_string(&c).unwrap());
                                }
                                Err(e) => {
                                    tracing::error!("Error calculating cost: {:?}", e);
                                }
                            };

                            s.record("usage", serde_json::to_string(&u).unwrap());
                        }
                    }

                    outer_tx.send(Some(msg)).await.unwrap();
                }
            }
            .instrument(span.clone()),
        );

        async {
            let result = self.inner.embed(request, tx, tags).await;
            let _ = result.as_ref().map(|r| r.data.len()).record();

            result
        }
        .instrument(span)
        .await
    }
}
