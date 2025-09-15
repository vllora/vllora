use super::error::{AuthorizationError, ModelError};
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
use crate::model::error::ModelFinishError;
use crate::model::handler::handle_tool_call;
use crate::model::types::LLMFirstToken;
use crate::model::{async_trait, DEFAULT_MAX_RETRIES};
use crate::types::credentials::ApiKeyCredentials;
use crate::types::engine::{ExecutionOptions, OpenAiModelParams, Prompt};
use crate::types::gateway::{ChatCompletionContent, ChatCompletionMessage, ToolCall};
use crate::types::gateway::{ChatCompletionMessageWithFinishReason, CompletionModelUsage};
use crate::types::message::{MessageType, PromptMessage};
use crate::types::threads::{InnerMessage, Message};
use crate::{create_model_span, GatewayResult};
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
    Finish(ChatCompletionMessageWithFinishReason),
    NextCall(Vec<ChatCompletionRequestMessage>),
}

fn custom_err(e: impl ToString) -> ModelError {
    ModelError::CustomError(e.to_string())
}

/// Helper function to determine if an endpoint is for Azure OpenAI
pub fn is_azure_endpoint(endpoint: &str) -> bool {
    endpoint.contains("azure.com")
}

/// Create an OpenAI client with standard OpenAI configuration
/// Note: This does not handle Azure OpenAI endpoints. Use azure_openai_client for Azure endpoints.
pub fn openai_client(
    credentials: Option<&ApiKeyCredentials>,
    endpoint: Option<&str>,
) -> Result<Client<OpenAIConfig>, ModelError> {
    let api_key = if let Some(credentials) = credentials {
        credentials.api_key.clone()
    } else {
        std::env::var("LANGDB_OPENAI_API_KEY").map_err(|_| AuthorizationError::InvalidApiKey)?
    };

    let mut config = OpenAIConfig::new();
    config = config.with_api_key(api_key);

    if let Some(endpoint) = endpoint {
        // Do not handle Azure endpoints here
        if is_azure_endpoint(endpoint) {
            return Err(ModelError::CustomError(format!(
                "Azure endpoints should be handled by azure_openai_client, not openai_client: {endpoint}"
            )));
        }

        // For custom non-Azure endpoints
        config = config.with_api_base(endpoint);
    }

    Ok(Client::with_config(config))
}

/// Create an Azure OpenAI client from endpoint URL
pub fn azure_openai_client(
    api_key: String,
    endpoint: &str,
    deployment_id: &str,
) -> Client<AzureConfig> {
    let azure_config = AzureConfig::new()
        .with_api_base(endpoint)
        .with_api_version("2024-10-21".to_string())
        .with_api_key(api_key)
        .with_deployment_id(deployment_id.to_string());

    Client::with_config(azure_config)
}

#[derive(Clone)]
pub struct OpenAIModel<C: Config = OpenAIConfig> {
    params: OpenAiModelParams,
    execution_options: ExecutionOptions,
    prompt: Prompt,
    client: Client<C>,
    tools: Arc<HashMap<String, Box<dyn Tool>>>,
    credentials_ident: CredentialsIdent,
}

// Specific implementation for OpenAIConfig
impl OpenAIModel<OpenAIConfig> {
    pub fn new(
        params: OpenAiModelParams,
        credentials: Option<&ApiKeyCredentials>,
        execution_options: ExecutionOptions,
        prompt: Prompt,
        tools: HashMap<String, Box<dyn Tool>>,
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
            prompt,
            client,
            tools: Arc::new(tools),
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Langdb),
        })
    }
}

