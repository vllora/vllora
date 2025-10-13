use crate::executor::context::ExecutorContext;
use crate::metadata::services::model::ModelService;
use crate::model::bedrock::BedrockModel;
use crate::model::cached::CachedModel;
use crate::model::error::ModelError;
use crate::telemetry::events::{JsonValue, RecordResult, SPAN_MODEL_CALL};
use crate::types::engine::{CompletionEngineParams, CompletionModelParams};
use crate::types::engine::{CompletionModelDefinition, ModelTools, ModelType};
use crate::types::gateway::{
    ChatCompletionContent, ChatCompletionMessage, ChatCompletionMessageWithFinishReason,
    ContentType, Extra, GuardOrName, GuardWithParameters, Usage,
};
use crate::types::guardrails::service::GuardrailsEvaluator;
use crate::types::guardrails::{GuardError, GuardResult, GuardStage};
use crate::types::provider::ModelPrice;
use crate::types::threads::Message;
use crate::GatewayResult;
use anthropic::AnthropicModel;
use async_openai::config::OpenAIConfig;
use async_openai::Client;
use async_trait::async_trait;
use futures::future::join;
use gemini::GeminiModel;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;
use tokio::sync::mpsc::{self, channel};
use tools::Tool;
use tracing::{info_span, Instrument};
use types::{ModelEvent, ModelEventType};
use valuable::Valuable;
pub mod handler;
use self::openai::OpenAIModel;
use crate::handler::find_model_by_full_name;
use crate::model::proxy::OpenAISpecModel;
use crate::models::ModelMetadata;
use crate::GatewayApiError;

pub mod anthropic;
pub mod azure;
pub mod bedrock;
pub mod cached;
pub mod embeddings;
pub mod error;
pub mod gemini;
pub mod google_vertex;
pub mod image_generation;
pub mod mcp;
pub mod mcp_server;
pub mod model_restrictions;
pub mod openai;
pub mod openai_spec_client;
pub mod proxy;
pub mod tools;
pub mod types;

