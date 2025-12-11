use crate::client::completions::response_stream::ResultStream;
use crate::client::tools::handler::handle_tool_call;
use crate::client::DEFAULT_MAX_RETRIES;
use crate::provider::openai::azure_openai_client;
use crate::provider::openai::is_azure_endpoint;
use crate::provider::openai::openai_client;
use crate::types::gateway::ChatCompletionChunk;
use crate::types::gateway::ChatCompletionChunkChoice;
use crate::types::gateway::ChatCompletionDelta;
use async_trait::async_trait;

use crate::client::error::AuthorizationError;
use crate::client::error::ModelError;
use crate::error::LLMError;
use crate::error::{LLMResult, ModelFinishError};
use crate::types::credentials::ApiKeyCredentials;
use crate::types::credentials_ident::CredentialsIdent;
use crate::types::engine::{render, ExecutionOptions, OpenAiModelParams};
use crate::types::gateway::{ChatCompletionContent, ChatCompletionMessage, ToolCall};
use crate::types::gateway::{ChatCompletionMessageWithFinishReason, GatewayModelUsage};
use crate::types::instance::ModelInstance;
use crate::types::message::{ImageDetail, MessageContentType, MessageType};
use crate::types::message::{InnerMessage, Message};
use crate::types::tools::Tool;
use crate::types::{
    LLMContentEvent, LLMFinishEvent, LLMFirstToken, LLMStartEvent, ModelEvent, ModelEventType,
    ModelFinishReason, ModelToolCall,
};
use async_openai::config::Config;
use async_openai::config::{AzureConfig, OpenAIConfig};
use async_openai::error::OpenAIError;
use async_openai::types::{
    ChatChoice, ChatCompletionMessageToolCall, ChatCompletionMessageToolCallChunk,
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessage,
    ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessageArgs,
    ChatCompletionRequestUserMessageContentPart, ChatCompletionResponseMessage, ChatCompletionTool,
    ChatCompletionToolArgs, ChatCompletionToolChoiceOption, ChatCompletionToolType,
    CreateChatCompletionRequest, CreateChatCompletionRequestArgs, CreateChatCompletionResponse,
    FinishReason, FunctionCall, FunctionCallStream, FunctionObject,
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
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::field;
use tracing::Instrument;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use valuable::Valuable;
use vllora_telemetry::create_model_span;
use vllora_telemetry::events::JsonValue;
use vllora_telemetry::events::SPAN_OPENAI;
use vllora_telemetry::events::{self, RecordResult};

pub type StreamExecutionResult = (
    FinishReason,
    Vec<ChatCompletionMessageToolCall>,
    Option<async_openai::types::CompletionUsage>,
    Option<CreateChatCompletionResponse>,
);

macro_rules! target {
    () => {
        "vllora::user_tracing::models::openai"
    };
    ($subtgt:literal) => {
        concat!("vllora::user_tracing::models::openai::", $subtgt)
    };
}

enum InnerExecutionResult {
    Finish(Box<ChatCompletionMessageWithFinishReason>),
    NextCall(Vec<ChatCompletionRequestMessage>),
}

#[derive(Clone)]
pub struct OpenAIModel<C: Config = OpenAIConfig> {
    params: OpenAiModelParams,
    execution_options: ExecutionOptions,
    client: Client<C>,
    tools: HashMap<String, Arc<Box<dyn Tool>>>,
    credentials_ident: CredentialsIdent,
}

// Specific implementation for OpenAIConfig
impl OpenAIModel<OpenAIConfig> {
    pub fn new(
        params: OpenAiModelParams,
        credentials: Option<&ApiKeyCredentials>,
        execution_options: ExecutionOptions,
        tools: HashMap<String, Arc<Box<dyn Tool>>>,
        client: Option<Client<OpenAIConfig>>,
        endpoint: Option<&str>,
    ) -> Result<Self, ModelError> {
        // Return an error if this is an Azure endpoint
        if let Some(ep) = endpoint {
            if is_azure_endpoint(ep) {
                return Err(ModelError::CustomError(format!(
                    "Azure endpoints should be created via OpenAIModel::from_azure_url: {ep}"
                )));
            }
        }

        let client = client.unwrap_or(openai_client(credentials, endpoint)?);

        Ok(Self {
            params,
            execution_options,
            client,
            tools,
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Vllora),
        })
    }
}

