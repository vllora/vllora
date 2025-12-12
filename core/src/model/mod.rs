use crate::executor::context::ExecutorContext;
use crate::handler::find_model_by_full_name;
use crate::model::cached::CachedModel;
use crate::types::guardrails::service::GuardrailsEvaluator;
use crate::types::guardrails::{GuardError, GuardResult, GuardStage};
use crate::types::metadata::services::model::ModelService;
use crate::GatewayApiError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;
use tokio::sync::mpsc::{self, channel};
use tracing::{info_span, Instrument};
use valuable::Valuable;
use vllora_llm::client::completions::response_stream::ResultStream;
use vllora_llm::client::completions::CompletionsClient;
use vllora_llm::client::error::ModelError;
use vllora_llm::client::ModelInstance;
use vllora_llm::error::{LLMError, LLMResult};
use vllora_llm::types::credentials_ident::CredentialsIdent;
use vllora_llm::types::engine::{CompletionEngineParams, CompletionModelParams};
use vllora_llm::types::engine::{CompletionEngineParamsBuilder, CompletionModelDefinition};
use vllora_llm::types::events::CustomEventType;
use vllora_llm::types::gateway::{
    ChatCompletionContent, ChatCompletionMessage, ChatCompletionMessageWithFinishReason,
    ChatCompletionRequest, ContentType, Extra, GatewayModelUsage, GuardOrName, GuardWithParameters,
    Usage,
};
use vllora_llm::types::instance::init_model_instance;
use vllora_llm::types::message::Message;
use vllora_llm::types::models::ModelMetadata;
use vllora_llm::types::models::ModelType;
use vllora_llm::types::provider::ModelPrice;
use vllora_llm::types::tools::ModelTools;
use vllora_llm::types::tools::Tool;
use vllora_llm::types::{CustomEvent, ModelEvent, ModelEventType};
use vllora_telemetry::events::{JsonValue, RecordResult, SPAN_MODEL_CALL};

pub mod azure;
pub mod bedrock;
pub mod cached;
pub mod embeddings;
pub mod google_vertex;
pub mod image_generation;
pub mod responses;
pub mod tools;

#[async_trait::async_trait]
pub trait ModelProviderInstance: Sync + Send {
    async fn get_private_models(&self) -> Result<Vec<ModelMetadata>, GatewayApiError>;
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ResponseCacheState {
    #[serde(rename = "HIT")]
    Hit,
    #[serde(rename = "MISS")]
    Miss,
}

impl Display for ResponseCacheState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponseCacheState::Hit => write!(f, "HIT"),
            ResponseCacheState::Miss => write!(f, "MISS"),
        }
    }
}

pub struct TracedModel {
    definition: CompletionModelDefinition,
    executor_context: ExecutorContext,
    router_span: tracing::Span,
    extra: Option<Extra>,
    initial_messages: Vec<ChatCompletionMessage>,
    response_cache_state: Option<ResponseCacheState>,
    request: ChatCompletionRequest,
    tools: HashMap<String, Arc<Box<dyn Tool + 'static>>>,
}

#[allow(clippy::too_many_arguments)]
pub async fn init_completion_model_instance(
    definition: CompletionModelDefinition,
    tools: HashMap<String, Arc<Box<dyn Tool + 'static>>>,
    executor_context: &ExecutorContext,
    router_span: tracing::Span,
    extra: Option<&Extra>,
    initial_messages: Vec<ChatCompletionMessage>,
    cached_model: Option<CachedModel>,
    cache_state: Option<ResponseCacheState>,
    request: ChatCompletionRequest,
) -> Result<Box<dyn ModelInstance>, ModelError> {
    if let Some(_cached_model) = cached_model {
        return Ok(Box::new(TracedModel {
            tools,
            definition,
            executor_context: executor_context.clone(),
            router_span: router_span.clone(),
            extra: extra.cloned(),
            initial_messages: initial_messages.clone(),
            response_cache_state: cache_state,
            request: request.clone(),
        }));
    }

    Ok(Box::new(TracedModel {
        tools,
        definition,
        executor_context: executor_context.clone(),
        router_span: router_span.clone(),
        extra: extra.cloned(),
        initial_messages: initial_messages.clone(),
        response_cache_state: cache_state,
        request: request.clone(),
    }))
}

