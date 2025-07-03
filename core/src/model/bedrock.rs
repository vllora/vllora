use super::types::{
    LLMContentEvent, LLMFinishEvent, LLMStartEvent, ModelEvent, ModelEventType, ModelFinishReason,
    ModelToolCall,
};
use super::{CredentialsIdent, ModelInstance};
use crate::error::GatewayError;
use crate::events::{self, JsonValue, RecordResult, SPAN_BEDROCK};
use crate::model::error::BedrockError;
use crate::model::handler::handle_tool_call;
use crate::model::types::LLMFirstToken;
use crate::model::Tool as LangdbTool;
use crate::model::DEFAULT_MAX_RETRIES;
use crate::models::BedrockMetaCompletionModel;
use crate::types::aws::{get_shared_config, get_user_shared_config};
use crate::types::credentials::AwsCredentials;
use crate::types::engine::{BedrockModelParams, ExecutionOptions, Prompt};
use crate::types::gateway::{
    ChatCompletionContent, ChatCompletionMessage, CompletionModelUsage, ToolCall,
};
use crate::types::message::{MessageType, PromptMessage};
use crate::types::provider::BedrockProvider;
use crate::types::threads::InnerMessage;
use crate::types::threads::Message as LMessage;
use crate::GatewayResult;
use async_trait::async_trait;
use aws_sdk_bedrockruntime::operation::converse::builders::ConverseFluentBuilder;
use aws_sdk_bedrockruntime::operation::converse_stream::builders::ConverseStreamFluentBuilder;
use aws_sdk_bedrockruntime::operation::converse_stream::{self, ConverseStreamError};
use aws_sdk_bedrockruntime::types::builders::ImageBlockBuilder;
use aws_sdk_bedrockruntime::types::ConverseOutput::Message as MessageVariant;
use aws_sdk_bedrockruntime::types::{
    ContentBlock, ContentBlockDelta, ContentBlockStart, ConversationRole, ConverseOutput,
    ConverseStreamOutput, InferenceConfiguration, Message, StopReason, SystemContentBlock,
    TokenUsage, Tool, ToolConfiguration, ToolInputSchema, ToolResultBlock, ToolResultContentBlock,
    ToolResultStatus, ToolSpecification, ToolUseBlock,
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

use super::error::ModelError;

macro_rules! target {
    () => {
        "langdb::user_tracing::models::bedrock"
    };
    ($subtgt:literal) => {
        concat!("langdb::user_tracing::models::bedrock::", $subtgt)
    };
}

enum InnerExecutionResult {
    Finish(ChatCompletionMessage),
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

pub async fn bedrock_client(credentials: Option<&AwsCredentials>) -> Result<Client, ModelError> {
    let config = match credentials {
        Some(creds) => get_user_shared_config(creds.clone()).await.load().await,
        None => {
            // TODO: read from env
            get_shared_config(Some(aws_config::Region::new("us-east-1".to_string())))
                .await
                .load()
                .await
        }
    };
    let client = Client::new(&config);
    Ok(client)
}

impl BedrockModel {
    fn get_model_region(model_id: &str) -> Option<String> {
        let us_models = [
            BedrockMetaCompletionModel::Llama318BInstruct.to_string(),
            BedrockMetaCompletionModel::Llama3170BInstruct.to_string(),
            BedrockMetaCompletionModel::Llama321BInstruct.to_string(),
            BedrockMetaCompletionModel::Llama323BInstruct.to_string(),
            BedrockMetaCompletionModel::Llama3211BInstruct.to_string(),
            BedrockMetaCompletionModel::Llama3370BInstruct.to_string(),
        ];

        if us_models.contains(&model_id.to_string()) {
            Some("us".to_string())
        } else {
            None
        }
    }

    pub async fn new(
        model_params: BedrockModelParams,
        execution_options: ExecutionOptions,
        credentials: Option<&AwsCredentials>,
        prompt: Prompt,
        tools: HashMap<String, Box<dyn LangdbTool>>,
        provider: BedrockProvider,
    ) -> Result<Self, ModelError> {
        let client = bedrock_client(credentials).await?;

        let model_id = model_params.model_id.clone().unwrap_or_default();
        let model_name = match credentials {
            Some(_) => model_id,
            None => {
                let provider_name = provider.to_string();
                let model_id = replace_version(&model_id);
                match Self::get_model_region(&model_id) {
                    Some(region) => format!("{region}.{provider_name}.{model_id}"),
                    None => format!("{provider_name}.{model_id}"),
                }
            }
        };

        Ok(Self {
            client,
            execution_options,
            prompt,
            params: model_params,
            tools: Arc::new(tools),
            model_name,
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
    ) -> GatewayResult<ChatCompletionMessage> {
        let mut calls = vec![input_messages];

        let mut retries = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some(input_messages) = calls.pop() {
            let input = serde_json::json!({
                "initial_messages": format!("{input_messages:?}"),
                "system_messages": format!("{system_messages:?}")
            });
            let span = tracing::info_span!(
                target: target!("chat"),
                SPAN_BEDROCK,
                ttft = field::Empty,
                output = field::Empty,
                error = field::Empty,
                usage = field::Empty,
                cost = field::Empty,
                input = JsonValue(&input).as_value(),
                tags = JsonValue(&serde_json::to_value(tags.clone()).unwrap_or_default()).as_value(),
                retries_left = retries
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
                    retries -= 1;
                    span.record("error", e.to_string());
                    if retries == 0 {
                        return Err(e);
                    } else {
                        calls.push(input_messages);
                    }
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
                model_name: self.params.model_id.clone().unwrap_or_default(),
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
                        ContentBlock::Text(content) => {
                            Ok(InnerExecutionResult::Finish(ChatCompletionMessage {
                                role: "assistant".to_string(),
                                content: Some(ChatCompletionContent::Text(content.clone())),
                                ..Default::default()
                            }))
                        }
                        _ => {
                            Err(ModelError::FinishError(
                        "Content block is not in a text format. Currently only TEXT format supported".into(),
                    ).into())
                        }
                    }
                }
                _ => Err(ModelError::FinishError("No output provided".into()).into()),
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
                                ModelError::FinishError(format!(
                                    "Tool {} not found",
                                    tool_uses[0].name
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

                                Ok(InnerExecutionResult::Finish(ChatCompletionMessage {
                                    role: "assistant".to_string(),
                                    tool_calls: Some(tool_calls),
                                    content: content.map(ChatCompletionContent::Text),
                                    ..Default::default()
                                }))
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
                        _ => Err(
                            ModelError::FinishError("Tool use doesnt have message".into()).into(),
                        ),
                    }
                } else {
                    Err(ModelError::FinishError("Tool missing content".to_string()).into())
                }
            }
            x => Err(Self::handle_stop_reason(x).into()),
        }
    }

    async fn process_stream(
        &self,
        stream: converse_stream::ConverseStreamOutput,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
    ) -> GatewayResult<(
        StopReason,
        Option<(ConversationRole, Vec<ToolUseBlock>)>,
        Option<TokenUsage>,
    )> {
        let mut stream = stream.stream;
        let mut role = None;
        let mut tool_uses: HashMap<i32, ToolUseBlock> = HashMap::new();
        let mut usage: Option<TokenUsage> = None;
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
            }
            match output {
                ConverseStreamOutput::ContentBlockDelta(a) => {
                    match a.delta {
                        Some(ContentBlockDelta::Text(t)) => {
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
                    return Ok((
                        event.stop_reason,
                        role.map(|role| (role, tool_uses.into_values().collect())),
                        usage,
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

        let mut retries = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some(input_messages) = calls.pop() {
            let input = serde_json::json!({
                "initial_messages": format!("{input_messages:?}"),
                "system_messages": format!("{system_messages:?}")
            });
            let span = tracing::info_span!(
                target: target!("chat"),
                SPAN_BEDROCK,
                ttft = field::Empty,
                output = field::Empty,
                error = field::Empty,
                usage = field::Empty,
                cost = field::Empty,
                input = JsonValue(&input).as_value(),
                tags = JsonValue(&serde_json::to_value(tags.clone()).unwrap_or_default()).as_value(),
                retries_left = retries
            );

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
                    retries -= 1;
                    span.record("error", e.to_string());
                    if retries == 0 {
                        return Err(e);
                    } else {
                        calls.push(input_messages);
                    }
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

        let response = builder.send().await.map_err(map_converse_stream_error)?;
        let (stop_reason, msg, usage) = self
            .process_stream(response, tx)
            .instrument(span.clone())
            .await?;
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

        let response = serde_json::json!({
            "stop_reason": format!("{stop_reason:?}"),
            "msg": format!("{msg:?}")
        });
        span.record("output", response.to_string());
        match stop_reason {
            StopReason::ToolUse => {
                let Some((role, tool_uses)) = msg else {
                    return Err(ModelError::CustomError("Empty tooluse block".to_string()).into());
                };

                let tool_calls_str = serde_json::to_string(&tool_calls)?;
                let tools_span = tracing::info_span!(target: target!(), events::SPAN_TOOLS, tool_calls=tool_calls_str, label=tool_uses.iter().map(|t| t.name.clone()).collect::<Vec<String>>().join(","));

                let tool = self.tools.get(&tool_calls[0].tool_name).unwrap();
                if tool.stop_at_call() {
                    return Ok(InnerExecutionResult::Finish(ChatCompletionMessage {
                        ..Default::default()
                    }));
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
            StopReason::EndTurn | StopReason::StopSequence => {
                Ok(InnerExecutionResult::Finish(ChatCompletionMessage {
                    ..Default::default()
                }))
            }
            other => Err(Self::handle_stop_reason(other).into()),
        }
    }

    pub fn handle_stop_reason(reason: StopReason) -> ModelError {
        let str = match reason {
            StopReason::ContentFiltered => "Content filter blocked the completion",
            StopReason::GuardrailIntervened => "Guardrail intervened and stopped this execution",
            StopReason::MaxTokens => {
                "the maximum number of tokens specified in the request was reached"
            }
            x => &format!("Unhandled reason : {x:?}"),
        };
        ModelError::FinishError(str.to_string())
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
    ) -> GatewayResult<ChatCompletionMessage> {
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