// Specific implementation for AzureConfig
impl OpenAIModel<AzureConfig> {
    pub fn new_azure(
        params: OpenAiModelParams,
        credentials: Option<&ApiKeyCredentials>,
        execution_options: ExecutionOptions,
        tools: HashMap<String, Arc<Box<dyn Tool>>>,
        client: Option<Client<AzureConfig>>,
        endpoint: Option<&str>,
    ) -> Result<Self, ModelError> {
        let client = if let Some(client) = client {
            client
        } else if let Some(endpoint) = endpoint {
            let api_key = if let Some(credentials) = credentials {
                credentials.api_key.clone()
            } else {
                std::env::var("VLLORA_OPENAI_API_KEY")
                    .map_err(|_| AuthorizationError::InvalidApiKey)?
            };
            azure_openai_client(api_key, endpoint, &params.model.clone().unwrap_or_default())
        } else {
            return Err(ModelError::CustomError(
                "Azure OpenAI requires an endpoint URL".to_string(),
            ));
        };

        Ok(Self {
            params,
            execution_options,
            client,
            tools,
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Vllora),
        })
    }

    // Helper to create from a URL
    pub fn from_azure_url(
        params: OpenAiModelParams,
        credentials: Option<&ApiKeyCredentials>,
        execution_options: ExecutionOptions,
        tools: HashMap<String, Arc<Box<dyn Tool>>>,
        endpoint: &str,
    ) -> Result<Self, ModelError> {
        Self::new_azure(
            params,
            credentials,
            execution_options,
            tools,
            None,
            Some(endpoint),
        )
    }
}

// Common implementation for all Config types
impl<C: Config> OpenAIModel<C> {
    pub fn map_tool_call(tool_call: &ChatCompletionMessageToolCall) -> ModelToolCall {
        ModelToolCall {
            tool_id: tool_call.id.clone(),
            tool_name: tool_call.function.name.clone(),
            input: tool_call.function.arguments.clone(),
            extra_content: None,
        }
    }

