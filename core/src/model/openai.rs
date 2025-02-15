use super::error::ModelError;
use super::tools::Tool;
use super::types::{
    LLMContentEvent, LLMFinishEvent, LLMStartEvent, ModelEvent, ModelEventType, ModelFinishReason,
    ModelToolCall,
};
use super::{CredentialsIdent, ModelInstance};
use crate::error::GatewayError;
use crate::events::JsonValue;
use crate::events::SPAN_OPENAI;
use crate::events::{self, RecordResult};
use crate::model::handler::handle_tool_call;
use crate::model::types::LLMFirstToken;
use crate::model::{async_trait, DEFAULT_MAX_RETRIES};
use crate::types::credentials::ApiKeyCredentials;
use crate::types::engine::{ExecutionOptions, OpenAiModelParams, Prompt};
use crate::types::gateway::CompletionModelUsage;
use crate::types::gateway::{ChatCompletionContent, ChatCompletionMessage, ToolCall};
use crate::types::message::{MessageType, PromptMessage};
use crate::types::threads::{InnerMessage, Message};
use crate::GatewayResult;
use async_openai::config::OpenAIConfig;
use async_openai::error::OpenAIError;
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionMessageToolCallChunk,
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessage,
    ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessageArgs,
    ChatCompletionRequestUserMessageContentPart, ChatCompletionTool, ChatCompletionToolArgs,
    ChatCompletionToolChoiceOption, ChatCompletionToolType, CreateChatCompletionRequest,
    CreateChatCompletionRequestArgs, FinishReason, FunctionCall, FunctionCallStream,
    FunctionObject,
};
use async_openai::types::{
    ChatCompletionRequestMessageContentPartImage, CreateChatCompletionStreamResponse, ImageUrl,
};
use async_openai::types::{ChatCompletionRequestToolMessageArgs, CompletionUsage};
use async_openai::types::{ChatCompletionRequestUserMessageContent, ChatCompletionStreamOptions};
use async_openai::Client;
use futures::Stream;
use futures::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::field;
use tracing::Instrument;
use tracing::Span;
use valuable::Valuable;

macro_rules! target {
    () => {
        "langdb::user_tracing::models::openai"
    };
    ($subtgt:literal) => {
        concat!("langdb::user_tracing::models::openai::", $subtgt)
    };
}

enum InnerExecutionResult {
    Finish(ChatCompletionMessage),
    NextCall(Vec<ChatCompletionRequestMessage>),
}

fn custom_err(e: impl ToString) -> ModelError {
    ModelError::CustomError(e.to_string())
}

pub fn openai_client(
    credentials: Option<&ApiKeyCredentials>,
) -> Result<async_openai::Client<async_openai::config::OpenAIConfig>, ModelError> {
    let mut config = OpenAIConfig::new();

    let api_key = if let Some(credentials) = credentials {
        credentials.api_key.clone()
    } else {
        std::env::var("LANGDB_OPENAI_API_KEY").map_err(|_| ModelError::InvalidApiKey)?
    };
    config = config.with_api_key(api_key);

    Ok(Client::with_config(config))
}

#[derive(Clone)]
pub struct OpenAIModel {
    params: OpenAiModelParams,
    execution_options: ExecutionOptions,
    prompt: Prompt,
    client: Client<OpenAIConfig>,
    tools: Arc<HashMap<String, Box<dyn Tool>>>,
    credentials_ident: CredentialsIdent,
}