#[async_trait]
pub trait ModelInstance: Sync + Send {
    async fn invoke(
        &self,
        input_vars: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessageWithFinishReason>;

    async fn stream(
        &self,
        input_vars: HashMap<String, Value>,
        tx: mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<()>;
}

#[async_trait]
pub trait ModelProviderInstance: Sync + Send {
    async fn get_private_models(&self) -> Result<Vec<ModelMetadata>, GatewayApiError>;
}

pub const DEFAULT_MAX_RETRIES: u32 = 0;

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

pub struct TracedModel<Inner: ModelInstance> {
    inner: Inner,
    definition: CompletionModelDefinition,
    executor_context: ExecutorContext,
    router_span: tracing::Span,
    extra: Option<Extra>,
    initial_messages: Vec<ChatCompletionMessage>,
    response_cache_state: Option<ResponseCacheState>,
}

#[allow(clippy::too_many_arguments)]
pub async fn init_completion_model_instance(
    definition: CompletionModelDefinition,
    tools: HashMap<String, Box<dyn Tool + 'static>>,
    executor_context: &ExecutorContext,
    endpoint: Option<&str>,
    provider_name: Option<&str>,
    router_span: tracing::Span,
    extra: Option<&Extra>,
    initial_messages: Vec<ChatCompletionMessage>,
    cached_model: Option<CachedModel>,
    cache_state: Option<ResponseCacheState>,
) -> Result<Box<dyn ModelInstance>, ModelError> {
    if let Some(cached_model) = cached_model {
        return Ok(Box::new(TracedModel {
            inner: cached_model,
            definition,
            executor_context: executor_context.clone(),
            router_span: router_span.clone(),
            extra: extra.cloned(),
            initial_messages: initial_messages.clone(),
            response_cache_state: cache_state,
        }));
    }

    match &definition.model_params.engine {
        CompletionEngineParams::Bedrock {
            params,
            execution_options,
            credentials,
            ..
        } => Ok(Box::new(TracedModel {
            inner: BedrockModel::new(
                params.clone(),
                execution_options.clone(),
                credentials.as_ref(),
                definition.prompt.clone(),
                tools,
            )
            .await?,
            definition,
            executor_context: executor_context.clone(),
            router_span: router_span.clone(),
            extra: extra.cloned(),
            initial_messages: initial_messages.clone(),
            response_cache_state: cache_state,
        })),
        CompletionEngineParams::OpenAi {
            params,
            execution_options,
            credentials,
            endpoint,
        } => {
            // Check if the endpoint is an Azure OpenAI endpoint
            if let Some(ep) = endpoint.as_ref() {
                if ep.contains("azure.com") {
                    // Use the Azure implementation
                    return Ok(Box::new(TracedModel {
                        inner: OpenAIModel::from_azure_url(
                            params.clone(),
                            credentials.as_ref(),
                            execution_options.clone(),
                            definition.prompt.clone(),
                            tools,
                            ep,
                        )?,
                        definition,
                        executor_context: executor_context.clone(),
                        router_span: router_span.clone(),
                        extra: extra.cloned(),
                        initial_messages: initial_messages.clone(),
                        response_cache_state: cache_state,
                    }));
                }
            }

            // Default OpenAI implementation
            Ok(Box::new(TracedModel {
                inner: OpenAIModel::new(
                    params.clone(),
                    credentials.as_ref(),
                    execution_options.clone(),
                    definition.prompt.clone(),
                    tools,
                    None::<Client<OpenAIConfig>>,
                    endpoint.as_ref().map(|x| x.as_str()),
                )?,
                definition,
                executor_context: executor_context.clone(),
                router_span: router_span.clone(),
                extra: extra.cloned(),
                initial_messages: initial_messages.clone(),
                response_cache_state: cache_state,
            }))
        }
        CompletionEngineParams::Proxy {
            params,
            execution_options,
            credentials,
        } => {
            let provider_name = provider_name.expect("provider_name is expected here");
            Ok(Box::new(TracedModel {
                inner: OpenAISpecModel::new(
                    params.clone(),
                    credentials.as_ref(),
                    execution_options.clone(),
                    definition.prompt.clone(),
                    tools,
                    endpoint,
                    provider_name,
                )?,
                definition,
                executor_context: executor_context.clone(),
                router_span: router_span.clone(),
                extra: extra.cloned(),
                initial_messages: initial_messages.clone(),
                response_cache_state: cache_state,
            }))
        }
        CompletionEngineParams::Anthropic {
            credentials,
            execution_options,
            params,
        } => Ok(Box::new(TracedModel {
            inner: AnthropicModel::new(
                params.clone(),
                execution_options.clone(),
                credentials.as_ref(),
                definition.prompt.clone(),
                tools,
            )?,
            definition,
            executor_context: executor_context.clone(),
            router_span: router_span.clone(),
            extra: extra.cloned(),
            initial_messages: initial_messages.clone(),
            response_cache_state: cache_state,
        })),
        CompletionEngineParams::Gemini {
            credentials,
            execution_options,
            params,
        } => Ok(Box::new(TracedModel {
            inner: GeminiModel::new(
                params.clone(),
                execution_options.clone(),
                credentials.as_ref(),
                definition.prompt.clone(),
                tools,
            )?,
            definition,
            executor_context: executor_context.clone(),
            router_span: router_span.clone(),
            extra: extra.cloned(),
            initial_messages: initial_messages.clone(),
            response_cache_state: cache_state,
        })),
    }
}

pub async fn initialize_completion(
    definition: CompletionModelDefinition,
    executor_context: &ExecutorContext,
    provider_name: Option<&str>,
    router_span: tracing::Span,
    extra: Option<&Extra>,
    initial_messages: Vec<ChatCompletionMessage>,
) -> Result<Box<dyn ModelInstance>, ModelError> {
    let tools: HashMap<_, Box<dyn Tool + 'static>> = HashMap::new();

    init_completion_model_instance(
        definition,
        tools,
        executor_context,
        None,
        provider_name,
        router_span,
        extra,
        initial_messages,
        None,
        None,
    )
    .await
}

#[derive(Clone, Serialize)]
struct TraceModelDefinition {
    pub name: String,
    pub provider_name: String,
    pub engine_name: String,
    pub prompt_name: Option<String>,
    pub model_params: CompletionModelParams,
    pub model_name: String,
    pub tools: ModelTools,
    pub model_type: ModelType,
}

impl TraceModelDefinition {
    pub fn sanitize_json(&self) -> GatewayResult<Value> {
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
            prompt_name: value.model_params.prompt_name.clone(),
            model_params: value.model_params,
            tools: value.tools,
            model_type: ModelType::Completions,
        }
    }
}

impl<Inner: ModelInstance> TracedModel<Inner> {
    fn clean_input_trace(&self, input_vars: &HashMap<String, Value>) -> GatewayResult<String> {
        let input_vars = input_vars.clone();
        let str = serde_json::to_string(&json!(input_vars))?;
        Ok(str)
    }
}

#[async_trait]
impl<Inner: ModelInstance> ModelInstance for TracedModel<Inner> {
    async fn invoke(
        &self,
        input_vars: HashMap<String, Value>,
        outer_tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessageWithFinishReason> {
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
            target: "langdb::user_tracing::models",
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
        .await?;

        let cost_calculator = self.executor_context.cost_calculator.clone();
        let price = self.definition.db_model.price.clone();
        tokio::spawn(
            async move {
                let mut start_time = None;
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
                                    Ok(c) => {
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

        async {
            let result = self
                .inner
                .invoke(input_vars, tx, previous_messages, tags)
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
                .await?;
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
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<()> {
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
            target: "langdb::user_tracing::models",
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
        .await?;

        async {
            let (tx, mut rx) = channel(outer_tx.max_capacity());
            let mut output = String::new();
            let mut start_time = None;
            let result = join(
                self.inner
                    .stream(input_vars, tx, previous_messages, tags.clone()),
                async {
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
                                s.record("output", serde_json::to_string(&output).unwrap());
                                if let Some(u) = &llmfinish_event.usage {
                                    let cost = cost_calculator
                                        .calculate_cost(
                                            &self.definition.db_model.price,
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
                            }
                            _ => {}
                        }
                        outer_tx.send(Some(msg)).await.unwrap();
                    }
                },
            )
            .instrument(span.clone())
            .await
            .0;
            let span = tracing::Span::current();
            span.record(
                "tags",
                JsonValue(&serde_json::to_value(tags.clone())?).as_value(),
            );
            match result {
                Ok(()) => span.record("output", output),
                Err(ref e) => span.record("error", tracing::field::display(e)),
            };
            result
        }
        .await
    }
}

pub fn credentials_identifier(model_params: &CompletionModelParams) -> CredentialsIdent {
    let langdb_creds = match &model_params.engine {
        CompletionEngineParams::Bedrock { credentials, .. } => credentials.is_none(),
        CompletionEngineParams::OpenAi { credentials, .. } => credentials.is_none(),
        CompletionEngineParams::Anthropic { credentials, .. } => credentials.is_none(),
        CompletionEngineParams::Gemini { credentials, .. } => credentials.is_none(),
        CompletionEngineParams::Proxy { credentials, .. } => credentials.is_none(),
    };

    if langdb_creds {
        CredentialsIdent::Langdb
    } else {
        CredentialsIdent::Own
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum CredentialsIdent {
    Langdb,
    Own,
}

impl Display for CredentialsIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialsIdent::Langdb => write!(f, "langdb"),
            CredentialsIdent::Own => write!(f, "own"),
        }
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

#[async_trait]
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
    restrictions_manager: Option<Arc<dyn model_restrictions::ProjectModelRestrictionsTrait>>,
}

impl DefaultModelMetadataFactory {
    pub fn new(service: Arc<Box<dyn ModelService>>) -> Self {
        Self { 
            service,
            restrictions_manager: None,
        }
    }

    pub fn with_restrictions(
        service: Arc<Box<dyn ModelService>>,
        restrictions_manager: Arc<dyn model_restrictions::ProjectModelRestrictionsTrait>,
    ) -> Self {
        Self { 
            service,
            restrictions_manager: Some(restrictions_manager),
        }
    }
}

#[async_trait]
impl ModelMetadataFactory for DefaultModelMetadataFactory {
    #[tracing::instrument(skip_all)]

    async fn get_model_metadata(
        &self,
        model_name: &str,
        _include_parameters: bool,
        _include_benchmark: bool,
        _project_id: Option<&uuid::Uuid>,
    ) -> Result<ModelMetadata, GatewayApiError> {
        // Find the model using ModelService
        let model = find_model_by_full_name(model_name, self.service.as_ref().as_ref())?;
        
        // If restrictions are configured, validate the model is allowed
        if let Some(restrictions_manager) = &self.restrictions_manager {
            let restrictions = restrictions_manager.fetch().await?;
            
            tracing::info!(
                "Checking restrictions for model: {}, found {} restrictions",
                model_name,
                restrictions.len()
            );
            
            // No tags for now (OSS version doesn't use tags initially)
            // Just apply all project-level restrictions
            let mut filtered_models = vec![model.clone()];
            
            // Apply each restriction
            for restriction in restrictions {
                // Apply allowed_models filter if present
                let allowed = restriction.allowed_models();
                if !allowed.is_empty() {
                    tracing::info!(
                        "Applying allow-list with {} models. Model qualified name: {}",
                        allowed.len(),
                        model.qualified_model_name()
                    );
                    filtered_models.retain(|m| {
                        allowed.contains(&m.model) || allowed.contains(&m.qualified_model_name())
                    });
                }
                
                // Apply disallowed_models filter if present
                let disallowed = restriction.disallowed_models();
                if !disallowed.is_empty() {
                    filtered_models.retain(|m| {
                        !disallowed.contains(&m.model) && !disallowed.contains(&m.qualified_model_name())
                    });
                }
            }
            
            // If filtered out, model is not allowed
            if filtered_models.is_empty() {
                tracing::warn!(
                    "Model {} was filtered out by restrictions",
                    model_name
                );
                return Err(GatewayApiError::GatewayError(crate::GatewayError::ModelError(
                    Box::new(ModelError::ModelNotFound(model_name.to_string())),
                )));
            }
        }
        
        Ok(model)
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
