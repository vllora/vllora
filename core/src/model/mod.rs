use crate::error::GatewayError;
use crate::events::{JsonValue, RecordResult, SPAN_MODEL_CALL};
use crate::model::bedrock::BedrockModel;
use crate::model::error::ToolError;
use crate::types::engine::{CompletionEngineParams, CompletionModelParams};
use crate::types::engine::{CompletionModelDefinition, InputArgs, ModelTools, ModelType};
use crate::types::gateway::{
    ChatCompletionContent, ChatCompletionMessage, ContentType, CostCalculator, Usage,
};
use crate::types::threads;
use crate::types::threads::{InnerMessage, Message, MessageContentPart};
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
use crate::model::langdb_open::OpenAISpecModel;
pub mod anthropic;
pub mod bedrock;
pub mod error;
pub mod executor;
pub mod gemini;
pub mod image_generation;
pub mod langdb_open;
pub mod mcp;
pub mod openai;
pub mod openai_spec_client;
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
}

pub async fn init_completion_model_instance(
    definition: CompletionModelDefinition,
    tools: HashMap<String, Box<(dyn Tool + 'static)>>,
    cost_calculator: Option<Arc<Box<dyn CostCalculator>>>,
    endpoint: Option<&str>,
    provider_name: Option<&str>,
) -> Result<Box<dyn ModelInstance>, ToolError> {
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
            .await
            .map_err(|_| ToolError::CredentialsError("Bedrock".into()))?,
            definition,
            cost_calculator: cost_calculator.clone(),
        })),
        CompletionEngineParams::OpenAi {
            params,
            execution_options,
            credentials,
            output_schema,
        } => Ok(Box::new(TracedModel {
            inner: OpenAIModel::new(
                params.clone(),
                credentials.as_ref(),
                execution_options.clone(),
                definition.prompt.clone(),
                tools,
                output_schema.clone(),
                None,
            )
            .map_err(|_| ToolError::CredentialsError("Openai".into()))?,
            definition,
            cost_calculator: cost_calculator.clone(),
        })),
        CompletionEngineParams::LangdbOpen {
            params,
            execution_options,
            credentials,
            output_schema,
        } => {
            let provider_name = provider_name.expect("provider_name is expected  here");
            Ok(Box::new(TracedModel {
                inner: OpenAISpecModel::new(
                    params.clone(),
                    credentials.as_ref(),
                    execution_options.clone(),
                    definition.prompt.clone(),
                    tools,
                    output_schema.clone(),
                    endpoint,
                    provider_name,
                )
                .map_err(|_| ToolError::CredentialsError(provider_name.into()))?,
                definition,
                cost_calculator: cost_calculator.clone(),
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
            )
            .map_err(|_| ToolError::CredentialsError("Anthropic".into()))?,
            definition,
            cost_calculator: cost_calculator.clone(),
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
            )
            .map_err(|_| ToolError::CredentialsError("Gemini".into()))?,
            definition,
            cost_calculator: cost_calculator.clone(),
        })),
    }
}