impl OpenAIModel {
    pub fn new(
        params: OpenAiModelParams,
        credentials: Option<&ApiKeyCredentials>,
        execution_options: ExecutionOptions,
        prompt: Prompt,
        tools: HashMap<String, Box<dyn Tool>>,
        client: Option<Client<OpenAIConfig>>,
    ) -> Result<Self, ModelError> {
        Ok(Self {
            params,
            execution_options,
            prompt,
            client: client.unwrap_or(openai_client(credentials)?),
            tools: Arc::new(tools),
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Langdb),
        })
    }

    pub fn map_tool_call(tool_call: &ChatCompletionMessageToolCall) -> ModelToolCall {
        ModelToolCall {
            tool_id: tool_call.id.clone(),
            tool_name: tool_call.function.name.clone(),
            input: tool_call.function.arguments.clone(),
        }
    }

    async fn handle_tool_calls(
        function_calls: impl Iterator<Item = &ChatCompletionMessageToolCall>,
        tools: &HashMap<String, Box<dyn Tool>>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> Vec<ChatCompletionRequestMessage> {
        futures::future::join_all(function_calls.map(|tool_call| {
            let tags_value = tags.clone();
            async move {
                let id = tool_call.id.clone();
                let function = tool_call.function.clone();
                tracing::trace!("Calling tool ({id}) {function:?}");

                let tool_call = Self::map_tool_call(tool_call);
                let result = handle_tool_call(&tool_call, tools, tx, tags_value).await;
                tracing::trace!("Result ({id}): {result:?}");
                let content = result.unwrap_or_else(|err| err.to_string());
                let content = ChatCompletionRequestToolMessageContent::Text(content);
                ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
                    content,
                    tool_call_id: id.clone(),
                })
            }
        }))
        .await
    }

    fn build_request(
        &self,
        messages: &[ChatCompletionRequestMessage],
        stream: bool,
    ) -> GatewayResult<CreateChatCompletionRequest> {
        let mut chat_completion_tools: Vec<ChatCompletionTool> = vec![];

        for (name, tool) in self.tools.iter() {
            chat_completion_tools.push(
                ChatCompletionToolArgs::default()
                    .r#type(ChatCompletionToolType::Function)
                    .function(FunctionObject {
                        name: name.to_owned(),
                        description: Some(tool.description()),
                        parameters: tool
                            .get_function_parameters()
                            .map(serde_json::to_value)
                            .transpose()?,
                        strict: Some(false),
                    })
                    .build()
                    .map_err(custom_err)?,
            );
        }

        let mut builder = CreateChatCompletionRequestArgs::default();
        let model_params = &self.params;
        if let Some(max_tokens) = model_params.max_tokens {
            builder.max_tokens(max_tokens);
        }
        if let Some(temperature) = model_params.temperature {
            builder.temperature(temperature);
        }

        if let Some(logprobs) = model_params.logprobs {
            builder.logprobs(logprobs);
        }

        if let Some(top_logprobs) = model_params.top_logprobs {
            builder.top_logprobs(top_logprobs);
        }

        if let Some(user) = &model_params.user {
            builder.user(user.clone());
        }

        if let Some(schema) = &model_params.response_format {
            builder.response_format(schema.clone());
        }

        builder
            .model(model_params.model.as_ref().unwrap())
            .messages(messages)
            .stream(stream);
        if !self.tools.is_empty() {
            builder
                .tools(chat_completion_tools)
                .tool_choice(ChatCompletionToolChoiceOption::Auto);
        }

        if stream {
            builder.stream_options(ChatCompletionStreamOptions {
                include_usage: true,
            });
        }

        Ok(builder.build().map_err(custom_err)?)
    }

    async fn process_stream(
        &self,
        mut stream: impl Stream<Item = Result<CreateChatCompletionStreamResponse, OpenAIError>> + Unpin,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        first_response_received: &mut bool,
    ) -> GatewayResult<(
        FinishReason,
        Vec<ChatCompletionMessageToolCall>,
        Option<async_openai::types::CompletionUsage>,
    )> {
        let mut tool_call_states: HashMap<u32, ChatCompletionMessageToolCall> = HashMap::new();
        while let Some(result) = stream.next().await {
            match result {
                Ok(mut response) => {
                    if !*first_response_received {
                        *first_response_received = true;
                        // current time in nanoseconds
                        let now: u64 = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_micros() as u64;
                        tx.send(Some(ModelEvent::new(
                            &Span::current(),
                            ModelEventType::LlmFirstToken(LLMFirstToken { ttft: now }),
                        )))
                        .await
                        .map_err(|e| GatewayError::CustomError(e.to_string()))?;
                    }
                    let chat_choice = response.choices.remove(0);
                    if let Some(tool_calls) = chat_choice.delta.tool_calls {
                        for tool_call in tool_calls.into_iter() {
                            let ChatCompletionMessageToolCallChunk {
                                index,
                                id,
                                function: Some(FunctionCallStream { name, arguments }),
                                ..
                            } = tool_call
                            else {
                                continue;
                            };
                            let state = tool_call_states.entry(index).or_insert_with(|| {
                                ChatCompletionMessageToolCall {
                                    id: id.unwrap(),
                                    r#type: ChatCompletionToolType::Function,
                                    function: FunctionCall {
                                        name: name.unwrap(),
                                        arguments: Default::default(),
                                    },
                                }
                            });
                            if let Some(arguments) = arguments {
                                state.function.arguments.push_str(&arguments);
                            }
                        }
                    }

                    if let Some(content) = &chat_choice.delta.content {
                        let _ = tx
                            .send(Some(ModelEvent::new(
                                &Span::current(),
                                ModelEventType::LlmContent(LLMContentEvent {
                                    content: content.to_owned(),
                                }),
                            )))
                            .await;
                    }

                    if let Some(reason) = &chat_choice.finish_reason {
                        // Collect last chunk. Some providers sends usage with last chunk instead of separate chunk
                        let mut usage = response.usage;
                        if let Some(Ok(response)) = stream.next().await {
                            if let Some(u) = response.usage {
                                usage = Some(u);
                            }
                        }
                        return Ok((*reason, tool_call_states.into_values().collect(), usage));
                    }
                }
                Err(err) => {
                    return Err(ModelError::OpenAIApi(err).into());
                }
            }
        }
        unreachable!();
    }

    async fn execute_inner(
        &self,
        span: Span,
        messages: Vec<ChatCompletionRequestMessage>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<InnerExecutionResult> {
        let call = self.build_request(&messages, false)?;
        let input_messages = call.messages.clone();
        tx.send(Some(ModelEvent::new(
            &span,
            ModelEventType::LlmStart(LLMStartEvent {
                provider_name: SPAN_OPENAI.to_string(),
                model_name: self.params.model.clone().unwrap_or_default(),
                input: serde_json::to_string(&input_messages)?,
            }),
        )))
        .await
        .map_err(|e| GatewayError::CustomError(e.to_string()))?;

        let response = async move {
            let result = self.client.chat().create(call).await;
            let _ = result
                .as_ref()
                .map(|response| serde_json::to_value(response).unwrap())
                .as_ref()
                .map(JsonValue)
                .record();
            let response = result.map_err(custom_err)?;

            let span = Span::current();
            span.record("output", serde_json::to_string(&response)?);
            if let Some(ref usage) = response.usage {
                span.record(
                    "usage",
                    JsonValue(&serde_json::to_value(usage).unwrap()).as_value(),
                );
            }
            Ok::<_, GatewayError>(response)
        }
        .instrument(span.clone().or_current())
        .await?;

        let choices = response.choices;
        if choices.is_empty() {
            return Err(custom_err("No Choices").into());
        }
        // always take 1 since we put n = 1 in request
        let first_choice = choices[0].to_owned();
        let finish_reason = first_choice.finish_reason;
        match finish_reason.as_ref() {
            Some(&FinishReason::ToolCalls) => {
                let tool_calls = first_choice.message.tool_calls.unwrap();
                tracing::warn!("Tool calls: {tool_calls:#?}");

                let content = first_choice.message.content;
                let tool_calls_str = serde_json::to_string(&tool_calls)?;

                let tools_span = tracing::info_span!(target: target!(), parent: span.clone(), events::SPAN_TOOLS, tool_calls=tool_calls_str, label=tool_calls.iter().map(|t| t.function.name.clone()).collect::<Vec<String>>().join(","));
                tools_span.follows_from(span.id());

                let tool_name = tool_calls[0].function.name.clone();
                let tool = self
                    .tools
                    .get(tool_name.as_str())
                    .unwrap_or_else(|| panic!("Tool {tool_name} not found checked"));
                if tool.stop_at_call() {
                    let finish_reason = Self::map_finish_reason(
                        &finish_reason.expect("Finish reason is already checked"),
                    );
                    tx.send(Some(ModelEvent::new(
                        &span,
                        ModelEventType::LlmStop(LLMFinishEvent {
                            provider_name: SPAN_OPENAI.to_string(),
                            model_name: self.params.model.clone().unwrap_or_default(),
                            output: content.clone(),
                            usage: Self::map_usage(response.usage.as_ref()),
                            finish_reason,
                            tool_calls: tool_calls.iter().map(Self::map_tool_call).collect(),
                            credentials_ident: self.credentials_ident.clone(),
                        }),
                    )))
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))?;

                    Ok(InnerExecutionResult::Finish(ChatCompletionMessage {
                        role: "assistant".to_string(),
                        content: content.map(ChatCompletionContent::Text),
                        tool_calls: Some(
                            tool_calls
                                .iter()
                                .map(|tool_call| ToolCall {
                                    id: tool_call.id.clone(),
                                    r#type: match tool_call.r#type {
                                        ChatCompletionToolType::Function => "function".to_string(),
                                    },
                                    function: crate::types::gateway::FunctionCall {
                                        name: tool_call.function.name.clone(),
                                        arguments: tool_call.function.arguments.clone(),
                                    },
                                })
                                .collect(),
                        ),
                        ..Default::default()
                    }))
                } else {
                    let mut messages: Vec<ChatCompletionRequestMessage> =
                        vec![ChatCompletionRequestMessage::Assistant(
                            ChatCompletionRequestAssistantMessageArgs::default()
                                .tool_calls(tool_calls.clone())
                                .build()
                                .map_err(custom_err)?,
                        )];
                    let result_tool_calls =
                        Self::handle_tool_calls(tool_calls.iter(), &self.tools, tx, tags.clone())
                            .instrument(tools_span.clone())
                            .await;
                    messages.extend(result_tool_calls);

                    let conversation_messages = [input_messages, messages].concat();

                    Ok(InnerExecutionResult::NextCall(conversation_messages))
                }
            }

            Some(&FinishReason::Stop) => {
                let finish_reason = Self::map_finish_reason(
                    &finish_reason.expect("Finish reason is already checked"),
                );
                let message_content = first_choice.message.content;
                if let Some(content) = &message_content {
                    tx.send(Some(ModelEvent::new(
                        &span,
                        ModelEventType::LlmStop(LLMFinishEvent {
                            provider_name: SPAN_OPENAI.to_string(),
                            model_name: self.params.model.clone().unwrap_or_default(),
                            output: Some(content.clone()),
                            usage: Self::map_usage(response.usage.as_ref()),
                            finish_reason,
                            tool_calls: vec![],
                            credentials_ident: self.credentials_ident.clone(),
                        }),
                    )))
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))?;

                    Ok(InnerExecutionResult::Finish(ChatCompletionMessage {
                        role: "assistant".to_string(),
                        content: Some(ChatCompletionContent::Text(content.to_string())),
                        ..Default::default()
                    }))
                } else {
                    Err(custom_err("no finish reason").into())
                }
            }
            _ => {
                let err = Self::handle_finish_reason(finish_reason);

                Err(err)
            }
        }
    }

    async fn execute(
        &self,
        input_messages: Vec<ChatCompletionRequestMessage>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessage> {
        let mut openai_calls = vec![input_messages];
        let mut retries = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some(messages) = openai_calls.pop() {
            let input = serde_json::to_string(&messages)?;
            let span = tracing::info_span!(target: target!("chat"), SPAN_OPENAI, input = input, output = field::Empty, error = field::Empty, usage = field::Empty, ttft = field::Empty, tags = JsonValue(&serde_json::to_value(tags.clone()).unwrap_or_default()).as_value());

            if retries == 0 {
                return Err(ModelError::MaxRetriesReached.into());
            } else {
                retries -= 1;
            }

            match self.execute_inner(span, messages, tx, tags.clone()).await? {
                InnerExecutionResult::Finish(message) => return Ok(message),
                InnerExecutionResult::NextCall(messages) => {
                    openai_calls.push(messages);
                    continue;
                }
            }
        }
        unreachable!();
    }

    fn handle_finish_reason(finish_reason: Option<FinishReason>) -> GatewayError {
        match finish_reason {
            Some(FinishReason::Length) => GatewayError::ModelError(ModelError::FinishError(
                "the maximum number of tokens specified in the request was reached".to_string(),
            )),
            Some(FinishReason::ContentFilter) => GatewayError::ModelError(ModelError::FinishError(
                "Content filter blocked the completion".to_string(),
            )),
            x => GatewayError::ModelError(ModelError::FinishError(format!("{x:?}"))),
        }
    }
    fn map_finish_reason(finish_reason: &FinishReason) -> ModelFinishReason {
        match finish_reason {
            FinishReason::Stop => ModelFinishReason::Stop,
            FinishReason::Length => ModelFinishReason::Length,
            FinishReason::ToolCalls => ModelFinishReason::ToolCalls,
            FinishReason::ContentFilter => ModelFinishReason::ContentFilter,
            FinishReason::FunctionCall => ModelFinishReason::Other("FunctionCall".to_string()),
        }
    }
    fn map_usage(usage: Option<&CompletionUsage>) -> Option<CompletionModelUsage> {
        usage.map(|u| CompletionModelUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            ..Default::default()
        })
    }

    async fn execute_stream_inner(
        &self,
        span: Span,
        input_messages: Vec<ChatCompletionRequestMessage>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
        first_response_received: &mut bool,
    ) -> GatewayResult<InnerExecutionResult> {
        tx.send(Some(ModelEvent::new(
            &span,
            ModelEventType::LlmStart(LLMStartEvent {
                provider_name: "openai".to_string(),
                model_name: self.params.model.clone().unwrap_or_default(),
                input: serde_json::to_string(&input_messages)?,
            }),
        )))
        .await
        .map_err(|e| GatewayError::CustomError(e.to_string()))?;

        let request = self.build_request(&input_messages, true)?;

        let stream = self
            .client
            .chat()
            .create_stream(request)
            .await
            .map_err(ModelError::OpenAIApi)?;
        let (finish_reason, tool_calls, usage) = self
            .process_stream(stream, tx, first_response_received)
            .instrument(span.clone())
            .await?;

        let trace_finish_reason = Self::map_finish_reason(&finish_reason);
        tx.send(Some(ModelEvent::new(
            &span,
            ModelEventType::LlmStop(LLMFinishEvent {
                provider_name: SPAN_OPENAI.to_string(),
                model_name: self.params.model.clone().unwrap_or_default(),
                output: None,
                usage: Self::map_usage(usage.as_ref()),
                finish_reason: trace_finish_reason.clone(),
                tool_calls: tool_calls.iter().map(Self::map_tool_call).collect(),
                credentials_ident: self.credentials_ident.clone(),
            }),
        )))
        .await
        .map_err(|e| GatewayError::CustomError(e.to_string()))?;
        if let Some(usage) = usage {
            span.record(
                "usage",
                JsonValue(&serde_json::to_value(usage).unwrap()).as_value(),
            );
        }
        let response = serde_json::json!({
            "finish_reason": trace_finish_reason,
            "tool_calls": tool_calls
        });
        span.record("output", response.to_string());
        match finish_reason {
            FinishReason::Stop => Ok(InnerExecutionResult::Finish(ChatCompletionMessage {
                ..Default::default()
            })),
            FinishReason::ToolCalls => {
                let tool = self
                    .tools
                    .get(tool_calls[0].function.name.as_str())
                    .unwrap();

                let tools_span = tracing::info_span!(target: target!(), parent: span.clone(), events::SPAN_TOOLS, tool_calls=field::Empty, label=tool_calls.iter().map(|t| t.function.name.clone()).collect::<Vec<String>>().join(","));
                tools_span.follows_from(span.id());

                if tool.stop_at_call() {
                    Ok(InnerExecutionResult::Finish(ChatCompletionMessage {
                        ..Default::default()
                    }))
                } else {
                    let mut messages: Vec<ChatCompletionRequestMessage> =
                        vec![ChatCompletionRequestMessage::Assistant(
                            ChatCompletionRequestAssistantMessageArgs::default()
                                .tool_calls(tool_calls.clone())
                                .build()
                                .map_err(custom_err)?,
                        )];
                    let result_tool_calls =
                        Self::handle_tool_calls(tool_calls.iter(), &self.tools, tx, tags.clone())
                            .instrument(tools_span.clone())
                            .await;
                    messages.extend(result_tool_calls);

                    let conversation_messages = [input_messages, messages].concat();
                    tracing::trace!("New messages: {conversation_messages:?}");

                    Ok(InnerExecutionResult::NextCall(conversation_messages))
                }
            }
            other => Err(Self::handle_finish_reason(Some(other))),
        }
    }

    async fn execute_stream(
        &self,
        input_messages: Vec<ChatCompletionRequestMessage>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<()> {
        let mut openai_calls = vec![input_messages];
        let mut retries = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some(input_messages) = openai_calls.pop() {
            if retries == 0 {
                return Err(ModelError::MaxRetriesReached.into());
            } else {
                retries -= 1;
            }

            let input = serde_json::to_string(&input_messages)?;
            let span = tracing::info_span!(target: target!("chat"), SPAN_OPENAI, input = input, output = field::Empty, error = field::Empty, usage = field::Empty, ttft = field::Empty, tags = JsonValue(&serde_json::to_value(tags.clone()).unwrap_or_default()).as_value());

            let first_response_received = &mut false;

            match self
                .execute_stream_inner(
                    span,
                    input_messages,
                    tx,
                    tags.clone(),
                    first_response_received,
                )
                .await?
            {
                InnerExecutionResult::Finish(_) => {
                    break;
                }
                InnerExecutionResult::NextCall(messages) => {
                    openai_calls.push(messages);
                    continue;
                }
            }
        }

        Ok(())
    }

    fn map_previous_messages(
        messages_dto: Vec<Message>,
    ) -> GatewayResult<Vec<ChatCompletionRequestMessage>> {
        // convert serde::Map into HashMap
        let mut messages: Vec<ChatCompletionRequestMessage> = vec![];
        for m in messages_dto.iter() {
            let request_message = {
                match m.r#type {
                    MessageType::SystemMessage => ChatCompletionRequestMessage::System(
                        ChatCompletionRequestSystemMessageArgs::default()
                            .content(m.content.clone().unwrap_or_default())
                            .build()
                            .unwrap_or_default(),
                    ),
                    MessageType::AIMessage => {
                        let mut msg_args = ChatCompletionRequestAssistantMessageArgs::default();
                        msg_args.content(m.content.clone().unwrap_or_default());

                        if let Some(calls) = m.tool_calls.as_ref() {
                            msg_args.tool_calls(
                                calls
                                    .iter()
                                    .map(|c| ChatCompletionMessageToolCall {
                                        id: c.id.clone(),
                                        r#type: ChatCompletionToolType::Function,
                                        function: FunctionCall {
                                            name: c.function.name.clone(),
                                            arguments: c.function.arguments.clone(),
                                        },
                                    })
                                    .collect::<Vec<ChatCompletionMessageToolCall>>(),
                            );
                        }
                        ChatCompletionRequestMessage::Assistant(
                            msg_args.build().unwrap_or_default(),
                        )
                    }
                    MessageType::HumanMessage => construct_user_message(&m.clone().into()),
                    MessageType::ToolResult => ChatCompletionRequestMessage::Tool(
                        ChatCompletionRequestToolMessageArgs::default()
                            .content(m.content.clone().unwrap_or_default())
                            .tool_call_id(
                                m.tool_call_id
                                    .clone()
                                    .ok_or(ModelError::ToolCallIdNotFound)?,
                            )
                            .build()
                            .unwrap_or_default(),
                    ),
                }
            };
            messages.push(request_message);
        }

        Ok(messages)
    }
}