#[derive(Clone, Serialize)]
struct TraceModelDefinition {
    pub name: String,
    pub provider_name: String,
    pub engine_name: String,
    pub model_params: CompletionModelParams,
    pub model_name: String,
    pub tools: ModelTools,
    pub model_type: ModelType,
}

impl TraceModelDefinition {
    pub fn sanitize_json(&self) -> LLMResult<Value> {
        let mut model = self.clone();

        match &mut model.model_params.engine {
            CompletionEngineParams::OpenAi {
                ref mut credentials,
                ..
            } => {
                credentials.take();
            }
            CompletionEngineParams::Bedrock {
                ref mut credentials,
                ..
            } => {
                credentials.take();
            }
            CompletionEngineParams::Anthropic {
                ref mut credentials,
                ..
            } => {
                credentials.take();
            }

            CompletionEngineParams::Gemini {
                ref mut credentials,
                ..
            } => {
                credentials.take();
            }
            CompletionEngineParams::Proxy {
                ref mut credentials,
                ..
            } => {
                credentials.take();
            }
        }
        let model = serde_json::to_value(&model)?;
        Ok(model)
    }
}
impl From<CompletionModelDefinition> for TraceModelDefinition {
    fn from(value: CompletionModelDefinition) -> Self {
        Self {
            model_name: value.model_name(),
            name: value.name,
            provider_name: value.model_params.provider_name.clone(),
            engine_name: value.model_params.engine.engine_name().to_string(),
            model_params: value.model_params,
            tools: value.tools,
            model_type: ModelType::Completions,
        }
    }
}

impl TracedModel {
    fn clean_input_trace(&self, input_vars: &HashMap<String, Value>) -> LLMResult<String> {
        let input_vars = input_vars.clone();
        let str = serde_json::to_string(&json!(input_vars))?;
        Ok(str)
    }
}