// Specific implementation for AzureConfig
impl OpenAIModel<AzureConfig> {
    pub fn new_azure(
        params: OpenAiModelParams,
        credentials: Option<&ApiKeyCredentials>,
        execution_options: ExecutionOptions,
        prompt: Prompt,
        tools: HashMap<String, Box<dyn Tool>>,
        client: Option<Client<AzureConfig>>,
        endpoint: Option<&str>,
    ) -> Result<Self, ModelError> {
        let client = if let Some(client) = client {
            client
        } else if let Some(endpoint) = endpoint {
            let api_key = if let Some(credentials) = credentials {
                credentials.api_key.clone()
            } else {
                std::env::var("LANGDB_OPENAI_API_KEY")
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
            prompt,
            client,
            tools: Arc::new(tools),
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Langdb),
        })
    }

    // Helper to create from a URL
    pub fn from_azure_url(
        params: OpenAiModelParams,
        credentials: Option<&ApiKeyCredentials>,
        execution_options: ExecutionOptions,
        prompt: Prompt,
        tools: HashMap<String, Box<dyn Tool>>,
        endpoint: &str,
    ) -> Result<Self, ModelError> {
        Self::new_azure(
            params,
            credentials,
            execution_options,
            prompt,
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
        }
    }

    async fn handle_tool_calls(
        function_calls: impl Iterator<Item = &ChatCompletionMessageToolCall>,
        tools: &HashMap<String, Box<dyn Tool>>,
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

        Ok(builder.build().map_err(custom_err)?)
    }

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
        first_response_received: &mut bool,
    ) -> GatewayResult<(
        FinishReason,
        Vec<ChatCompletionMessageToolCall>,
        Option<async_openai::types::CompletionUsage>,
        Option<CreateChatCompletionResponse>,
    )> {
        let mut tool_call_states: HashMap<u32, ChatCompletionMessageToolCall> = HashMap::new();
        let mut first_chunk = None;
        let mut stream_content = String::new();
        let mut finish_reason = None;
        let mut usage = None;

        while let Some(result) = stream.next().await {
            match result {
                Ok(mut response) => {
                    if !*first_response_received {
                        *first_response_received = true;
                        tx.send(Some(ModelEvent::new(
                            &Span::current(),
                            ModelEventType::LlmFirstToken(LLMFirstToken {}),
                        )))
                        .await
                        .map_err(|e| GatewayError::CustomError(e.to_string()))?;
                        first_chunk = Some(response.clone());
                    }

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
                        stream_content.push_str(content);
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
                    return Err(ModelError::OpenAIApi(err).into());
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

    async fn execute_inner(
        &self,
        span: Span,
        messages: Vec<ChatCompletionRequestMessage>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<InnerExecutionResult> {
        let call = self.build_request(&messages, false)?;
        span.record("request", serde_json::to_string(&call)?);

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
                    "raw_usage",
                    JsonValue(&serde_json::to_value(usage).unwrap()).as_value(),
                );
                span.record(
                    "usage",
                    JsonValue(&serde_json::to_value(Self::map_usage(Some(usage))).unwrap())
                        .as_value(),
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

                let label = map_tool_names_to_labels(&tool_calls);
                let tools_span = tracing::info_span!(
                    target: target!(),
                    parent: span.clone(),
                    events::SPAN_TOOLS,
                    tool_calls=JsonValue(&serde_json::to_value(&tool_calls)?).as_value(),
                    label=label
                );
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
                            finish_reason: finish_reason.clone(),
                            tool_calls: tool_calls.iter().map(Self::map_tool_call).collect(),
                            credentials_ident: self.credentials_ident.clone(),
                        }),
                    )))
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))?;

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
                                        })
                                        .collect(),
                                ),
                                ..Default::default()
                            },
                            finish_reason,
                        ),
                    ))
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
                    tx.send(Some(ModelEvent::new(
                        &span,
                        ModelEventType::LlmStop(LLMFinishEvent {
                            provider_name: SPAN_OPENAI.to_string(),
                            model_name: self.params.model.clone().unwrap_or_default(),
                            output: Some(content.clone()),
                            usage: Self::map_usage(response.usage.as_ref()),
                            finish_reason: finish_reason.clone(),
                            tool_calls: vec![],
                            credentials_ident: self.credentials_ident.clone(),
                        }),
                    )))
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))?;

                    Ok(InnerExecutionResult::Finish(
                        ChatCompletionMessageWithFinishReason::new(
                            ChatCompletionMessage {
                                role: "assistant".to_string(),
                                content: Some(ChatCompletionContent::Text(content.to_string())),
                                ..Default::default()
                            },
                            finish_reason,
                        ),
                    ))
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
    ) -> GatewayResult<ChatCompletionMessageWithFinishReason> {
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
                Ok(InnerExecutionResult::Finish(message)) => return Ok(message),
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

    fn handle_finish_reason(finish_reason: Option<FinishReason>) -> GatewayError {
        match finish_reason {
            Some(FinishReason::ContentFilter) => {
                ModelError::FinishError(ModelFinishError::ContentFilter).into()
            }
            x => ModelError::FinishError(ModelFinishError::Custom(format!("{x:?}"))).into(),
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
            output_tokens: u.completion_tokens
                + u.completion_tokens_details
                    .as_ref()
                    .and_then(|c| c.reasoning_tokens)
                    .unwrap_or(0),
            total_tokens: u.total_tokens,
            prompt_tokens_details: u.prompt_tokens_details.as_ref().map(|p| {
                crate::types::gateway::PromptTokensDetails::new(
                    p.cached_tokens,
                    Some(0),
                    p.audio_tokens,
                )
            }),
            completion_tokens_details: u.completion_tokens_details.as_ref().map(|c| {
                crate::types::gateway::CompletionTokensDetails::new(
                    c.accepted_prediction_tokens,
                    c.audio_tokens,
                    c.reasoning_tokens,
                    c.rejected_prediction_tokens,
                )
            }),
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
        span.record("request", serde_json::to_string(&request)?);

        let stream = self
            .client
            .chat()
            .create_stream(request)
            .await
            .map_err(ModelError::OpenAIApi)?;
        let (finish_reason, tool_calls, usage, response) = self
            .process_stream(stream, tx, first_response_received)
            .instrument(span.clone())
            .await?;

        span.record("output", serde_json::to_string(&response)?);
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
                "raw_usage",
                JsonValue(&serde_json::to_value(usage.clone()).unwrap()).as_value(),
            );
            let usage = Self::map_usage(Some(&usage));
            span.record(
                "usage",
                JsonValue(&serde_json::to_value(usage).unwrap()).as_value(),
            );
        }

        match finish_reason {
            FinishReason::Stop | FinishReason::Length => Ok(InnerExecutionResult::Finish(
                ChatCompletionMessageWithFinishReason::new(
                    ChatCompletionMessage {
                        ..Default::default()
                    },
                    Self::map_finish_reason(&finish_reason),
                ),
            )),
            FinishReason::ToolCalls => {
                let tool = self
                    .tools
                    .get(tool_calls[0].function.name.as_str())
                    .unwrap();

                let label = map_tool_names_to_labels(&tool_calls);
                let tools_span = tracing::info_span!(
                    target: target!(),
                    parent: span.clone(),
                    events::SPAN_TOOLS,
                    tool_calls=JsonValue(&serde_json::to_value(&tool_calls)?).as_value(),
                    tool_results=field::Empty,
                    label=label
                );
                tools_span.follows_from(span.id());

                if tool.stop_at_call() {
                    Ok(InnerExecutionResult::Finish(
                        ChatCompletionMessageWithFinishReason::new(
                            ChatCompletionMessage {
                                ..Default::default()
                            },
                            Self::map_finish_reason(&finish_reason),
                        ),
                    ))
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
        tags: HashMap<String, String>,
    ) -> GatewayResult<()> {
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

            let first_response_received = &mut false;

            match self
                .execute_stream_inner(
                    span.clone(),
                    input_messages.clone(),
                    tx,
                    tags.clone(),
                    first_response_received,
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
                        msg_args.content(Prompt::render(
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
impl<C: Config + std::marker::Sync + std::marker::Send> ModelInstance for OpenAIModel<C> {
    async fn invoke(
        &self,
        input_variables: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessageWithFinishReason> {
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

impl<C: Config> OpenAIModel<C> {
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
        let previous_messages =
            Self::map_previous_messages(previous_messages, input_variables.clone())?;
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
            let raw_message = Prompt::render(prompt.msg, variables);
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
                InnerMessage::Text(Prompt::render(msg, variables))
            };
            construct_user_message(&inner_message, variables.clone())
        }
        MessageType::SystemMessage => {
            let raw_message = Prompt::render(prompt.msg, variables);
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

fn construct_user_message(
    m: &InnerMessage,
    variables: HashMap<String, Value>,
) -> ChatCompletionRequestMessage {
    let content = match m {
        crate::types::threads::InnerMessage::Text(text) => {
            ChatCompletionRequestUserMessageContent::Text(Prompt::render(
                text.clone(),
                &variables.clone(),
            ))
        }
        crate::types::threads::InnerMessage::Array(content_array) => {
            let mut messages = vec![];
            for m in content_array {
                let msg = match m.r#type {
                    crate::types::threads::MessageContentType::Text => {
                        ChatCompletionRequestUserMessageContentPart::Text(
                            Prompt::render(m.value.clone(), &variables).into(),
                        )
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

fn map_tool_names_to_labels(tool_calls: &[ChatCompletionMessageToolCall]) -> String {
    tool_calls
        .iter()
        .map(|tool_call| tool_call.function.name.clone())
        .collect::<HashSet<String>>()
        .into_iter()
        .collect::<Vec<String>>()
        .join(",")
}
