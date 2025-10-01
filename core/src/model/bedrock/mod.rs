use super::types::{
    LLMContentEvent, LLMFinishEvent, LLMStartEvent, ModelEvent, ModelEventType, ModelFinishReason,
    ModelToolCall,
};
use super::{CredentialsIdent, ModelInstance};
use crate::error::GatewayError;
use crate::model::error::{BedrockError, ModelFinishError};
use crate::model::handler::handle_tool_call;
use crate::model::types::LLMFirstToken;
use crate::model::DEFAULT_MAX_RETRIES;
use crate::model::{ModelProviderInstance, Tool as LangdbTool};
use crate::models::{
    InferenceProvider, Limits, ModelCapability, ModelIOFormats, ModelMetadata, ModelType,
};
use crate::telemetry::events::{self, JsonValue, RecordResult, SPAN_BEDROCK};
use crate::types::aws::{get_shared_config, get_user_shared_config};
use crate::types::credentials::BedrockCredentials;
use crate::types::engine::{BedrockModelParams, ExecutionOptions, Prompt};
use crate::types::gateway::{
    ChatCompletionContent, ChatCompletionMessage, ChatCompletionMessageWithFinishReason,
    CompletionModelUsage, ToolCall,
};
use crate::types::message::{MessageType, PromptMessage};
use crate::types::provider::{CompletionModelPrice, InferenceModelProvider, ModelPrice};
use crate::types::threads::InnerMessage;
use crate::types::threads::Message as LMessage;
use crate::{create_model_span, GatewayApiError, GatewayResult};
use async_trait::async_trait;
use aws_config::{BehaviorVersion, SdkConfig};
use aws_sdk_bedrock::config::SharedTokenProvider;
use aws_sdk_bedrock::Client as BedrockClient;
use aws_sdk_bedrockruntime::operation::converse::builders::ConverseFluentBuilder;
use aws_sdk_bedrockruntime::operation::converse_stream::builders::ConverseStreamFluentBuilder;
use aws_sdk_bedrockruntime::operation::converse_stream::{self, ConverseStreamError};
use aws_sdk_bedrockruntime::types::builders::ImageBlockBuilder;
use aws_sdk_bedrockruntime::types::ConverseOutput::Message as MessageVariant;
use aws_sdk_bedrockruntime::types::{
    ContentBlock, ContentBlockDelta, ContentBlockStart, ConversationRole, ConverseOutput,
    ConverseStreamOutput, InferenceConfiguration, Message, ReasoningContentBlock, StopReason,
    SystemContentBlock, TokenUsage, Tool, ToolConfiguration, ToolInputSchema, ToolResultBlock,
    ToolResultContentBlock, ToolResultStatus, ToolSpecification, ToolUseBlock,
};
use aws_sdk_bedrockruntime::Client;
use aws_smithy_types::{Blob, Document};
use base64::Engine;
use serde::de::IntoDeserializer;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::log::info;
use tracing::{field, Instrument, Span};
use valuable::Valuable;

mod pricing;

use super::error::ModelError;

const DEFAULT_REGION: &str = "us-east-1";

macro_rules! target {
    () => {
        "langdb::user_tracing::models::bedrock"
    };
    ($subtgt:literal) => {
        concat!("langdb::user_tracing::models::bedrock::", $subtgt)
    };
}

enum InnerExecutionResult {
    Finish(ChatCompletionMessageWithFinishReason),
    NextCall(Vec<Message>),
}

fn build_err(e: impl ToString) -> ModelError {
    ModelError::CustomError(e.to_string())
}

pub struct BedrockModel {
    pub client: Client,
    pub execution_options: ExecutionOptions,
    prompt: Prompt,
    params: BedrockModelParams,
    pub tools: Arc<HashMap<String, Box<dyn LangdbTool>>>,
    pub model_name: String,
    pub credentials_ident: CredentialsIdent,
}

#[derive(Debug, Clone, Serialize)]
pub struct BedrockToolCall {
    pub tool_use_id: String,
    pub name: String,
    pub properties: Value,
}

pub(crate) async fn get_sdk_config(
    credentials: Option<&BedrockCredentials>,
) -> Result<SdkConfig, ModelError> {
    Ok(match credentials {
        Some(BedrockCredentials::IAM(creds)) => {
            get_user_shared_config(creds.clone()).await.load().await
        }
        Some(BedrockCredentials::ApiKey(creds)) => {
            let token = aws_credential_types::Token::new(creds.api_key.clone(), None);
            SdkConfig::builder()
                .token_provider(SharedTokenProvider::new(token))
                .behavior_version(BehaviorVersion::latest())
                .region(aws_config::Region::new(
                    creds.region.clone().unwrap_or(DEFAULT_REGION.to_string()),
                ))
                .build()
        }
        None => {
            get_shared_config(Some(aws_config::Region::new(DEFAULT_REGION.to_string())))
                .await
                .load()
                .await
        }
    })
}

pub async fn bedrock_client(
    credentials: Option<&BedrockCredentials>,
) -> Result<Client, ModelError> {
    let config = get_sdk_config(credentials).await?;
    Ok(Client::new(&config))
}