#[async_trait]
impl ModelInstance for OpenAIModel {
    async fn invoke(
        &self,
        input_variables: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessage> {
        let conversational_messages =
            self.construct_messages(input_variables, previous_messages.clone())?;
        self.execute(conversational_messages, &tx, tags).await
    }

    async fn stream(
        &self,
        input_variables: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<()> {
        let conversational_messages =
            self.construct_messages(input_variables, previous_messages.clone())?;

        self.execute_stream(conversational_messages, &tx, tags)
            .await
    }
}

impl OpenAIModel {
    fn construct_messages(
        &self,
        input_variables: HashMap<String, Value>,
        previous_messages: Vec<Message>,
    ) -> GatewayResult<Vec<ChatCompletionRequestMessage>> {
        let mut conversational_messages: Vec<ChatCompletionRequestMessage> = vec![];
        let system_message = self
            .prompt
            .messages
            .iter()
            .find(|m| m.r#type == MessageType::SystemMessage)
            .map(|message| map_chat_messages(message.to_owned(), &input_variables));
        if let Some(system_message) = system_message {
            conversational_messages.push(system_message?);
        }
        let previous_messages = Self::map_previous_messages(previous_messages)?;
        conversational_messages.extend(previous_messages);
        let human_message = self
            .prompt
            .messages
            .iter()
            .find(|m| m.r#type == MessageType::HumanMessage)
            .map(|message| map_chat_messages(message.to_owned(), &input_variables));
        if let Some(human_message) = human_message {
            conversational_messages.push(human_message?);
        }

        Ok(conversational_messages)
    }
}

fn map_chat_messages(
    prompt: PromptMessage,
    variables: &HashMap<String, Value>,
) -> GatewayResult<ChatCompletionRequestMessage> {
    let message = match prompt.r#type {
        MessageType::AIMessage => {
            let raw_message = Prompt::render(prompt.msg, variables.clone());
            ChatCompletionRequestMessage::Assistant(
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content(raw_message)
                    .build()
                    .map_err(ModelError::OpenAIApi)?,
            )
        }
        MessageType::HumanMessage => {
            let msg = prompt.msg;
            let inner_message: InnerMessage = if prompt.wired {
                let value = variables
                    .get(&msg)
                    .ok_or(GatewayError::CustomError(format!("{msg} not specified")))?;
                serde_json::from_value(value.clone())?
            } else {
                InnerMessage::Text(Prompt::render(msg, variables.clone()))
            };
            construct_user_message(&inner_message)
        }
        MessageType::SystemMessage => {
            let raw_message = Prompt::render(prompt.msg, variables.clone());
            ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(raw_message)
                    .build()
                    .map_err(ModelError::OpenAIApi)?,
            )
        }
        MessageType::ToolResult => {
            todo!()
        }
    };
    Ok(message)
}

fn construct_user_message(m: &InnerMessage) -> ChatCompletionRequestMessage {
    let content = match m {
        crate::types::threads::InnerMessage::Text(text) => {
            ChatCompletionRequestUserMessageContent::Text(text.to_owned())
        }
        crate::types::threads::InnerMessage::Array(content_array) => {
            let mut messages = vec![];
            for m in content_array {
                let msg = match m.r#type {
                    crate::types::threads::MessageContentType::Text => {
                        ChatCompletionRequestUserMessageContentPart::Text(m.value.clone().into())
                    }
                    crate::types::threads::MessageContentType::ImageUrl => {
                        ChatCompletionRequestUserMessageContentPart::ImageUrl(
                            ChatCompletionRequestMessageContentPartImage {
                                image_url: ImageUrl {
                                    url: m.value.clone(),
                                    detail: m
                                        .additional_options
                                        .as_ref()
                                        .and_then(|o| o.as_image())
                                        .map(|o| match o {
                                            crate::types::threads::ImageDetail::Auto => {
                                                async_openai::types::ImageDetail::Auto
                                            }
                                            crate::types::threads::ImageDetail::Low => {
                                                async_openai::types::ImageDetail::Low
                                            }
                                            crate::types::threads::ImageDetail::High => {
                                                async_openai::types::ImageDetail::High
                                            }
                                        }),
                                },
                            },
                        )
                    }
                    crate::types::threads::MessageContentType::InputAudio => {
                        todo!()
                    }
                };
                messages.push(msg)
            }
            ChatCompletionRequestUserMessageContent::Array(messages)
        }
    };
    ChatCompletionRequestMessage::User(
        ChatCompletionRequestUserMessageArgs::default()
            .content(content)
            .build()
            .unwrap_or_default(),
    )
}

pub fn record_map_err(e: impl Into<GatewayError> + ToString, span: tracing::Span) -> GatewayError {
    span.record("error", e.to_string());
    e.into()
}