#[async_trait::async_trait]
impl ModelInstance for TracedModel {
    async fn invoke(
        &self,
        input_vars: HashMap<String, Value>,
        outer_tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        _previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> LLMResult<ChatCompletionMessageWithFinishReason> {
        let credentials_ident = credentials_identifier(&self.definition.model_params);
        let traced_model: TraceModelDefinition = self.definition.clone().into();
        let model = traced_model.sanitize_json()?;
        let model_str = serde_json::to_string(&model)?;
        // TODO: Fix input creation properly
        let input_str = self.clean_input_trace(&input_vars)?;
        let model_name = self.definition.name.clone();
        let provider_name = self.definition.db_model.provider_name.clone();
        let (tx, mut rx) = channel::<Option<ModelEvent>>(outer_tx.max_capacity());

        let span = info_span!(
            target: "vllora::user_tracing::models",
            parent: self.router_span.clone(),
            SPAN_MODEL_CALL,
            input = &input_str,
            model = model_str,
            provider_name = provider_name,
            model_name = model_name.clone(),
            inference_model_name = self.definition.db_model.inference_model_name.to_string(),
            output = tracing::field::Empty,
            error = tracing::field::Empty,
            credentials_identifier = credentials_ident.to_string(),
            cost = tracing::field::Empty,
            usage = tracing::field::Empty,
            ttft = tracing::field::Empty,
            tags = JsonValue(&serde_json::to_value(tags.clone())?).as_value(),
            cache = tracing::field::Empty
        );

        if let Some(state) = &self.response_cache_state {
            span.record("cache", state.to_string());
        }

        apply_guardrails(
            &self.initial_messages,
            self.extra.as_ref(),
            self.executor_context.evaluator_service.as_ref().as_ref(),
            &self.executor_context,
            GuardStage::Input,
        )
        .instrument(span.clone())
        .await
        .map_err(|e| LLMError::BoxedError(Box::new(e)))?;

        outer_tx
            .send(Some(ModelEvent::new(
                &span,
                ModelEventType::Custom(CustomEvent::new(CustomEventType::SpanStart {
                    operation_name: "model_call".to_string(),
                    attributes: serde_json::json!({}),
                })),
            )))
            .await?;

        let cost_calculator = self.executor_context.cost_calculator.clone();
        let price = self.definition.db_model.price.clone();
        tokio::spawn(
            async move {
                let mut start_time = None;
                let mut usage = GatewayModelUsage::default();
                let mut total_cost = 0.0;
                while let Some(Some(msg)) = rx.recv().await {
                    match &msg.event {
                        ModelEventType::LlmStart(_) => {
                            start_time = Some(msg.timestamp.timestamp_micros() as u64);
                        }
                        ModelEventType::LlmStop(llmfinish_event) => {
                            let current_span = tracing::Span::current();
                            if let Some(output) = &llmfinish_event.output {
                                current_span
                                    .record("output", serde_json::to_string(output).unwrap());
                            }
                            if let Some(u) = &llmfinish_event.usage {
                                match cost_calculator
                                    .calculate_cost(
                                        &price,
                                        &Usage::CompletionModelUsage(u.clone()),
                                        &credentials_ident,
                                    )
                                    .await
                                {
                                    Ok(mut c) => {
                                        total_cost += c.cost;
                                        c.cost = total_cost;
                                        current_span
                                            .record("cost", serde_json::to_string(&c).unwrap());
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Error calculating cost: {:?} {:#?}",
                                            e,
                                            llmfinish_event
                                        );
                                    }
                                };

                                usage.add_usage(u);
                                current_span.record("usage", serde_json::to_string(u).unwrap());
                            }
                        }
                        ModelEventType::LlmFirstToken(_) => {
                            if let Some(start_time) = start_time {
                                let current_span = tracing::Span::current();
                                current_span.record(
                                    "ttft",
                                    msg.timestamp.timestamp_micros() as u64 - start_time,
                                );
                            }
                        }
                        _ => (),
                    }

                    tracing::debug!(
                        "{} Received Model Event: {:?}",
                        msg.trace_id,
                        msg.event.as_str()
                    );
                    let _ = outer_tx.send(Some(msg)).await;
                }
            }
            .instrument(span.clone()),
        );

        let tools = self.tools.clone();
        async {
            let instance =
                init_model_instance(self.definition.model_params.engine.clone(), tools).await?;
            let vllora_llm_client = CompletionsClient::new(CompletionEngineParamsBuilder::new())
                .with_instance(instance);
            let result = vllora_llm_client
                .with_input_variables(input_vars.clone())
                .with_tx(tx.clone())
                .with_tags(tags.clone())
                .create(self.request.clone())
                .await;
            let _ = result
                .as_ref()
                .map(|r| match r.message().content.as_ref() {
                    Some(content) => match content {
                        ChatCompletionContent::Text(t) => t.to_string(),
                        ChatCompletionContent::Content(b) => b
                            .iter()
                            .map(|a| match a.r#type {
                                ContentType::Text => a.text.clone().unwrap_or_default(),
                                ContentType::ImageUrl => "".to_string(),
                                ContentType::InputAudio => "".to_string(),
                            })
                            .collect::<Vec<String>>()
                            .join("\n"),
                    },
                    _ => "".to_string(),
                })
                .record();

            if let Ok(message) = &result {
                apply_guardrails(
                    std::slice::from_ref(message.message()),
                    self.extra.as_ref(),
                    self.executor_context.evaluator_service.as_ref().as_ref(),
                    &self.executor_context,
                    GuardStage::Output,
                )
                .instrument(span.clone())
                .await
                .map_err(|e| LLMError::BoxedError(Box::new(e)))?;
            }

            result
        }
        .instrument(span.clone())
        .await
    }