    async fn handle_tool_calls(
        function_calls: impl Iterator<Item = &ChatCompletionMessageToolCall>,
        tools: &HashMap<String, Arc<Box<dyn Tool>>>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> HashMap<String, String> {
        let result = futures::future::join_all(function_calls.map(|tool_call| {
            let tags_value = tags.clone();
            async move {
                let id = tool_call.id.clone();
                let function = tool_call.function.clone();
                tracing::trace!("Calling tool ({id}) {function:?}");

                let tool_call = Self::map_tool_call(tool_call);
                let result = handle_tool_call(&tool_call, tools, tx, tags_value).await;
                tracing::trace!("Result ({id}): {result:?}");
                let content = result.unwrap_or_else(|err| err.to_string());
                (id, content)
            }
        }))
        .await;

        HashMap::from_iter(result)
    }

    fn map_tool_call_results(
        results: HashMap<String, String>,
    ) -> Vec<ChatCompletionRequestMessage> {
        results
            .into_iter()
            .map(|(id, content)| {
                ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
                    content: ChatCompletionRequestToolMessageContent::Text(content),
                    tool_call_id: id,
                })
            })
            .collect()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn build_request(
        &self,
        messages: &[ChatCompletionRequestMessage],
        stream: bool,
    ) -> LLMResult<CreateChatCompletionRequest> {
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
                            .map(|mut s| {
                                if s.required.is_none() {
                                    s.required = Some(vec![]);
                                }

                                serde_json::to_value(s)
                            })
                            .transpose()?,
                        strict: Some(false),
                    })
                    .build()
                    .map_err(|e| ModelError::OpenAIApi(Box::new(e)))?,
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

        if let Some(prompt_cache_key) = &model_params.prompt_cache_key {
            builder.prompt_cache_key(prompt_cache_key.clone());
        }

        if stream {
            builder.stream_options(ChatCompletionStreamOptions {
                include_usage: true,
            });
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

        Ok(builder
            .build()
            .map_err(|e| ModelError::OpenAIApi(Box::new(e)))?)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn build_response(
        first_chunk: Option<&CreateChatCompletionStreamResponse>,
        tool_call_states: &[ChatCompletionMessageToolCall],
        usage: Option<async_openai::types::CompletionUsage>,
        stream_content: String,
        finish_reason: &FinishReason,
    ) -> Option<CreateChatCompletionResponse> {
        match first_chunk {
            Some(first_chunk) => {
                let choices = if let Some(first_chunk) = first_chunk.choices.first() {
                    vec![ChatChoice {
                        index: 0,
                        message: ChatCompletionResponseMessage {
                            content: Some(stream_content),
                            role: first_chunk.delta.role.unwrap_or_default(),
                            tool_calls: Some(tool_call_states.to_vec()),
                            refusal: first_chunk.delta.refusal.clone(),
                            #[allow(deprecated)]
                            function_call: None,
                            audio: None,
                        },
                        finish_reason: Some(*finish_reason),
                        logprobs: None,
                    }]
                } else {
                    vec![]
                };

                Some(CreateChatCompletionResponse {
                    id: first_chunk.id.clone(),
                    created: first_chunk.created,
                    model: first_chunk.model.clone(),
                    object: Some("chat.completion".to_string()),
                    usage: usage.clone(),
                    choices,
                    service_tier: first_chunk.service_tier.clone(),
                    system_fingerprint: first_chunk.system_fingerprint.clone(),
                })
            }
            None => None,
        }
    }

    async fn process_stream(
        &self,
        mut stream: impl Stream<Item = Result<CreateChatCompletionStreamResponse, OpenAIError>> + Unpin,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tx_response: &tokio::sync::mpsc::Sender<LLMResult<ChatCompletionChunk>>,
        started_at: std::time::Instant,
    ) -> LLMResult<StreamExecutionResult> {
        let mut tool_call_states: HashMap<u32, ChatCompletionMessageToolCall> = HashMap::new();
        let mut first_chunk = None;
        let mut stream_content = String::new();
        let mut finish_reason = None;
        let mut usage = None;

        let mut first_response_received = false;

        while let Some(result) = stream.next().await {
            match result {
                Ok(mut response) => {
                    if !first_response_received {
                        first_response_received = true;
                        Span::current().add_event("llm.first_token", vec![]);
                        let _ = tx
                            .send(Some(ModelEvent::new(
                                &Span::current(),
                                ModelEventType::LlmFirstToken(LLMFirstToken {}),
                            )))
                            .await;
                        first_chunk = Some(response.clone());
                        Span::current().record("ttft", started_at.elapsed().as_micros());
                    }

                    let chunk = ChatCompletionChunk {
                        id: response.id.clone(),
                        object: response
                            .object
                            .unwrap_or("chat.completion.chunk".to_string()),
                        created: response.created as i64,
                        model: response.model.clone(),
                        choices: vec![],
                        usage: None,
                    };

                    if response.choices.is_empty() {
                        // XAI bug workaround
                        if let Some(usage) = response.usage {
                            // If there are no tool calls, it means the response is finished with all content passed
                            let reason = match tool_call_states.len() {
                                0 => FinishReason::Stop,
                                _ => FinishReason::ToolCalls,
                            };

                            let tool_calls =
                                tool_call_states.clone().into_values().collect::<Vec<_>>();
                            let response = Self::build_response(
                                first_chunk.as_ref(),
                                &tool_calls,
                                Some(usage.clone()),
                                stream_content,
                                &reason,
                            );

                            return Ok((reason, tool_calls.clone(), Some(usage.clone()), response));
                        }

                        continue;
                    }

                    let chat_choice = response.choices.remove(0);
                    let mut has_content = false;
                    if let Some(tool_calls) = &chat_choice.delta.tool_calls {
                        has_content = true;
                        for tool_call in tool_calls.iter() {
                            let ChatCompletionMessageToolCallChunk {
                                index,
                                id,
                                function: Some(FunctionCallStream { name, arguments }),
                                ..
                            } = tool_call
                            else {
                                continue;
                            };
                            let state = tool_call_states.entry(*index).or_insert_with(|| {
                                ChatCompletionMessageToolCall {
                                    id: id.clone().unwrap(),
                                    r#type: ChatCompletionToolType::Function,
                                    function: FunctionCall {
                                        name: name.clone().unwrap(),
                                        arguments: Default::default(),
                                    },
                                }
                            });
                            if let Some(arguments) = arguments {
                                state.function.arguments.push_str(arguments);
                            }
                        }
                    }

                    if let Some(content) = &chat_choice.delta.content {
                        has_content = true;
                        Self::send_event(
                            tx,
                            ModelEvent::new(
                                &Span::current(),
                                ModelEventType::LlmContent(LLMContentEvent {
                                    content: content.to_owned(),
                                }),
                            ),
                        )
                        .await;
                        stream_content.push_str(content);
                    }

                    if has_content {
                        let mut chunk_clone = chunk.clone();
                        chunk_clone.choices.push(ChatCompletionChunkChoice {
                            index: chat_choice.index as i32,
                            delta: ChatCompletionDelta {
                                content: chat_choice.delta.content.clone(),
                                role: chat_choice.delta.role.map(|r| r.to_string()),
                                tool_calls: chat_choice
                                    .delta
                                    .tool_calls
                                    .as_ref()
                                    .map(|t| t.iter().map(|t| t.into()).collect()),
                            },
                            finish_reason: None,
                            logprobs: chat_choice.logprobs.clone(),
                        });
                        let _ = tx_response.send(Ok(chunk_clone)).await;
                    }

                    if let Some(reason) = &chat_choice.finish_reason {
                        finish_reason = Some(*reason);
                    }

                    if response.usage.is_some() {
                        usage = response.usage.clone();
                    }
                }
                Err(err) => {
                    tracing::warn!("OpenAI API error: {err}");
                    return Err(ModelError::OpenAIApi(Box::new(err)).into());
                }
            }
        }

        match finish_reason {
            Some(finish_reason) => {
                let tool_calls = tool_call_states.clone().into_values().collect::<Vec<_>>();
                let response = Self::build_response(
                    first_chunk.as_ref(),
                    &tool_calls,
                    usage.clone(),
                    stream_content,
                    &finish_reason,
                );

                Ok((finish_reason, tool_calls, usage, response))
            }
            _ => unreachable!(),
        }
    }

    async fn send_event(tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>, event: ModelEvent) {
        let _ = tx.send(Some(event)).await;
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute_inner(
        &self,
        span: Span,
        messages: Vec<ChatCompletionRequestMessage>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> LLMResult<InnerExecutionResult> {
        let call = self.build_request(&messages, false)?;
        span.record("request", serde_json::to_string(&call)?);

        let input_messages = call.messages.clone();
        let _ = tx
            .send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStart(LLMStartEvent {
                    provider_name: SPAN_OPENAI.to_string(),
                    model_name: self.params.model.clone().unwrap_or_default(),
                    input: serde_json::to_string(&input_messages)?,
                }),
            )))
            .await;

        let response = async move {
            let result = self.client.chat().create(call).await;
            let _ = result
                .as_ref()
                .map(|response| serde_json::to_value(response).unwrap())
                .as_ref()
                .map(JsonValue)
                .record();
            let response = result.map_err(|e| ModelError::OpenAIApi(Box::new(e)))?;

            let span = Span::current();
            span.record("output", serde_json::to_string(&response)?);
            if let Some(ref usage) = response.usage {
                span.record(
                    "raw_usage",
                    JsonValue(&serde_json::to_value(usage).unwrap()).as_value(),
                );
                span.record(
                    "usage",
                    JsonValue(&serde_json::to_value(Self::map_usage(Some(usage))).unwrap())
                        .as_value(),
                );
            }
            Ok::<_, LLMError>(response)
        }
        .instrument(span.clone().or_current())
        .await?;

        let choices = response.choices;
        if choices.is_empty() {
            return Err(ModelError::FinishError(ModelFinishError::NoChoices).into());
        }
        // always take 1 since we put n = 1 in request
        let first_choice = choices[0].to_owned();

        let mut finish_reason = first_choice.finish_reason;
        // XAI bug workaround
        if let Some(content) = &first_choice.message.content {
            if content.is_empty() && first_choice.message.tool_calls.is_some() {
                finish_reason = Some(FinishReason::ToolCalls);
            }
        }

        match finish_reason.as_ref() {
            Some(&FinishReason::ToolCalls) => {
                let tool_calls = first_choice.message.tool_calls.unwrap();
                tracing::warn!("Tool calls: {tool_calls:#?}");

                let content = first_choice.message.content;

                let tool_names = map_tool_names(&tool_calls);
                let tools_span = tracing::info_span!(
                    target: target!(),
                    parent: span.clone(),
                    events::SPAN_TOOLS,
                    tool_calls=JsonValue(&serde_json::to_value(&tool_calls)?).as_value(),
                    tool.name=tool_names
                );
                tools_span.follows_from(span.id());

                let tool_name = tool_calls[0].function.name.clone();
                let tool = self
                    .tools
                    .get(tool_name.as_str())
                    .unwrap_or_else(|| panic!("Tool {tool_name} not found checked"));
                let finish_reason = Self::map_finish_reason(
                    &finish_reason.expect("Finish reason is already checked"),
                );
                let _ = tx
                    .send(Some(ModelEvent::new(
                        &span,
                        ModelEventType::LlmStop(LLMFinishEvent {
                            provider_name: SPAN_OPENAI.to_string(),
                            model_name: self.params.model.clone().unwrap_or_default(),
                            output: content.clone(),
                            usage: Self::map_usage(response.usage.as_ref()),
                            finish_reason: finish_reason.clone(),
                            tool_calls: tool_calls.iter().map(Self::map_tool_call).collect(),
                            credentials_ident: self.credentials_ident.clone(),
                        }),
                    )))
                    .await;

                if tool.stop_at_call() {
                    Ok(InnerExecutionResult::Finish(
                        ChatCompletionMessageWithFinishReason::new(
                            ChatCompletionMessage {
                                role: "assistant".to_string(),
                                content: content.map(ChatCompletionContent::Text),
                                tool_calls: Some(
                                    tool_calls
                                        .iter()
                                        .enumerate()
                                        .map(|(index, tool_call)| ToolCall {
                                            index: Some(index),
                                            id: tool_call.id.clone(),
                                            r#type: match tool_call.r#type {
                                                ChatCompletionToolType::Function => {
                                                    "function".to_string()
                                                }
                                            },
                                            function: crate::types::gateway::FunctionCall {
                                                name: tool_call.function.name.clone(),
                                                arguments: tool_call.function.arguments.clone(),
                                            },
                                            extra_content: None,
                                        })
                                        .collect(),
                                ),
                                ..Default::default()
                            },
                            finish_reason,
                            response.id,
                            response.created,
                            response.model,
                            Self::map_usage(response.usage.as_ref()),
                        )
                        .into(),
                    ))
                } else {
                    let mut messages: Vec<ChatCompletionRequestMessage> =
                        vec![ChatCompletionRequestMessage::Assistant(
                            ChatCompletionRequestAssistantMessageArgs::default()
                                .tool_calls(tool_calls.clone())
                                .build()
                                .map_err(|e| ModelError::OpenAIApi(Box::new(e)))?,
                        )];
                    let result_tool_calls =
                        Self::handle_tool_calls(tool_calls.iter(), &self.tools, tx, tags.clone())
                            .instrument(tools_span.clone())
                            .await;
                    tools_span.record(
                        "tool_results",
                        JsonValue(&serde_json::to_value(&result_tool_calls)?).as_value(),
                    );
                    messages.extend(Self::map_tool_call_results(result_tool_calls));

                    let conversation_messages = [input_messages, messages].concat();

                    Ok(InnerExecutionResult::NextCall(conversation_messages))
                }
            }

            Some(&FinishReason::Stop) | Some(&FinishReason::Length) => {
                let finish_reason = Self::map_finish_reason(
                    &finish_reason.expect("Finish reason is already checked"),
                );
                let message_content = first_choice.message.content;
                if let Some(content) = &message_content {
                    let usage = Self::map_usage(response.usage.as_ref());
                    let _ = tx
                        .send(Some(ModelEvent::new(
                            &span,
                            ModelEventType::LlmStop(LLMFinishEvent {
                                provider_name: SPAN_OPENAI.to_string(),
                                model_name: self.params.model.clone().unwrap_or_default(),
                                output: Some(content.clone()),
                                usage: usage.clone(),
                                finish_reason: finish_reason.clone(),
                                tool_calls: vec![],
                                credentials_ident: self.credentials_ident.clone(),
                            }),
                        )))
                        .await;

                    Ok(InnerExecutionResult::Finish(
                        ChatCompletionMessageWithFinishReason::new(
                            ChatCompletionMessage {
                                role: "assistant".to_string(),
                                content: Some(ChatCompletionContent::Text(content.to_string())),
                                ..Default::default()
                            },
                            finish_reason,
                            response.id,
                            response.created,
                            response.model,
                            usage,
                        )
                        .into(),
                    ))
                } else {
                    Err(ModelError::FinishError(ModelFinishError::NoOutputProvided).into())
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
    ) -> LLMResult<ChatCompletionMessageWithFinishReason> {
        let mut openai_calls = vec![input_messages];
        let mut retries_left = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some(messages) = openai_calls.pop() {
            let input = serde_json::to_string(&messages)?;
            let span = create_model_span!(
                SPAN_OPENAI,
                target!("chat"),
                tags,
                retries_left,
                input = input
            );

            match self
                .execute_inner(span.clone(), messages.clone(), tx, tags.clone())
                .await
            {
                Ok(InnerExecutionResult::Finish(message)) => return Ok(*message),
                Ok(InnerExecutionResult::NextCall(messages)) => {
                    openai_calls.push(messages);
                }
                Err(e) => {
                    span.record("error", e.to_string());
                    if retries_left == 0 {
                        return Err(e);
                    } else {
                        openai_calls.push(messages);
                    }
                    retries_left -= 1;
                }
            }
        }
        unreachable!();
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn handle_finish_reason(finish_reason: Option<FinishReason>) -> LLMError {
        match finish_reason {
            Some(FinishReason::ContentFilter) => {
                ModelError::FinishError(ModelFinishError::ContentFilter).into()
            }
            x => ModelError::FinishError(ModelFinishError::Custom(format!("{x:?}"))).into(),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn map_finish_reason(finish_reason: &FinishReason) -> ModelFinishReason {
        match finish_reason {
            FinishReason::Stop => ModelFinishReason::Stop,
            FinishReason::Length => ModelFinishReason::Length,
            FinishReason::ToolCalls => ModelFinishReason::ToolCalls,
            FinishReason::ContentFilter => ModelFinishReason::ContentFilter,
            FinishReason::FunctionCall => ModelFinishReason::Other("FunctionCall".to_string()),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn map_usage(usage: Option<&CompletionUsage>) -> Option<GatewayModelUsage> {
        usage.map(GatewayModelUsage::from)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute_stream_inner(
        &self,
        span: Span,
        input_messages: Vec<ChatCompletionRequestMessage>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tx_response: &tokio::sync::mpsc::Sender<LLMResult<ChatCompletionChunk>>,
        tags: HashMap<String, String>,
    ) -> LLMResult<InnerExecutionResult> {
        let request = self.build_request(&input_messages, true)?;
        span.record("request", serde_json::to_string(&request)?);

        let _ = tx
            .send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStart(LLMStartEvent {
                    provider_name: "openai".to_string(),
                    model_name: self.params.model.clone().unwrap_or_default(),
                    input: serde_json::to_string(&input_messages)?,
                }),
            )))
            .await;

        let started_at = std::time::Instant::now();
        let stream = self
            .client
            .chat()
            .create_stream(request)
            .await
            .map_err(|e| ModelError::OpenAIApi(Box::new(e)))?;
        let (finish_reason, tool_calls, usage, response) = self
            .process_stream(stream, tx, tx_response, started_at)
            .instrument(span.clone())
            .await?;

        span.record("output", serde_json::to_string(&response)?);
        let model_finish_reason = Self::map_finish_reason(&finish_reason);
        let _ = tx
            .send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStop(LLMFinishEvent {
                    provider_name: SPAN_OPENAI.to_string(),
                    model_name: self.params.model.clone().unwrap_or_default(),
                    output: None,
                    usage: Self::map_usage(usage.as_ref()),
                    finish_reason: model_finish_reason.clone(),
                    tool_calls: tool_calls.iter().map(Self::map_tool_call).collect(),
                    credentials_ident: self.credentials_ident.clone(),
                }),
            )))
            .await;
        if let Some(response) = &response {
            let chunk = ChatCompletionChunk {
                id: response.id.clone(),
                object: response
                    .object
                    .clone()
                    .unwrap_or("chat.completion.chunk".to_string()),
                created: response.created as i64,
                model: response.model.clone(),
                choices: vec![],
                usage: None,
            };

            let mut chunk_clone = chunk.clone();

            chunk_clone.choices.push(ChatCompletionChunkChoice {
                index: 0,
                delta: ChatCompletionDelta::default(),
                finish_reason: Some(model_finish_reason.to_string()),
                logprobs: None,
            });
            let _ = tx_response.send(Ok(chunk_clone)).await;

            if let Some(usage) = &usage {
                let mut chunk_clone = chunk.clone();
                chunk_clone.usage = Some(usage.clone().into());
                let _ = tx_response.send(Ok(chunk_clone)).await;
            }
        }

        let mut completion_model_usage = None;
        if let Some(usage) = usage {
            span.record(
                "raw_usage",
                JsonValue(&serde_json::to_value(usage.clone()).unwrap()).as_value(),
            );
            completion_model_usage = Self::map_usage(Some(&usage));
            span.record(
                "usage",
                JsonValue(&serde_json::to_value(completion_model_usage.clone()).unwrap())
                    .as_value(),
            );
        }

        match finish_reason {
            FinishReason::Stop | FinishReason::Length => Ok(InnerExecutionResult::Finish(
                ChatCompletionMessageWithFinishReason::new(
                    ChatCompletionMessage {
                        ..Default::default()
                    },
                    Self::map_finish_reason(&finish_reason),
                    response.as_ref().map(|r| r.id.clone()).unwrap_or_default(),
                    response.as_ref().map(|r| r.created).unwrap_or_default(),
                    response
                        .as_ref()
                        .map(|r| r.model.clone())
                        .unwrap_or_default(),
                    completion_model_usage,
                )
                .into(),
            )),
            FinishReason::ToolCalls => {
                let tool = self
                    .tools
                    .get(tool_calls[0].function.name.as_str())
                    .unwrap();

                let tool_names = tool_calls
                    .iter()
                    .map(|t| t.function.name.clone())
                    .collect::<Vec<String>>()
                    .join(",");
                let tools_span = tracing::info_span!(
                    target: target!(),
                    parent: span.clone(),
                    events::SPAN_TOOLS,
                    tool_calls=JsonValue(&serde_json::to_value(&tool_calls)?).as_value(),
                    tool_results=field::Empty,
                    tool.name=tool_names
                );
                tools_span.follows_from(span.id());

                if tool.stop_at_call() {
                    Ok(InnerExecutionResult::Finish(
                        ChatCompletionMessageWithFinishReason::new(
                            ChatCompletionMessage {
                                ..Default::default()
                            },
                            Self::map_finish_reason(&finish_reason),
                            response.as_ref().map(|r| r.id.clone()).unwrap_or_default(),
                            response.as_ref().map(|r| r.created).unwrap_or_default(),
                            response
                                .as_ref()
                                .map(|r| r.model.clone())
                                .unwrap_or_default(),
                            completion_model_usage,
                        )
                        .into(),
                    ))
                } else {
                    let mut messages: Vec<ChatCompletionRequestMessage> =
                        vec![ChatCompletionRequestMessage::Assistant(
                            ChatCompletionRequestAssistantMessageArgs::default()
                                .tool_calls(tool_calls.clone())
                                .build()
                                .map_err(|e| ModelError::OpenAIApi(Box::new(e)))?,
                        )];
                    let result_tool_calls =
                        Self::handle_tool_calls(tool_calls.iter(), &self.tools, tx, tags.clone())
                            .instrument(tools_span.clone())
                            .await;
                    tools_span.record(
                        "tool_results",
                        JsonValue(&serde_json::to_value(&result_tool_calls)?).as_value(),
                    );
                    messages.extend(Self::map_tool_call_results(result_tool_calls));

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
        tx_response: &tokio::sync::mpsc::Sender<LLMResult<ChatCompletionChunk>>,
        tags: HashMap<String, String>,
    ) -> LLMResult<()> {
        let mut openai_calls = vec![input_messages];
        let mut retries_left = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some(input_messages) = openai_calls.pop() {
            let input = serde_json::to_string(&input_messages)?;
            let span = create_model_span!(
                SPAN_OPENAI,
                target!("chat"),
                tags,
                retries_left,
                input = input
            );

            match self
                .execute_stream_inner(
                    span.clone(),
                    input_messages.clone(),
                    tx,
                    tx_response,
                    tags.clone(),
                )
                .await
            {
                Ok(InnerExecutionResult::Finish(_)) => {
                    break;
                }
                Ok(InnerExecutionResult::NextCall(messages)) => {
                    openai_calls.push(messages);
                }
                Err(e) => {
                    span.record("error", e.to_string());
                    if retries_left == 0 {
                        return Err(e);
                    } else {
                        openai_calls.push(input_messages);
                    }
                    retries_left -= 1;
                }
            }
        }

        Ok(())
    }

    fn map_previous_messages(
        messages_dto: Vec<Message>,
        input_variables: HashMap<String, Value>,
    ) -> LLMResult<Vec<ChatCompletionRequestMessage>> {
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
                        msg_args.content(render(
                            m.content.clone().unwrap_or_default(),
                            &input_variables,
                        ));

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
                    MessageType::HumanMessage => {
                        construct_user_message(&m.clone().into(), input_variables.clone())
                    }
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
impl<C> ModelInstance for OpenAIModel<C>
where
    C: Config + std::marker::Sync + std::marker::Send + Clone + 'static,
{
    async fn invoke(
        &self,
        input_variables: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> LLMResult<ChatCompletionMessageWithFinishReason> {
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
    ) -> LLMResult<ResultStream> {
        let conversational_messages =
            self.construct_messages(input_variables, previous_messages.clone())?;

        let (tx_response, rx_response) = tokio::sync::mpsc::channel(10000);
        let model = (*self).clone();
        let tx_clone = tx.clone();
        tokio::spawn(
            async move {
                let result = model
                    .execute_stream(conversational_messages, &tx_clone, &tx_response, tags)
                    .instrument(tracing::Span::current())
                    .await;

                if let Err(e) = result {
                    let _ = tx_response.send(Err(e)).await;
                }
            }
            .instrument(tracing::Span::current()),
        );

        Ok(ResultStream::create(rx_response))
    }
}

impl<C: Config> OpenAIModel<C> {
    fn construct_messages(
        &self,
        input_variables: HashMap<String, Value>,
        previous_messages: Vec<Message>,
    ) -> LLMResult<Vec<ChatCompletionRequestMessage>> {
        let mut conversational_messages: Vec<ChatCompletionRequestMessage> = vec![];
        let previous_messages =
            Self::map_previous_messages(previous_messages, input_variables.clone())?;
        conversational_messages.extend(previous_messages);

        Ok(conversational_messages)
    }
}

fn construct_user_message(
    m: &InnerMessage,
    variables: HashMap<String, Value>,
) -> ChatCompletionRequestMessage {
    let content = match m {
        InnerMessage::Text(text) => {
            ChatCompletionRequestUserMessageContent::Text(render(text.clone(), &variables.clone()))
        }
        InnerMessage::Array(content_array) => {
            let mut messages = vec![];
            for m in content_array {
                let msg = match m.r#type {
                    MessageContentType::Text => ChatCompletionRequestUserMessageContentPart::Text(
                        render(m.value.clone(), &variables).into(),
                    ),
                    MessageContentType::ImageUrl => {
                        ChatCompletionRequestUserMessageContentPart::ImageUrl(
                            ChatCompletionRequestMessageContentPartImage {
                                image_url: ImageUrl {
                                    url: m.value.clone(),
                                    detail: m
                                        .additional_options
                                        .as_ref()
                                        .and_then(|o| o.as_image())
                                        .map(|o| match o {
                                            ImageDetail::Auto => {
                                                async_openai::types::ImageDetail::Auto
                                            }
                                            ImageDetail::Low => {
                                                async_openai::types::ImageDetail::Low
                                            }
                                            ImageDetail::High => {
                                                async_openai::types::ImageDetail::High
                                            }
                                        }),
                                },
                            },
                        )
                    }
                    MessageContentType::InputAudio => {
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

pub fn record_map_err(e: impl Into<LLMError> + ToString, span: tracing::Span) -> LLMError {
    span.record("error", e.to_string());
    e.into()
}

fn map_tool_names(tool_calls: &[ChatCompletionMessageToolCall]) -> String {
    tool_calls
        .iter()
        .map(|tool_call| tool_call.function.name.clone())
        .collect::<HashSet<String>>()
        .into_iter()
        .collect::<Vec<String>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::tests::MockStreamServer;

    fn get_instance(url: &str) -> OpenAIModel {
        OpenAIModel::new(
            OpenAiModelParams {
                model: Some("gpt-3.5-turbo-0125".to_string()),
                ..Default::default()
            },
            Some(&ApiKeyCredentials {
                api_key: "test".to_string(),
            }),
            ExecutionOptions::default(),
            HashMap::new(),
            None,
            Some(url),
        )
        .expect("Failed to create instance")
    }

    #[tokio::test]
    async fn test_stream_request() {
        // Start the mock server
        let server = MockStreamServer::start()
            .await
            .expect("Failed to start mock server");
        let server_url = server.url();

        // Set some test events (you can modify these later)
        server.set_events(vec![
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-3.5-turbo-0125","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#.to_string(),
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-3.5-turbo-0125","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}"#.to_string(),
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-3.5-turbo-0125","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#.to_string(),
        ]).await;

        // Give the server a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let instance = get_instance(&server_url);

        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        instance
            .stream(HashMap::new(), tx, vec![], HashMap::new())
            .await
            .expect("Failed to stream");

        let mut index = 0;
        while let Some(event) = rx.recv().await {
            match index {
                0 => assert!(matches!(
                    event.unwrap().event,
                    ModelEventType::LlmStart(LLMStartEvent { .. })
                )),
                1 => assert!(matches!(
                    event.unwrap().event,
                    ModelEventType::LlmFirstToken(LLMFirstToken { .. })
                )),
                2 => assert!(matches!(
                    event.unwrap().event,
                    ModelEventType::LlmContent(LLMContentEvent { .. })
                )),
                3 => assert!(matches!(
                    event.unwrap().event,
                    ModelEventType::LlmContent(LLMContentEvent { .. })
                )),
                4 => assert!(matches!(
                    event.unwrap().event,
                    ModelEventType::LlmStop(LLMFinishEvent { .. })
                )),
                _ => panic!("Unexpected event: {:?}", event),
            }
            index += 1;
        }

        drop(server);
    }

    #[tokio::test]
    async fn test_full_stream_response() {
        let full_events = vec![
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"role":"assistant","content":"","refusal":null},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"UrWxwbIG"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":"1"},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"Kpm8omvXB"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":","},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"HAjYtiWhe"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":" "},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"o9rUpaADh"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":"2"},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"IfuuGcDFo"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":","},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"HF62XYh2s"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":" "},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"IsVZP6OlJ"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":"3"},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"4jKTiIO0f"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":","},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"Fv8DQ9p8n"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":" "},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"n6gzpfrwP"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":"4"},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"EPqVtBGbK"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":","},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"W2BvNvF9B"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":" "},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"5zJpxm411"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":"5"},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"Rx3LyH4eA"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":","},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"UhmQ5ihpa"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":" "},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"572keklmp"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":"6"},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"iO7FCu6k4"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":","},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"jO2XoB91u"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":" "},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"znTy9g65Q"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":"7"},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"AaCu0Oytg"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":","},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"oBt7KU0h9"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":" "},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"Rhlwkyv6j"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":"8"},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"LdmUtSFMy"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":","},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"SRhaiALc6"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":" "},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"963hPzLZg"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":"9"},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"jNXCllQFW"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":","},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"TdsZ5Llz6"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":" "},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"dPjdjeoAt"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{"content":"10"},"logprobs":null,"finish_reason":null}],"usage":null,"obfuscation":"veOOsAWI"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[{"index":0,"delta":{},"logprobs":null,"finish_reason":"stop"}],"usage":null,"obfuscation":"gDye"}"#.to_string(),
            r#"{"id":"chatcmpl-CfmyXmcrxfYmjtAtj7KUUoqPBFb0H","object":"chat.completion.chunk","created":1764075745,"model":"gpt-4.1-mini-2025-04-14","service_tier":"default","system_fingerprint":"fp_24710c7f06","choices":[],"usage":{"prompt_tokens":17,"completion_tokens":28,"total_tokens":45,"prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0},"completion_tokens_details":{"reasoning_tokens":0,"audio_tokens":0,"accepted_prediction_tokens":0,"rejected_prediction_tokens":0}},"obfuscation":"V7Hj72nHk"}"#.to_string(),
        ];

        // Start the mock server
        let server = MockStreamServer::start()
            .await
            .expect("Failed to start mock server");
        let server_url = server.url();

        // Set some test events (you can modify these later)
        server.set_events(full_events.clone()).await;

        let instance = get_instance(&server_url);

        let (tx, _rx) = tokio::sync::mpsc::channel(100);
        let mut stream = instance
            .stream(HashMap::new(), tx, vec![], HashMap::new())
            .await
            .expect("Failed to stream");

        let mut index = 0;
        while let Some(Ok(event)) = stream.next().await {
            let expected_event = full_events[index].clone();
            let expected_event_struct: CreateChatCompletionStreamResponse =
                serde_json::from_str(&expected_event).unwrap();

            if let Some(choice) = expected_event_struct.choices.first() {
                assert_eq!(event.choices[0].delta.content, choice.delta.content);
            } else {
                assert_eq!(event.choices.len(), 0);
            }
            index += 1;
        }
    }
}