impl BedrockModel {
    pub async fn new(
        model_params: BedrockModelParams,
        execution_options: ExecutionOptions,
        credentials: Option<&BedrockCredentials>,
        prompt: Prompt,
        tools: HashMap<String, Box<dyn LangdbTool>>,
    ) -> Result<Self, ModelError> {
        let client = bedrock_client(credentials).await?;

        let model_id = model_params.model_id.clone().unwrap_or_default();

        Ok(Self {
            client,
            execution_options,
            prompt,
            params: model_params,
            tools: Arc::new(tools),
            model_name: model_id,
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Langdb),
        })
    }

    pub(crate) fn construct_messages(
        &self,
        input_vars: HashMap<String, Value>,
        previous_messages: Vec<LMessage>,
    ) -> GatewayResult<(Vec<Message>, Vec<SystemContentBlock>)> {
        let mut conversational_messages: Vec<Message> = vec![];
        let mut system_messages = self
            .prompt
            .messages
            .iter()
            .filter(|m| m.r#type == MessageType::SystemMessage)
            .map(|message| Self::map_system_message(message.to_owned(), &input_vars))
            .collect::<Vec<_>>();

        for m in previous_messages.iter() {
            if m.r#type == MessageType::SystemMessage {
                if let Some(content) = m.content.clone() {
                    system_messages.push(SystemContentBlock::Text(content));
                }
            }
        }
        let previous_messages = Self::map_previous_messages(previous_messages)?;

        conversational_messages.extend(previous_messages);
        let human_message = self
            .prompt
            .messages
            .iter()
            .find(|m| m.r#type == MessageType::HumanMessage)
            .map(|message| Self::map_chat_messages(message.to_owned(), input_vars.to_owned()));

        if let Some(human_message) = human_message {
            conversational_messages.push(human_message?);
        }

        Ok((conversational_messages, system_messages))
    }

    fn map_previous_messages(messages_dto: Vec<LMessage>) -> Result<Vec<Message>, ModelError> {
        // convert serde::Map into HashMap
        let mut messages: Vec<Message> = vec![];
        let mut tool_results_expected = 0;
        let mut tool_calls_results = vec![];
        for m in messages_dto.iter() {
            let message = match m.r#type {
                MessageType::AIMessage => {
                    let mut contents = vec![];
                    if let Some(content) = m.content.clone() {
                        if !content.is_empty() {
                            contents.push(ContentBlock::Text(content));
                        }
                    }
                    if let Some(tool_calls) = m.tool_calls.clone() {
                        tool_results_expected = tool_calls.len();
                        tool_calls_results = vec![];

                        for tool_call in tool_calls {
                            let doc =
                                serde_json::from_str::<Document>(&tool_call.function.arguments)?;
                            contents.push(ContentBlock::ToolUse(
                                ToolUseBlock::builder()
                                    .tool_use_id(tool_call.id.clone())
                                    .name(tool_call.function.name.clone())
                                    .input(doc)
                                    .build()
                                    .map_err(build_err)?,
                            ));
                        }
                    }

                    Message::builder()
                        .set_content(Some(contents))
                        .role(ConversationRole::Assistant)
                        .build()
                        .map_err(build_err)?
                }
                MessageType::HumanMessage => construct_human_message(&m.clone().into())?,
                MessageType::ToolResult => {
                    tool_results_expected -= 1;
                    let content = m.content.clone().unwrap_or_default();
                    tool_calls_results.push(ContentBlock::ToolResult(
                        ToolResultBlock::builder()
                            .tool_use_id(m.tool_call_id.clone().unwrap_or_default())
                            .content(ToolResultContentBlock::Text(content))
                            .status(ToolResultStatus::Success)
                            .build()
                            .map_err(build_err)?,
                    ));

                    if tool_results_expected > 0 {
                        continue;
                    }

                    Message::builder()
                        .set_content(Some(tool_calls_results.clone()))
                        .role(ConversationRole::User)
                        .build()
                        .map_err(build_err)?
                }
                _ => {
                    continue;
                }
            };
            messages.push(message);
        }
        Ok(messages)
    }
    pub(crate) fn map_chat_messages(
        prompt: PromptMessage,
        variables: HashMap<String, Value>,
    ) -> Result<Message, ModelError> {
        let message = match prompt.r#type {
            MessageType::SystemMessage | MessageType::AIMessage => {
                let raw_message = Prompt::render(prompt.msg.clone(), &variables);
                Message::builder()
                    .content(ContentBlock::Text(raw_message))
                    .role(ConversationRole::Assistant)
                    .build()
                    .map_err(|e| ModelError::CustomError(format!("Error building messages: {e}")))?
            }

            MessageType::HumanMessage => {
                let msg = prompt.msg;
                let inner_message: InnerMessage = if prompt.wired {
                    let value = variables
                        .get(&msg)
                        .ok_or(ModelError::CustomError(format!("{msg} not specified")))?;
                    serde_json::from_value(value.clone())
                        .map_err(|e| ModelError::CustomError(e.to_string()))?
                } else {
                    InnerMessage::Text(Prompt::render(msg.clone(), &variables))
                };

                construct_human_message(&inner_message)?
            }

            MessageType::ToolResult => {
                todo!()
            }
        };

        Ok(message)
    }

    pub(crate) fn map_system_message(
        message: PromptMessage,
        variables: &HashMap<String, Value>,
    ) -> SystemContentBlock {
        let raw_message = Prompt::render(message.msg.clone(), variables);

        SystemContentBlock::Text(raw_message)
    }

    pub fn map_tool_call(tool_call: &ToolUseBlock) -> GatewayResult<ModelToolCall> {
        Ok(ModelToolCall {
            tool_id: tool_call.tool_use_id.clone(),
            tool_name: tool_call.name.clone(),
            input: serde_json::to_string(&tool_call.input)?,
        })
    }
    async fn handle_tool_calls(
        tool_uses: Vec<ToolUseBlock>,
        tools: &HashMap<String, Box<dyn LangdbTool>>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<Message> {
        let content = futures::future::join_all(tool_uses.iter().map(|tool| {
            let tags_value = tags.clone();
            async move {
                let tool_use_id = tool.tool_use_id.clone();
                tracing::trace!("Calling tool ({tool_use_id}) {:?}", tool.name);
                let tool_call = Self::map_tool_call(tool)?;
                let result = handle_tool_call(&tool_call, tools, tx, tags_value.clone()).await;
                tracing::trace!("Result ({tool_use_id}): {result:?}");
                let content = result.unwrap_or_else(|err| err.to_string());
                Ok(ContentBlock::ToolResult(
                    ToolResultBlock::builder()
                        .tool_use_id(tool_use_id.clone())
                        .content(ToolResultContentBlock::Text(content))
                        .status(ToolResultStatus::Success)
                        .build()
                        .unwrap(),
                ))
            }
        }))
        .await;

        let c = content
            .into_iter()
            .collect::<GatewayResult<Vec<ContentBlock>>>()?;
        Ok(Message::builder()
            .set_content(Some(c))
            .role(ConversationRole::User)
            .build()
            .unwrap())
    }

    pub(crate) fn get_tools_config(&self) -> Result<Option<ToolConfiguration>, GatewayError> {
        if self.tools.is_empty() {
            return Ok(None);
        }

        let mut tools = vec![];

        for (name, tool) in self.tools.iter() {
            let schema = tool
                .get_function_parameters()
                .map(|params| serde_json::from_value(serde_json::to_value(params)?))
                .transpose()?
                .map(ToolInputSchema::Json);
            let t = Tool::ToolSpec(
                ToolSpecification::builder()
                    .name(name)
                    // .set_description(tool.description.clone())
                    .set_input_schema(schema)
                    .build()
                    .map_err(build_err)?,
            );

            tools.push(t);
        }

        info!("TOOLS {:?}", tools);

        let config = ToolConfiguration::builder()
            .set_tools(Some(tools))
            .build()
            .map_err(build_err)?;

        Ok(Some(config))
    }

    pub fn build_request(
        &self,
        input_messages: &[Message],
        system_messages: &[SystemContentBlock],
    ) -> GatewayResult<ConverseFluentBuilder> {
        let model_params = &self.params;
        let inference_config = InferenceConfiguration::builder()
            .set_max_tokens(model_params.max_tokens)
            .set_temperature(model_params.temperature)
            .set_top_p(model_params.top_p)
            .set_stop_sequences(model_params.stop_sequences.clone())
            .build();

        tracing::warn!("Bedrock Model name: {}", self.model_name);

        Ok(self
            .client
            .converse()
            .set_system(Some(system_messages.to_vec()))
            .set_tool_config(self.get_tools_config()?)
            .model_id(replace_version(&self.model_name))
            .set_messages(Some(input_messages.to_vec()))
            .additional_model_request_fields(Document::deserialize(
                model_params
                    .additional_parameters
                    .clone()
                    .into_deserializer(),
            )?)
            .set_inference_config(Some(inference_config)))
    }

    async fn execute(
        &self,
        input_messages: Vec<Message>,
        system_messages: Vec<SystemContentBlock>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessageWithFinishReason> {
        let mut calls = vec![input_messages];

        let mut retries_left = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some(input_messages) = calls.pop() {
            let input = serde_json::json!({
                "initial_messages": format!("{input_messages:?}"),
                "system_messages": format!("{system_messages:?}")
            });
            let span = create_model_span!(
                SPAN_BEDROCK,
                target!("chat"),
                tags,
                retries_left,
                input = JsonValue(&input).as_value(),
                system_prompt = field::Empty
            );

            let builder = self.build_request(&input_messages, &system_messages)?;
            let response = self
                .execute_inner(builder, span.clone(), tx, tags.clone())
                .await;

            match response {
                Ok(InnerExecutionResult::Finish(message)) => return Ok(message),
                Ok(InnerExecutionResult::NextCall(messages)) => {
                    calls.push(messages);
                }
                Err(e) => {
                    span.record("error", e.to_string());
                    if retries_left == 0 {
                        return Err(e);
                    } else {
                        calls.push(input_messages);
                    }
                    retries_left -= 1;
                }
            }
        }
        unreachable!();
    }

    async fn execute_inner(
        &self,
        builder: ConverseFluentBuilder,
        span: Span,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<InnerExecutionResult> {
        let input_messages = builder.get_messages().clone().unwrap_or_default();
        tx.send(Some(ModelEvent::new(
            &span,
            ModelEventType::LlmStart(LLMStartEvent {
                provider_name: SPAN_BEDROCK.to_string(),
                model_name: self.model_name.clone(),
                input: format!("{input_messages:?}"),
            }),
        )))
        .await
        .map_err(|e| GatewayError::CustomError(e.to_string()))?;

        let response = async move {
            let result = builder.send().await;
            let _ = result
                .as_ref()
                .map(|response| Value::String(format!("{response:?}")))
                .as_ref()
                .map(JsonValue)
                .record();
            let response = result.map_err(|e| ModelError::Bedrock(Box::new(e.into())))?;
            let span = Span::current();

            span.record("output", format!("{response:?}"));
            if let Some(ref usage) = response.usage {
                span.record(
                    "usage",
                    JsonValue(&serde_json::json!({
                        "input_tokens": usage.input_tokens,
                        "output_tokens": usage.output_tokens,
                        "total_tokens": usage.total_tokens,
                    }))
                    .as_value(),
                );
            }
            Ok::<_, GatewayError>(response)
        }
        .instrument(span.clone().or_current())
        .await?;

        match response.stop_reason {
            StopReason::EndTurn | StopReason::StopSequence => match response.output {
                Some(MessageVariant(message)) => {
                    let usage = response.usage.as_ref().map(|usage| CompletionModelUsage {
                        input_tokens: usage.input_tokens as u32,
                        output_tokens: usage.output_tokens as u32,
                        total_tokens: usage.total_tokens as u32,
                        ..Default::default()
                    });

                    let output = match message.content.first() {
                        Some(ContentBlock::Text(message)) => Some(message.clone()),
                        _ => None,
                    };

                    tx.send(Some(ModelEvent::new(
                        &span,
                        ModelEventType::LlmStop(LLMFinishEvent {
                            provider_name: SPAN_BEDROCK.to_string(),
                            model_name: self
                                .params
                                .model_id
                                .clone()
                                .map(|m| m.to_string())
                                .unwrap_or_default(),
                            output,
                            usage,
                            finish_reason: ModelFinishReason::Stop,
                            tool_calls: vec![],
                            credentials_ident: self.credentials_ident.clone(),
                        }),
                    )))
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))?;

                    let message = message.content.first().ok_or(ModelError::CustomError(
                        "Content Block Not Found".to_string(),
                    ))?;
                    match message {
                        ContentBlock::Text(content) => Ok(InnerExecutionResult::Finish(
                            ChatCompletionMessageWithFinishReason::new(
                                ChatCompletionMessage {
                                    role: "assistant".to_string(),
                                    content: Some(ChatCompletionContent::Text(content.clone())),
                                    ..Default::default()
                                },
                                ModelFinishReason::Stop,
                            ),
                        )),
                        ContentBlock::ReasoningContent(ReasoningContentBlock::ReasoningText(
                            content,
                        )) => Ok(InnerExecutionResult::Finish(
                            ChatCompletionMessageWithFinishReason::new(
                                ChatCompletionMessage {
                                    role: "assistant".to_string(),
                                    content: Some(ChatCompletionContent::Text(
                                        content.text().to_string(),
                                    )),
                                    ..Default::default()
                                },
                                ModelFinishReason::Stop,
                            ),
                        )),
                        _ => Err(ModelError::FinishError(
                            ModelFinishError::ContentBlockNotInTextFormat,
                        )
                        .into()),
                    }
                }
                _ => Err(ModelError::FinishError(ModelFinishError::NoOutputProvided).into()),
            },

            StopReason::ToolUse => {
                let tools_span =
                    tracing::info_span!(target: target!(), events::SPAN_TOOLS, label=field::Empty);
                tools_span.follows_from(span.id());
                if let Some(message_output) = response.output {
                    match message_output {
                        ConverseOutput::Message(message) => {
                            let mut messages = vec![message.clone()];
                            let mut text = String::new();
                            let mut tool_uses = vec![];

                            for m in message.content {
                                match m {
                                    ContentBlock::Text(t) => text.push_str(&t),
                                    ContentBlock::ToolUse(tool_use) => {
                                        tool_uses.push(tool_use);
                                    }
                                    _ => {}
                                }
                            }

                            let content = if text.is_empty() { None } else { Some(text) };

                            let tool = self.tools.get(&tool_uses[0].name).ok_or(
                                ModelError::FinishError(ModelFinishError::ToolNotFound(
                                    tool_uses[0].name.clone(),
                                )),
                            )?;
                            let tool_calls: Vec<ToolCall> = tool_uses
                                .iter()
                                .enumerate()
                                .map(|(index, tool_call)| ToolCall {
                                    index: Some(index),
                                    id: tool_call.tool_use_id().to_string(),
                                    r#type: "function".to_string(),
                                    function: crate::types::gateway::FunctionCall {
                                        name: tool_call.name().to_string(),
                                        arguments: serde_json::to_string(tool_call.input())
                                            .unwrap_or_default(),
                                    },
                                })
                                .collect();
                            let tool_calls_str = serde_json::to_string(&tool_calls)?;
                            let tools_span = tracing::info_span!(target: target!(), events::SPAN_TOOLS, tool_calls=tool_calls_str, label=tool_uses.iter().map(|t| t.name.clone()).collect::<Vec<String>>().join(","));

                            tools_span.record(
                                "label",
                                tool_uses
                                    .iter()
                                    .map(|t| t.name.clone())
                                    .collect::<Vec<String>>()
                                    .join(","),
                            );
                            if tool.stop_at_call() {
                                let usage =
                                    response.usage.as_ref().map(|usage| CompletionModelUsage {
                                        input_tokens: usage.input_tokens as u32,
                                        output_tokens: usage.output_tokens as u32,
                                        total_tokens: usage.total_tokens as u32,
                                        ..Default::default()
                                    });

                                tx.send(Some(ModelEvent::new(
                                    &span,
                                    ModelEventType::LlmStop(LLMFinishEvent {
                                        provider_name: SPAN_BEDROCK.to_string(),
                                        model_name: self
                                            .params
                                            .model_id
                                            .clone()
                                            .map(|m| m.to_string())
                                            .unwrap_or_default(),
                                        output: content.clone(),
                                        usage,
                                        finish_reason: ModelFinishReason::ToolCalls,
                                        tool_calls: tool_uses
                                            .iter()
                                            .map(Self::map_tool_call)
                                            .collect::<Result<Vec<ModelToolCall>, GatewayError>>(
                                        )?,
                                        credentials_ident: self.credentials_ident.clone(),
                                    }),
                                )))
                                .await
                                .map_err(|e| GatewayError::CustomError(e.to_string()))?;

                                Ok(InnerExecutionResult::Finish(
                                    ChatCompletionMessageWithFinishReason::new(
                                        ChatCompletionMessage {
                                            role: "assistant".to_string(),
                                            tool_calls: Some(tool_calls),
                                            content: content.map(ChatCompletionContent::Text),
                                            ..Default::default()
                                        },
                                        ModelFinishReason::ToolCalls,
                                    ),
                                ))
                            } else {
                                let tools_message = Self::handle_tool_calls(
                                    tool_uses,
                                    &self.tools,
                                    tx,
                                    tags.clone(),
                                )
                                .instrument(tools_span.clone())
                                .await?;
                                messages.push(tools_message);

                                let conversation_messages = [input_messages, messages].concat();

                                Ok(InnerExecutionResult::NextCall(conversation_messages))
                            }
                        }
                        _ => Err(ModelError::FinishError(
                            ModelFinishError::ToolUseDoesntHaveMessage,
                        )
                        .into()),
                    }
                } else {
                    Err(ModelError::FinishError(ModelFinishError::ToolMissingContent).into())
                }
            }
            x => Err(Self::handle_stop_reason(x).into()),
        }
    }

    async fn process_stream(
        &self,
        stream: converse_stream::ConverseStreamOutput,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        started_at: std::time::Instant,
    ) -> GatewayResult<(
        StopReason,
        Option<(ConversationRole, Vec<ToolUseBlock>)>,
        Option<TokenUsage>,
        ConverseOutput,
    )> {
        let mut stream = stream.stream;
        let mut role = None;
        let mut tool_uses: HashMap<i32, ToolUseBlock> = HashMap::new();
        let mut usage: Option<TokenUsage> = None;
        let mut accumulated_text = String::new();
        let mut first_response_received = false;
        while let Some(result) = stream.recv().await.transpose() {
            let output = result.map_err(|e| ModelError::Bedrock(Box::new(e.into())))?;
            if !first_response_received {
                first_response_received = true;
                tx.send(Some(ModelEvent::new(
                    &Span::current(),
                    ModelEventType::LlmFirstToken(LLMFirstToken {}),
                )))
                .await
                .map_err(|e| GatewayError::CustomError(e.to_string()))?;
                Span::current().record("ttft", started_at.elapsed().as_micros());
            }
            match output {
                ConverseStreamOutput::ContentBlockDelta(a) => {
                    match a.delta {
                        Some(ContentBlockDelta::Text(t)) => {
                            // Save streamed text content
                            accumulated_text.push_str(&t);
                            tx.send(Some(ModelEvent::new(
                                &Span::current(),
                                ModelEventType::LlmContent(LLMContentEvent { content: t }),
                            )))
                            .await
                            .unwrap();
                        }
                        Some(ContentBlockDelta::ToolUse(tool_use)) => {
                            tool_uses.entry(a.content_block_index).and_modify(|t| {
                                let Document::String(ref mut s) = t.input else {
                                    unreachable!("Streaming tool input is always a string")
                                };
                                s.push_str(tool_use.input());
                            });
                        }
                        _ => {
                            return Err(ModelError::CustomError(
                                "Tooluse block not found in response".to_string(),
                            )
                            .into());
                        }
                    };
                }
                ConverseStreamOutput::ContentBlockStart(a) => match a.start {
                    Some(ContentBlockStart::ToolUse(tool_use)) => {
                        let tool_use = ToolUseBlock::builder()
                            .name(tool_use.name)
                            .tool_use_id(tool_use.tool_use_id)
                            .input(String::new().into())
                            .build()
                            .map_err(build_err)?;
                        tool_uses.insert(a.content_block_index, tool_use);
                    }
                    _ => {
                        return Err(ModelError::CustomError(
                            "Tooluse block not found in response".to_string(),
                        )
                        .into())
                    }
                },
                ConverseStreamOutput::ContentBlockStop(event) => {
                    if let Some(block) = tool_uses.get_mut(&event.content_block_index) {
                        let Document::String(ref s) = block.input else {
                            unreachable!()
                        };
                        let d: Document = serde_json::from_str(s)?;
                        block.input = d;
                    }
                }
                ConverseStreamOutput::MessageStart(event) => {
                    role = Some(event.role);
                }
                ConverseStreamOutput::MessageStop(event) => {
                    if let Ok(Some(ConverseStreamOutput::Metadata(m))) = stream.recv().await {
                        usage = m.usage;
                    }
                    // Build a ConverseOutput::Message assembled from accumulated content and tool uses
                    let mut content_blocks: Vec<ContentBlock> = Vec::new();
                    if !accumulated_text.is_empty() {
                        content_blocks.push(ContentBlock::Text(accumulated_text.clone()));
                    }
                    let tool_use_blocks: Vec<ToolUseBlock> =
                        tool_uses.clone().into_values().collect();
                    for t in tool_use_blocks.iter().cloned() {
                        content_blocks.push(ContentBlock::ToolUse(t));
                    }
                    let message = Message::builder()
                        .role(role.clone().unwrap_or(ConversationRole::Assistant))
                        .set_content(Some(content_blocks))
                        .build()
                        .map_err(build_err)?;
                    let response = ConverseOutput::Message(message);
                    return Ok((
                        event.stop_reason,
                        role.map(|role| (role, tool_uses.into_values().collect())),
                        usage,
                        response,
                    ));
                }
                ConverseStreamOutput::Metadata(m) => {
                    if let Some(u) = m.usage {
                        usage = Some(u);
                    }
                }
                x => {
                    return Err(
                        ModelError::CustomError(format!("Unhandled Stream output: {x:?}")).into(),
                    )
                }
            }
        }
        unreachable!();
    }

    fn map_finish_reason(reason: &StopReason) -> ModelFinishReason {
        match reason {
            StopReason::EndTurn | StopReason::StopSequence => ModelFinishReason::Stop,
            StopReason::ToolUse => ModelFinishReason::ToolCalls,
            StopReason::ContentFiltered => ModelFinishReason::ContentFilter,
            StopReason::GuardrailIntervened => ModelFinishReason::Guardrail,
            StopReason::MaxTokens => ModelFinishReason::Length,
            x => ModelFinishReason::Other(format!("{x:?}")),
        }
    }
    fn map_usage(usage: Option<&TokenUsage>) -> Option<CompletionModelUsage> {
        usage.map(|u| CompletionModelUsage {
            input_tokens: u.input_tokens as u32,
            output_tokens: u.output_tokens as u32,
            total_tokens: u.total_tokens as u32,
            ..Default::default()
        })
    }

    async fn execute_stream(
        &self,
        input_messages: Vec<Message>,
        system_messages: Vec<SystemContentBlock>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<()> {
        let mut calls = vec![input_messages];

        let mut retries_left = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some(input_messages) = calls.pop() {
            let input = serde_json::json!({
                "initial_messages": format!("{input_messages:?}"),
                "system_messages": format!("{system_messages:?}")
            });
            let span = create_model_span!(
                SPAN_BEDROCK,
                target!("chat"),
                tags,
                retries_left,
                input = JsonValue(&input).as_value(),
                system_prompt = field::Empty
            );

            tracing::warn!("Bedrock Model name: {}", self.model_name);

            let builder = self
                .client
                .converse_stream()
                .model_id(replace_version(&self.model_name))
                .set_system(Some(system_messages.clone()))
                .set_tool_config(self.get_tools_config()?)
                .set_messages(Some(input_messages.clone()));

            let response = self
                .execute_stream_inner(builder, span.clone(), tx, tags.clone())
                .await;

            match response {
                Ok(InnerExecutionResult::Finish(_)) => return Ok(()),
                Ok(InnerExecutionResult::NextCall(messages)) => {
                    calls.push(messages);
                }
                Err(e) => {
                    span.record("error", e.to_string());
                    if retries_left == 0 {
                        return Err(e);
                    } else {
                        calls.push(input_messages);
                    }
                    retries_left -= 1;
                }
            }
        }

        Ok(())
    }

    async fn execute_stream_inner(
        &self,
        builder: ConverseStreamFluentBuilder,
        span: Span,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<InnerExecutionResult> {
        let input_messages = builder.get_messages().clone().unwrap_or_default();

        tx.send(Some(ModelEvent::new(
            &span,
            ModelEventType::LlmStart(LLMStartEvent {
                provider_name: SPAN_BEDROCK.to_string(),
                model_name: self.params.model_id.clone().unwrap_or_default(),
                input: format!("{input_messages:?}"),
            }),
        )))
        .await
        .map_err(|e| GatewayError::CustomError(e.to_string()))?;

        let started_at = std::time::Instant::now();
        let response = builder.send().await.map_err(map_converse_stream_error)?;
        let (stop_reason, msg, usage, response_message) = self
            .process_stream(response, tx, started_at)
            .instrument(span.clone())
            .await?;

        span.record("output", format!("{response_message:?}"));
        let trace_finish_reason = Self::map_finish_reason(&stop_reason);
        let usage = Self::map_usage(usage.as_ref());
        if let Some(usage) = &usage {
            span.record(
                "usage",
                JsonValue(&serde_json::json!({
                    "input_tokens": usage.input_tokens,
                    "output_tokens": usage.output_tokens,
                }))
                .as_value(),
            );
        }
        let tool_calls = msg
            .as_ref()
            .map(|(_, tool_uses)| {
                tool_uses
                    .iter()
                    .map(Self::map_tool_call)
                    .collect::<GatewayResult<Vec<_>>>()
            })
            .unwrap_or(Ok(vec![]))?;
        tx.send(Some(ModelEvent::new(
            &span,
            ModelEventType::LlmStop(LLMFinishEvent {
                provider_name: SPAN_BEDROCK.to_string(),
                model_name: self.params.model_id.clone().unwrap_or_default(),
                output: None,
                usage,
                finish_reason: trace_finish_reason.clone(),
                tool_calls: tool_calls.clone(),
                credentials_ident: self.credentials_ident.clone(),
            }),
        )))
        .await
        .map_err(|e| GatewayError::CustomError(e.to_string()))?;

        match stop_reason {
            StopReason::ToolUse => {
                let Some((role, tool_uses)) = msg else {
                    return Err(ModelError::CustomError("Empty tooluse block".to_string()).into());
                };

                let tool_calls_str = serde_json::to_string(&tool_calls)?;
                let tools_span = tracing::info_span!(target: target!(), events::SPAN_TOOLS, tool_calls=tool_calls_str, label=tool_uses.iter().map(|t| t.name.clone()).collect::<Vec<String>>().join(","));

                let tool = self.tools.get(&tool_calls[0].tool_name).unwrap();
                if tool.stop_at_call() {
                    return Ok(InnerExecutionResult::Finish(
                        ChatCompletionMessageWithFinishReason::new(
                            ChatCompletionMessage {
                                ..Default::default()
                            },
                            ModelFinishReason::ToolCalls,
                        ),
                    ));
                }

                let mut conversational_messages = input_messages.clone();

                let message = Message::builder()
                    .role(role.clone())
                    .set_content(Some(
                        tool_uses
                            .iter()
                            .cloned()
                            .map(ContentBlock::ToolUse)
                            .collect::<Vec<_>>(),
                    ))
                    .build()
                    .map_err(build_err)?;
                conversational_messages.push(message);
                let result_tool_calls =
                    Self::handle_tool_calls(tool_uses, &self.tools, tx, tags.clone())
                        .instrument(tools_span.clone())
                        .await?;
                conversational_messages.push(result_tool_calls);

                Ok(InnerExecutionResult::NextCall(conversational_messages))
            }
            StopReason::EndTurn | StopReason::StopSequence => Ok(InnerExecutionResult::Finish(
                ChatCompletionMessageWithFinishReason::new(
                    ChatCompletionMessage {
                        ..Default::default()
                    },
                    ModelFinishReason::Stop,
                ),
            )),
            other => Err(Self::handle_stop_reason(other).into()),
        }
    }

    pub fn handle_stop_reason(reason: StopReason) -> ModelError {
        let error = match reason {
            StopReason::ContentFiltered => ModelFinishError::ContentFilter,
            StopReason::GuardrailIntervened => ModelFinishError::GuardrailIntervened,
            StopReason::MaxTokens => ModelFinishError::MaxTokens,
            x => ModelFinishError::Custom(format!("Unhandled reason : {x:?}")),
        };
        ModelError::FinishError(error)
    }
}

#[async_trait]
impl ModelInstance for BedrockModel {
    async fn invoke(
        &self,
        input_vars: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<LMessage>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessageWithFinishReason> {
        let (initial_messages, system_messages) =
            self.construct_messages(input_vars.clone(), previous_messages)?;
        self.execute(initial_messages.clone(), system_messages.clone(), &tx, tags)
            .await
    }

    async fn stream(
        &self,
        input_vars: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<LMessage>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<()> {
        let (initial_messages, system_messages) =
            self.construct_messages(input_vars.clone(), previous_messages)?;

        self.execute_stream(initial_messages, system_messages, &tx, tags)
            .await
    }
}

fn construct_human_message(m: &InnerMessage) -> Result<Message, ModelError> {
    let content_blocks = match &m {
        crate::types::threads::InnerMessage::Text(text) => {
            vec![ContentBlock::Text(text.clone())]
        }
        crate::types::threads::InnerMessage::Array(content_array) => {
            let mut content_blocks = vec![];
            for part in content_array {
                match part.r#type {
                    crate::types::threads::MessageContentType::Text => {
                        content_blocks.push(ContentBlock::Text(part.value.clone()));
                    }
                    crate::types::threads::MessageContentType::ImageUrl => {
                        let url = part.value.clone();
                        let base64_data = url
                            .split_once(',')
                            .map_or_else(|| url.as_str(), |(_, data)| data);

                        let image_bytes = base64::engine::general_purpose::STANDARD
                            .decode(base64_data)
                            .map_err(|e| ModelError::CustomError(e.to_string()))?;
                        let image = ImageBlockBuilder::default()
                            .format(aws_sdk_bedrockruntime::types::ImageFormat::Png)
                            .source(aws_sdk_bedrockruntime::types::ImageSource::Bytes(
                                Blob::new(image_bytes),
                            ))
                            .build()
                            .map_err(build_err)?;

                        content_blocks.push(ContentBlock::Image(image));
                    }
                    crate::types::threads::MessageContentType::InputAudio => {
                        todo!()
                    }
                }
            }
            content_blocks
        }
    };

    let message = Message::builder()
        .set_content(Some(content_blocks))
        .role(ConversationRole::User)
        .build()
        .map_err(build_err)?;
    Ok(message)
}

pub struct BedrockModelProvider {
    client: BedrockClient,
}

impl BedrockModelProvider {
    pub async fn new(credentials: BedrockCredentials) -> Result<Self, GatewayApiError> {
        let config = get_sdk_config(Some(&credentials))
            .await
            .map_err(|e| GatewayApiError::GatewayError(GatewayError::ModelError(Box::new(e))))?;
        let client = BedrockClient::new(&config);
        Ok(Self { client })
    }
}

#[async_trait]
impl ModelProviderInstance for BedrockModelProvider {
    async fn get_private_models(&self) -> Result<Vec<ModelMetadata>, GatewayApiError> {
        // List foundation models
        let response = self
            .client
            .list_foundation_models()
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to list Bedrock models: {:?}", e);
                GatewayApiError::GatewayError(GatewayError::ModelError(Box::new(
                    ModelError::CustomError(format!("Failed to list Bedrock models: {}", e)),
                )))
            })?;

        let mut models = Vec::new();

        let mut region_prefix = "";

        if let Some(region) = self.client.config().region() {
            let region = region.to_string();
            region_prefix = if region.starts_with("us") {
                "us."
            } else if region.starts_with("ap") {
                "apac."
            } else if region.starts_with("eu") {
                "eu."
            } else {
                ""
            };
        }

        let prices = pricing::fetch_pricing().await?;

        if let Some(model_summaries) = response.model_summaries {
            for model_summary in &model_summaries {
                // Extract model information
                let model_id = model_summary.model_id.clone();
                let model_arn = model_summary.model_arn.clone();
                if model_arn.ends_with("k") || model_arn.ends_with("m") {
                    continue;
                }

                let first_modality =
                    model_summary
                        .output_modalities
                        .as_ref()
                        .and_then(|output_modalities| {
                            output_modalities.iter().find(|m| {
                                [
                                    aws_sdk_bedrock::types::ModelModality::Embedding,
                                    aws_sdk_bedrock::types::ModelModality::Text,
                                ]
                                .contains(m)
                            })
                        });

                let model_type = match first_modality {
                    Some(aws_sdk_bedrock::types::ModelModality::Embedding) => ModelType::Embeddings,
                    Some(aws_sdk_bedrock::types::ModelModality::Text) => ModelType::Completions,
                    _ => continue,
                };

                let provider_name = model_summary.provider_name.clone().unwrap_or_default();
                let model_name = model_summary.model_name.clone().unwrap_or_default();

                // Determine capabilities based on modalities
                let mut capabilities = Vec::new();

                // Determine input/output formats from modalities
                let input_formats =
                    if let Some(input_modalities) = model_summary.input_modalities.as_ref() {
                        input_modalities
                            .iter()
                            .filter_map(|m| match m.as_str() {
                                "TEXT" => Some(ModelIOFormats::Text),
                                "IMAGE" => Some(ModelIOFormats::Image),
                                "VIDEO" => Some(ModelIOFormats::Video),
                                _ => None,
                            })
                            .collect()
                    } else {
                        vec![ModelIOFormats::Text]
                    };

                if input_formats.len() == 1 {
                    if let Some(input_format) = input_formats.first() {
                        if input_format == &ModelIOFormats::Video {
                            continue;
                        }
                    }
                }

                let output_formats =
                    if let Some(output_modalities) = model_summary.output_modalities.as_ref() {
                        output_modalities
                            .iter()
                            .filter_map(|m| match m.as_str() {
                                "TEXT" => Some(ModelIOFormats::Text),
                                "IMAGE" => Some(ModelIOFormats::Image),
                                _ => None,
                            })
                            .collect()
                    } else {
                        vec![ModelIOFormats::Text]
                    };

                let inference_provider_model_name =
                    if let Some(types) = model_summary.inference_types_supported.as_ref() {
                        if types.iter().any(|t| t.as_str() == "INFERENCE_PROFILE") {
                            format!("{region_prefix}{model_id}")
                        } else {
                            model_arn.clone()
                        }
                    } else {
                        model_arn.clone()
                    };

                let mut price = prices.get(&format!("{region_prefix}{model_id}"));

                if price.is_none() {
                    price = prices.get(&model_id);
                }

                if price.is_none() {
                    tracing::error!("Model is missing in pricing: {:#?}", model_summary);
                }

                // Check if model supports tools/functions based on known models
                if model_id.contains("claude") || model_id.contains("mistral") {
                    capabilities.push(ModelCapability::Tools);
                } else if let Some(price) = price {
                    if price.supports_function_calling.unwrap_or(false) {
                        capabilities.push(ModelCapability::Tools);
                    }
                }

                // Create ModelMetadata
                let metadata = ModelMetadata {
                    model: model_id.clone(),
                    model_provider: provider_name.clone().to_lowercase(),
                    inference_provider: InferenceProvider {
                        provider: InferenceModelProvider::Bedrock,
                        model_name: inference_provider_model_name,
                        endpoint: None,
                    },
                    price: ModelPrice::Completion(CompletionModelPrice {
                        per_input_token: price
                            .and_then(|p| {
                                p.input_cost_per_token
                                    .map(|c| ((c * 1000000.0) * 1000.0).round() / 1000.0)
                            })
                            .unwrap_or(0.0),
                        per_output_token: price
                            .and_then(|p| {
                                p.output_cost_per_token
                                    .map(|c| ((c * 1000000.0) * 1000.0).round() / 1000.0)
                            })
                            .unwrap_or(0.0),
                        per_cached_input_token: None,
                        per_cached_input_write_token: None,
                        valid_from: None,
                    }),
                    input_formats,
                    output_formats,
                    capabilities,
                    r#type: model_type,
                    limits: Limits::new(price.map(|p| p.max_tokens.unwrap_or(0)).unwrap_or(0)), // Default context size, would need model-specific values
                    description: model_name,
                    parameters: None,
                    benchmark_info: None,
                    virtual_model_id: None,
                    min_service_level: 0,
                    release_date: None,
                    license: None,
                    knowledge_cutoff_date: None,
                    langdb_release_date: None,
                    is_private: true,
                };

                models.push(metadata);
            }
        }

        Ok(models)
    }
}

fn map_converse_stream_error(
    e: aws_smithy_runtime_api::client::result::SdkError<
        aws_sdk_bedrockruntime::operation::converse_stream::ConverseStreamError,
        aws_smithy_runtime_api::http::Response,
    >,
) -> ModelError {
    match e.as_service_error() {
        Some(ConverseStreamError::ValidationException(e)) => match e.message() {
            Some(msg) => {
                ModelError::Bedrock(Box::new(BedrockError::ValidationError(msg.to_string())))
            }
            None => ModelError::Bedrock(Box::new(BedrockError::ValidationError(e.to_string()))),
        },
        _ => ModelError::Bedrock(Box::new(e.into())),
    }
}

fn replace_version(model: &str) -> String {
    regex::Regex::new(r"(.*)v(\d+)\.(\d+)")
        .unwrap()
        .replace_all(model, |caps: &regex::Captures| {
            model.replace(
                &format!("v{}.{}", &caps[2], &caps[3]),
                &format!("v{}:{}", &caps[2], &caps[3]),
            )
        })
        .to_string()
}