    async fn stream(
        &self,
        input_vars: HashMap<String, Value>,
        outer_tx: mpsc::Sender<Option<ModelEvent>>,
        _previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> LLMResult<ResultStream> {
        let credentials_ident = credentials_identifier(&self.definition.model_params);
        let traced_model: TraceModelDefinition = self.definition.clone().into();
        let model = traced_model.sanitize_json()?;
        let model_str = serde_json::to_string(&model)?;
        // TODO: Fix input creation properly
        let input_str = self.clean_input_trace(&input_vars)?;

        let model_name = self.definition.name.clone();
        let provider_name = self.definition.db_model.provider_name.clone();
        let cost_calculator = self.executor_context.cost_calculator.clone();

        let span = info_span!(
            target: "vllora::user_tracing::models",
            parent: self.router_span.clone(),
            SPAN_MODEL_CALL,
            input = &input_str,
            model = model_str,
            provider_name = provider_name,
            model_name = model_name.clone(),
            inference_model_name = self.definition.db_model.inference_model_name.to_string(),
            output = tracing::field::Empty,
            error = tracing::field::Empty,
            credentials_identifier = credentials_ident.to_string(),
            cost = tracing::field::Empty,
            usage = tracing::field::Empty,
            tags = JsonValue(&serde_json::to_value(tags.clone())?).as_value(),
            ttft = tracing::field::Empty,
            cache = tracing::field::Empty
        );

        if let Some(state) = &self.response_cache_state {
            span.record("cache", state.to_string());
        }

        apply_guardrails(
            &self.initial_messages,
            self.extra.as_ref(),
            self.executor_context.evaluator_service.as_ref().as_ref(),
            &self.executor_context,
            GuardStage::Input,
        )
        .instrument(span.clone())
        .await
        .map_err(|e| LLMError::BoxedError(Box::new(e)))?;

        outer_tx
            .send(Some(ModelEvent::new(
                &span,
                ModelEventType::Custom(CustomEvent::new(CustomEventType::SpanStart {
                    operation_name: "model_call".to_string(),
                    attributes: serde_json::json!({}),
                })),
            )))
            .await?;

        let (tx, mut rx) = channel(outer_tx.max_capacity());
        let mut start_time = None;
        let tools = self.tools.clone();

        let instance =
            init_model_instance(self.definition.model_params.engine.clone(), tools).await?;
        let completions_client =
            CompletionsClient::new(CompletionEngineParamsBuilder::new()).with_instance(instance);

        let result = execute_stream(
            completions_client,
            self.request.clone(),
            input_vars.clone(),
            tx.clone(),
            tags.clone(),
        )
        .instrument(span.clone())
        .await;

        span.record(
            "tags",
            JsonValue(&serde_json::to_value(tags.clone())?).as_value(),
        );

        let price = self.definition.db_model.price.clone();
        tokio::spawn(
            async move {
                let mut output = String::new();
                while let Some(Some(msg)) = rx.recv().await {
                    match &msg.event {
                        ModelEventType::LlmStart(_event) => {
                            start_time = Some(msg.timestamp.timestamp_micros() as u64);
                        }
                        ModelEventType::LlmContent(event) => {
                            output.push_str(event.content.as_str());
                        }
                        ModelEventType::LlmFirstToken(_) => {
                            if let Some(start_time) = start_time {
                                let current_span = tracing::Span::current();
                                current_span.record(
                                    "ttft",
                                    msg.timestamp.timestamp_micros() as u64 - start_time,
                                );
                            }
                        }
                        ModelEventType::LlmStop(llmfinish_event) => {
                            let s = tracing::Span::current();
                            if let Some(u) = &llmfinish_event.usage {
                                let cost = cost_calculator
                                    .calculate_cost(
                                        &price,
                                        &Usage::CompletionModelUsage(u.clone()),
                                        &credentials_ident,
                                    )
                                    .await;

                                match cost {
                                    Ok(c) => {
                                        s.record("cost", serde_json::to_string(&c).unwrap());
                                    }
                                    Err(e) => {
                                        tracing::error!("Error calculating cost: {:?}", e);
                                    }
                                }
                                s.record("usage", serde_json::to_string(u).unwrap());
                            }
                            s.record("output", output.clone());
                        }
                        _ => {}
                    }
                    outer_tx.send(Some(msg)).await.unwrap();
                }
            }
            .instrument(span.clone()),
        );

        result
    }
}

async fn execute_stream(
    completions_client: CompletionsClient,
    request: ChatCompletionRequest,
    input_vars: HashMap<String, Value>,
    tx: mpsc::Sender<Option<ModelEvent>>,
    tags: HashMap<String, String>,
) -> LLMResult<ResultStream> {
    let client = completions_client
        .with_input_variables(input_vars.clone())
        .with_tx(tx.clone())
        .with_tags(tags.clone());
    client
        .create_stream(request.clone())
        .instrument(tracing::Span::current())
        .await
}

pub fn credentials_identifier(model_params: &CompletionModelParams) -> CredentialsIdent {
    let vllora_creds = match &model_params.engine {
        CompletionEngineParams::Bedrock { credentials, .. } => credentials.is_none(),
        CompletionEngineParams::OpenAi { credentials, .. } => credentials.is_none(),
        CompletionEngineParams::Anthropic { credentials, .. } => credentials.is_none(),
        CompletionEngineParams::Gemini { credentials, .. } => credentials.is_none(),
        CompletionEngineParams::Proxy { credentials, .. } => credentials.is_none(),
    };

    if vllora_creds {
        CredentialsIdent::Vllora
    } else {
        CredentialsIdent::Own
    }
}

pub async fn apply_guardrails(
    messages: &[ChatCompletionMessage],
    extra: Option<&Extra>,
    evaluator: &dyn GuardrailsEvaluator,
    executor_context: &ExecutorContext,
    guard_stage: GuardStage,
) -> Result<(), GuardError> {
    let Some(Extra { guards, .. }) = extra else {
        return Ok(());
    };

    for guard in guards {
        let (guard_id, parameters) = match guard {
            GuardOrName::GuardId(guard_id) => (guard_id, None),
            GuardOrName::GuardWithParameters(GuardWithParameters { id, parameters }) => {
                (id, Some(parameters))
            }
        };

        let result = evaluator
            .evaluate(
                messages,
                guard_id,
                executor_context,
                parameters,
                &guard_stage,
            )
            .await
            .map_err(GuardError::GuardEvaluationError)?;

        match result {
            GuardResult::Json { passed, .. }
            | GuardResult::Boolean { passed, .. }
            | GuardResult::Text { passed, .. }
                if !passed =>
            {
                return Err(GuardError::GuardNotPassed(guard_id.clone(), result));
            }
            _ => {}
        }
    }

    Ok(())
}

#[async_trait::async_trait]
pub trait ModelMetadataFactory: Send + Sync {
    async fn get_model_metadata(
        &self,
        model_name: &str,
        include_parameters: bool,
        include_benchmark: bool,
        project_id: Option<&uuid::Uuid>,
    ) -> Result<ModelMetadata, GatewayApiError>;

