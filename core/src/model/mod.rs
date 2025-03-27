use crate::events::{JsonValue, RecordResult, SPAN_MODEL_CALL};
use crate::model::bedrock::BedrockModel;
use crate::model::error::ModelError;
use crate::types::engine::{CompletionEngineParams, CompletionModelParams};
use crate::types::engine::{CompletionModelDefinition, ModelTools, ModelType};
use crate::types::gateway::{
    ChatCompletionContent, ChatCompletionMessage, ContentType, CostCalculator, Usage,
};
use crate::types::threads::Message;
use crate::GatewayResult;
use anthropic::AnthropicModel;
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
use crate::model::proxy::OpenAISpecModel;
pub mod anthropic;
pub mod bedrock;
pub mod error;
pub mod gemini;
pub mod image_generation;
pub mod mcp;
pub mod mcp_server;
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
    ) -> GatewayResult<ChatCompletionMessage>;

    async fn stream(
        &self,
        input_vars: HashMap<String, Value>,
        tx: mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<()>;
}

pub const DEFAULT_MAX_RETRIES: i32 = 5;

pub struct TracedModel<Inner: ModelInstance> {
    inner: Inner,
    definition: CompletionModelDefinition,
    cost_calculator: Option<Arc<Box<dyn CostCalculator>>>,
    router_span: tracing::Span,
}

pub async fn init_completion_model_instance(
    definition: CompletionModelDefinition,
    tools: HashMap<String, Box<(dyn Tool + 'static)>>,
    cost_calculator: Option<Arc<Box<dyn CostCalculator>>>,
    endpoint: Option<&str>,
    provider_name: Option<&str>,
    router_span: tracing::Span,
) -> Result<Box<dyn ModelInstance>, ModelError> {
    match &definition.model_params.engine {
        CompletionEngineParams::Bedrock {
            params,
            execution_options,
            credentials,
            provider,
        } => Ok(Box::new(TracedModel {
            inner: BedrockModel::new(
                params.clone(),
                execution_options.clone(),
                credentials.as_ref(),
                definition.prompt.clone(),
                tools,
                provider.clone(),
            )
            .await?,
            definition,
            cost_calculator: cost_calculator.clone(),
            router_span: router_span.clone(),
        })),
        CompletionEngineParams::OpenAi {
            params,
            execution_options,
            credentials,
            endpoint,
        } => Ok(Box::new(TracedModel {
            inner: OpenAIModel::new(
                params.clone(),
                credentials.as_ref(),
                execution_options.clone(),
                definition.prompt.clone(),
                tools,
                None,
                endpoint.as_ref().map(|x| x.as_str()),
            )?,
            definition,
            cost_calculator: cost_calculator.clone(),
            router_span: router_span.clone(),
        })),
        CompletionEngineParams::Proxy {
            params,
            execution_options,
            credentials,
        } => {
            let provider_name = provider_name.expect("provider_name is expected  here");
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
                cost_calculator: cost_calculator.clone(),
                router_span: router_span.clone(),
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
            cost_calculator: cost_calculator.clone(),
            router_span: router_span.clone(),
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
            cost_calculator: cost_calculator.clone(),
            router_span: router_span.clone(),
        })),
    }
}

pub async fn initialize_completion(
    definition: CompletionModelDefinition,
    cost_calculator: Option<Arc<Box<dyn CostCalculator>>>,
    provider_name: Option<&str>,
    router_span: tracing::Span,
) -> Result<Box<dyn ModelInstance>, ModelError> {
    let tools: HashMap<_, Box<(dyn Tool + 'static)>> = HashMap::new();

    init_completion_model_instance(
        definition,
        tools,
        cost_calculator,
        None,
        provider_name,
        router_span,
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
    ) -> GatewayResult<ChatCompletionMessage> {
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
            inference_model_name = self.definition.db_model.name.to_string(),
            output = tracing::field::Empty,
            error = tracing::field::Empty,
            credentials_identifier = credentials_ident.to_string(),
            cost = tracing::field::Empty,
            usage = tracing::field::Empty,
            ttft = tracing::field::Empty,
            tags = JsonValue(&serde_json::to_value(tags.clone())?).as_value(),
        );

        let cost_calculator = self.cost_calculator.clone();
        tokio::spawn(
            async move {
                let mut start_time = None;
                while let Some(Some(msg)) = rx.recv().await {
                    if let Some(cost_calculator) = cost_calculator.as_ref() {
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
                                            &model_name,
                                            &provider_name,
                                            &Usage::CompletionModelUsage(u.clone()),
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
                .map(|r| match r.content.as_ref() {
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

            result
        }
        .instrument(span)
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
        let cost_calculator = self.cost_calculator.clone();

        let span = info_span!(
            target: "langdb::user_tracing::models",
            parent: self.router_span.clone(),
            SPAN_MODEL_CALL,
            input = &input_str,
            model = model_str,
            provider_name = provider_name,
            model_name = model_name.clone(),
            inference_model_name = self.definition.db_model.name.to_string(),
            output = tracing::field::Empty,
            error = tracing::field::Empty,
            credentials_identifier = credentials_ident.to_string(),
            cost = tracing::field::Empty,
            usage = tracing::field::Empty,
            tags = JsonValue(&serde_json::to_value(tags.clone())?).as_value(),
            ttft = tracing::field::Empty,
        );

        async {
            let (tx, mut rx) = channel(outer_tx.max_capacity());
            let mut output = String::new();
            let mut events = Vec::new();
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
                                if let Some(cost_calculator) = cost_calculator.as_ref() {
                                    if let Some(u) = &llmfinish_event.usage {
                                        let cost = cost_calculator
                                            .calculate_cost(
                                                &model_name,
                                                &provider_name,
                                                &Usage::CompletionModelUsage(u.clone()),
                                            )
                                            .await;

                                        match cost {
                                            Ok(c) => {
                                                s.record(
                                                    "cost",
                                                    serde_json::to_string(&c).unwrap(),
                                                );
                                            }
                                            Err(e) => {
                                                tracing::error!("Error calculating cost: {:?}", e);
                                            }
                                        }
                                        s.record("usage", serde_json::to_string(u).unwrap());
                                    }
                                }
                                events.push(msg.clone());
                            }
                            _ => {
                                events.push(msg.clone());
                            }
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