pub async fn initialize_completion(
    definition: CompletionModelDefinition,
    cost_calculator: Option<Arc<Box<dyn CostCalculator>>>,
    provider_name: Option<&str>,
) -> Result<Box<dyn ModelInstance>, ToolError> {
    let tools: HashMap<_, Box<(dyn Tool + 'static)>> = HashMap::new();

    init_completion_model_instance(definition, tools, cost_calculator, None, provider_name).await
}

#[derive(Clone, Serialize)]
struct TraceModelDefinition {
    pub name: String,
    pub input_args: InputArgs,
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
            CompletionEngineParams::LangdbOpen {
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
            input_args: value.input_args,
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
        let mut input_vars = input_vars.clone();
        let first_arg = self.definition.input_args.0.first();
        if let Some(first_arg) = first_arg {
            input_vars.entry(first_arg.name.clone()).and_modify(|m| {
                if let Ok(InnerMessage::Array(arr)) =
                    serde_json::from_value::<InnerMessage>(m.clone())
                {
                    let arr: Vec<MessageContentPart> = arr
                        .iter()
                        .map(|a| {
                            let mut a = a.clone();
                            match a.r#type {
                                threads::MessageContentType::Text => {}
                                threads::MessageContentType::ImageUrl => {
                                    // Dont return image urls in tracing
                                    a.value =
                                        a.value.split(',').nth(0).unwrap_or_default().to_string();
                                }
                                threads::MessageContentType::InputAudio => {}
                            }
                            a
                        })
                        .collect();
                    let v = serde_json::to_value(arr).unwrap();
                    *m = v;
                }
            });
        }
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
        // let json_value_tags = JsonValue(&serde_json::to_value(tags_1.clone())?).as_value();
        let span = info_span!(
            target: "langdb::user_tracing::models",
            SPAN_MODEL_CALL,
            input = &input_str,
            model = model_str,
            provider_name = provider_name,
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
                while let Some(Some(msg)) = rx.recv().await {
                    if let Some(cost_calculator) = cost_calculator.as_ref() {
                        match &msg.event {
                            ModelEventType::LlmStop(llmfinish_event) => {
                                if let Some(u) = &llmfinish_event.usage {
                                    let s = tracing::Span::current();
                                    match cost_calculator
                                        .calculate_cost(
                                            &model_name,
                                            &provider_name,
                                            &Usage::CompletionModelUsage(u.clone()),
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

                                    s.record("usage", serde_json::to_string(u).unwrap());
                                }
                            }
                            ModelEventType::LlmFirstToken(llmfirst_token) => {
                                let s = tracing::Span::current();
                                s.record("ttft", llmfirst_token.ttft);
                            }
                            _ => (),
                        }
                    }

                    tracing::debug!(
                        "{} Received Model Event: {:?}",
                        msg.trace_id,
                        msg.event.as_str()
                    );
                    outer_tx.send(Some(msg)).await.unwrap();
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
            target: "langdb::user_tracing::models", SPAN_MODEL_CALL,
            input = &input_str,
            model = model_str,
            provider_name = provider_name,
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
            let result = join(
                self.inner
                    .stream(input_vars, tx, previous_messages, tags.clone()),
                async {
                    while let Some(Some(msg)) = rx.recv().await {
                        match &msg.event {
                            ModelEventType::LlmContent(event) => {
                                output.push_str(event.content.as_str());
                            }
                            ModelEventType::LlmFirstToken(event) => {
                                let current_span = tracing::Span::current();
                                current_span.record("ttft", event.ttft);
                            }
                            ModelEventType::LlmStop(llmfinish_event) => {
                                if let Some(cost_calculator) = cost_calculator.as_ref() {
                                    if let Some(u) = &llmfinish_event.usage {
                                        let cost = cost_calculator
                                            .calculate_cost(
                                                &model_name,
                                                &provider_name,
                                                &Usage::CompletionModelUsage(u.clone()),
                                            )
                                            .await;

                                        let s = tracing::Span::current();
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

pub fn validate_variables(
    input_variables: &HashMap<String, Value>,
    required_variables: &[String],
    model_args: &InputArgs,
) -> GatewayResult<()> {
    for var in required_variables {
        if !model_args.contains(var) && !input_variables.contains_key(var) {
            return Err(GatewayError::MissingVariable(var.clone()));
        }
    }
    Ok(())
}

pub fn credentials_identifier(model_params: &CompletionModelParams) -> CredentialsIdent {
    let langdb_creds = match &model_params.engine {
        CompletionEngineParams::Bedrock { credentials, .. } => credentials.is_none(),
        CompletionEngineParams::OpenAi { credentials, .. } => credentials.is_none(),
        CompletionEngineParams::Anthropic { credentials, .. } => credentials.is_none(),
        CompletionEngineParams::Gemini { credentials, .. } => credentials.is_none(),
        CompletionEngineParams::LangdbOpen { credentials, .. } => credentials.is_none(),
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