    async fn get_cheapest_model_metadata(
        &self,
        model_names: &[String],
    ) -> Result<ModelMetadata, GatewayApiError>;

    async fn get_models_by_name(
        &self,
        model_name: &str,
        project_id: Option<&uuid::Uuid>,
    ) -> Result<Vec<ModelMetadata>, GatewayApiError>;

    async fn get_top_by_ranking(
        &self,
        ranking_name: &str,
        top: u8,
    ) -> Result<Vec<ModelMetadata>, GatewayApiError>;
}

pub struct DefaultModelMetadataFactory {
    service: Arc<Box<dyn ModelService>>,
}

impl DefaultModelMetadataFactory {
    pub fn new(service: Arc<Box<dyn ModelService>>) -> Self {
        Self { service }
    }
}

#[async_trait::async_trait]
impl ModelMetadataFactory for DefaultModelMetadataFactory {
    #[tracing::instrument(skip_all)]

    async fn get_model_metadata(
        &self,
        model_name: &str,
        _include_parameters: bool,
        _include_benchmark: bool,
        _project_id: Option<&uuid::Uuid>,
    ) -> Result<ModelMetadata, GatewayApiError> {
        find_model_by_full_name(model_name, self.service.as_ref().as_ref())
    }

    async fn get_cheapest_model_metadata(
        &self,
        _model_names: &[String],
    ) -> Result<ModelMetadata, GatewayApiError> {
        unimplemented!()
    }

    async fn get_models_by_name(
        &self,
        _model_name: &str,
        _project_id: Option<&uuid::Uuid>,
    ) -> Result<Vec<ModelMetadata>, GatewayApiError> {
        unimplemented!()
    }

    async fn get_top_by_ranking(
        &self,
        _ranking_name: &str,
        _top: u8,
    ) -> Result<Vec<ModelMetadata>, GatewayApiError> {
        unimplemented!()
    }
}

pub fn get_cheapest_model_metadata(
    models: &[ModelMetadata],
) -> Result<ModelMetadata, GatewayApiError> {
    let cheapest_model = models
        .iter()
        .min_by(|a, b| {
            let price_a = match &a.price {
                ModelPrice::Completion(price) => price.per_input_token,
                _ => f64::INFINITY,
            };
            let price_b = match &b.price {
                ModelPrice::Completion(price) => price.per_input_token,
                _ => f64::INFINITY,
            };
            price_a
                .partial_cmp(&price_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or(GatewayApiError::CustomError("No model found".to_string()))?;

    Ok(cheapest_model.clone())
}
