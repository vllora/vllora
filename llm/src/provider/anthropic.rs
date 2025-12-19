use crate::client::completions::response_stream::ResultStream;
use crate::client::error::AnthropicError;
use crate::client::error::AuthorizationError;
use crate::client::error::ModelError;
use crate::client::tools::handler::handle_tool_call;
use crate::client::DEFAULT_MAX_RETRIES;
use crate::error::{LLMError, LLMResult, ModelFinishError};
use crate::types::credentials::ApiKeyCredentials;
use crate::types::credentials_ident::CredentialsIdent;
use crate::types::engine::{render, AnthropicModelParams, ExecutionOptions};
use crate::types::gateway::ChatCompletionChunk;
use crate::types::gateway::ChatCompletionChunkChoice;
use crate::types::gateway::ChatCompletionDelta;
use crate::types::gateway::{
    ChatCompletionContent, ChatCompletionMessage, ChatCompletionMessageWithFinishReason,
    FunctionCall, ToolCall,
};
use crate::types::gateway::{GatewayModelUsage, PromptTokensDetails};
use crate::types::instance::ModelInstance;
use crate::types::message::InnerMessage;
use crate::types::message::Message;
use crate::types::message::{MessageContentType, MessageType};
use crate::types::tools::Tool;
use crate::types::{
    LLMContentEvent, LLMFinishEvent, LLMFirstToken, LLMStartEvent, ModelEvent, ModelEventType,
    ModelFinishReason, ModelToolCall, ToolStartEvent,
};
use async_trait::async_trait;
use clust::messages::MessagesResponseBody;
use clust::messages::{
    Content, ContentBlock, ImageContentBlock, ImageContentSource, Message as ClustMessage,
    MessageChunk, MessagesRequestBody, MessagesRequestBuilder, StopReason, StreamError,
    StreamOption, SystemPrompt, TextContentBlock, ToolDefinition, ToolResult,
    ToolResultContentBlock, ToolUse, ToolUseContentBlock, Usage,
};
use clust::Client;
use futures::Stream;
use futures::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use tracing::field;
use tracing::Instrument;
use tracing::Span;
use valuable::Valuable;
use vllora_telemetry::create_model_span;
use vllora_telemetry::events::{JsonValue, RecordResult, SPAN_ANTHROPIC, SPAN_TOOLS};

macro_rules! target {
    () => {
        "vllora::user_tracing::models::anthropic"
    };
    ($subtgt:literal) => {
        concat!("vllora::user_tracing::models::anthropic::", $subtgt)
    };
}

enum InnerExecutionResult {
    Finish(Box<ChatCompletionMessageWithFinishReason>),
    NextCall((Option<SystemPrompt>, Vec<ClustMessage>)),
}

fn custom_err(e: impl ToString) -> ModelError {
    ModelError::CustomError(e.to_string())
}

pub fn anthropic_client(
    credentials: Option<&ApiKeyCredentials>,
) -> Result<clust::Client, ModelError> {
    let api_key = if let Some(credentials) = credentials {
        credentials.api_key.clone()
    } else {
        std::env::var("VLLORA_ANTHROPIC_API_KEY").map_err(|_| AuthorizationError::InvalidApiKey)?
    };
    let client = Client::from_api_key(clust::ApiKey::new(api_key));
    Ok(client)
}

fn tool_definition(tool: &dyn Tool) -> clust::messages::ToolDefinition {
    let name = tool.name();
    let description = Some(tool.description());
    let input_schema = tool
        .get_function_parameters()
        .and_then(|a| serde_json::to_value(a).ok())
        .unwrap_or(serde_json::json!({}));
    clust::messages::ToolDefinition {
        name,
        description,
        input_schema,
    }
}

#[derive(Clone)]
pub struct AnthropicModel {
    params: AnthropicModelParams,
    execution_options: ExecutionOptions,
    client: Client,
    tools: HashMap<String, Arc<Box<dyn Tool>>>,
    credentials_ident: CredentialsIdent,
    endpoint: Option<String>,
}

impl AnthropicModel {
    pub fn new(
        params: AnthropicModelParams,
        execution_options: ExecutionOptions,
        credentials: Option<&ApiKeyCredentials>,
        tools: HashMap<String, Arc<Box<dyn Tool>>>,
        endpoint: Option<String>,
    ) -> Result<Self, ModelError> {
        let client: Client = anthropic_client(credentials)?;
        Ok(Self {
            params,
            execution_options,
            client,
            tools,
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Vllora),
            endpoint,
        })
    }

    async fn handle_tool_calls(
        function_calls: impl Iterator<Item = &ToolUse>,
        tools: &HashMap<String, Arc<Box<dyn Tool>>>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> Vec<ClustMessage> {
        futures::future::join_all(function_calls.map(|tool_use| {
            let tags_value = tags.clone();
            async move {
                let tool_call = Self::map_tool_call(tool_use);
                let tool_call = tool_call.map_err(|e| LLMError::CustomError(e.to_string()));
                let result = match tool_call {
                    Ok(tool_call) => {
                        let result =
                            handle_tool_call(&tool_call, tools, tx, tags_value.clone()).await;
                        match result {
                            Ok(content) => ToolResult::success(tool_use.id.clone(), Some(content)),
                            Err(e) => ToolResult::error(tool_use.id.clone(), Some(e.to_string())),
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error calling tool ({}): {}", tool_use.id, e);
                        ToolResult::error(tool_use.id.clone(), Some(e.to_string()))
                    }
                };

                ClustMessage::user(result)
            }
        }))
        .await
    }

    fn build_request(
        &self,
        system_message: Option<&SystemPrompt>,
        messages: Vec<ClustMessage>,
        stream: bool,
    ) -> Result<MessagesRequestBody, AnthropicError> {
        let model = self.params.model.as_ref().unwrap();
        let builder = MessagesRequestBuilder::new(model.model.clone());
        let model_params = &self.params;

        let builder = if let Some(system_message) = system_message {
            builder.system(system_message.clone())
        } else {
            builder
        };

        let builder = if let Some(max_tokens) = model_params.max_tokens {
            builder.max_tokens(max_tokens)
        } else {
            builder
        };
        let builder = if let Some(temperature) = model_params.temperature {
            builder.temperature(temperature)
        } else {
            builder
        };

        let builder = if let Some(top_k) = model_params.top_k {
            builder.top_k(top_k)
        } else {
            builder
        };

        let builder = if let Some(top_p) = model_params.top_p {
            builder.top_p(top_p)
        } else {
            builder
        };

        let builder = if let Some(stop) = &model_params.stop_sequences {
            builder.stop_sequences(stop.clone())
        } else {
            builder
        };

        let builder = if let Some(thinking) = &model_params.thinking {
            builder.thinking(thinking.clone())
        } else {
            builder
        };

        let builder = builder.messages(messages.clone());

        let builder = match stream {
            true => builder.stream(StreamOption::ReturnStream),
            false => builder.stream(StreamOption::ReturnOnce),
        };
        let builder = if !self.tools.is_empty() {
            let mut tools: Vec<ToolDefinition> = vec![];
            for (_, tool) in self.tools.clone().iter() {
                tools.push(tool_definition(tool.deref().as_ref()));
            }

            builder.tools(tools)
        } else {
            builder
        };

        Ok(builder.build())
    }

    fn handle_max_tokens_error() -> LLMError {
        LLMError::FinishError(ModelFinishError::MaxTokens)
    }

    fn build_response(
        &self,
        id: String,
        tool_call_states: &[ToolUse],
        usage: Usage,
        stream_content: String,
        stop_reason: &StopReason,
    ) -> MessagesResponseBody {
        let content = if tool_call_states.is_empty() {
            Content::SingleText(stream_content)
        } else {
            Content::MultipleBlocks(
                tool_call_states
                    .iter()
                    .map(|t| ContentBlock::ToolUse(ToolUseContentBlock::new(t.clone())))
                    .collect(),
            )
        };

        MessagesResponseBody {
            id,
            content,
            model: self.params.model.clone().expect("model is required").into(),
            role: clust::messages::Role::Assistant,
            stop_reason: Some(*stop_reason),
            stop_sequence: None,
            usage,
            _type: clust::messages::MessageObjectType::Message,
        }
    }

    async fn process_stream(
        &self,
        stream: impl Stream<Item = Result<MessageChunk, StreamError>>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tx_response: &tokio::sync::mpsc::Sender<LLMResult<ChatCompletionChunk>>,
        started_at: std::time::Instant,
    ) -> LLMResult<(StopReason, Vec<ToolUse>, Usage, MessagesResponseBody)> {
        let mut tool_call_states: HashMap<u32, ToolUse> = HashMap::new();
        tokio::pin!(stream);
        let mut json_states: HashMap<u32, String> = HashMap::new();
        let mut usage = Usage {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            cache_creation: None,
        };
        let mut first_response_received = false;
        let mut stream_content = String::new();
        let mut response_id = "".to_string();
        loop {
            let r = stream.next().await.transpose();
            if !first_response_received {
                first_response_received = true;
                let _ = tx
                    .send(Some(ModelEvent::new(
                        &Span::current(),
                        ModelEventType::LlmFirstToken(LLMFirstToken {}),
                    )))
                    .await;
                Span::current().record("ttft", started_at.elapsed().as_micros());
            }

            match r {
                Ok(Some(result)) => {
                    let chunk = ChatCompletionChunk {
                        id: response_id.clone(),
                        object: "chat.completion.chunk".to_string(),
                        created: chrono::Utc::now().timestamp(),
                        model: self
                            .params
                            .model
                            .clone()
                            .expect("model is required")
                            .to_string(),
                        choices: vec![],
                        usage: None,
                    };

                    // let _ = tx_response.send(Ok(Chunk::Anthropic(result.clone()))).await;
                    match result {
                        MessageChunk::ContentBlockStart(block) => match block.content_block {
                            clust::messages::ContentBlockStart::TextContentBlock(block) => {
                                let _ = tx
                                    .send(Some(ModelEvent::new(
                                        &tracing::Span::current(),
                                        ModelEventType::LlmContent(LLMContentEvent {
                                            content: block.text.clone(),
                                        }),
                                    )))
                                    .await;
                                stream_content.push_str(&block.text);

                                let mut chunk_clone = chunk.clone();
                                chunk_clone.choices.push(ChatCompletionChunkChoice {
                                    index: 0,
                                    delta: ChatCompletionDelta {
                                        role: Some("assistant".to_string()),
                                        content: Some(block.text.clone()),
                                        tool_calls: None,
                                    },
                                    finish_reason: None,
                                    logprobs: None,
                                });
                                let _ = tx_response.send(Ok(chunk_clone)).await;
                            }
                            clust::messages::ContentBlockStart::ThinkingContentBlock(thinking) => {
                                let _ = tx
                                    .send(Some(ModelEvent::new(
                                        &tracing::Span::current(),
                                        ModelEventType::LlmContent(LLMContentEvent {
                                            content: format!("thinking: {}", thinking.thinking),
                                        }),
                                    )))
                                    .await;

                                let mut chunk_clone = chunk.clone();
                                chunk_clone.choices.push(ChatCompletionChunkChoice {
                                    index: 0,
                                    delta: ChatCompletionDelta {
                                        role: Some("assistant".to_string()),
                                        content: Some(format!("thinking: {}", thinking.thinking)),
                                        tool_calls: None,
                                    },
                                    finish_reason: None,
                                    logprobs: None,
                                });
                                let _ = tx_response.send(Ok(chunk_clone)).await;
                            }
                            clust::messages::ContentBlockStart::ToolUseContentBlock(
                                tool_use_block,
                            ) => {
                                tool_call_states.insert(block.index, tool_use_block.tool_use);
                                json_states.insert(block.index, String::new());
                            }
                        },
                        MessageChunk::ContentBlockDelta(block) => match block.delta {
                            clust::messages::ContentBlockDelta::TextDeltaContentBlock(delta) => {
                                let _ = tx
                                    .send(Some(ModelEvent::new(
                                        &tracing::Span::current(),
                                        ModelEventType::LlmContent(LLMContentEvent {
                                            content: delta.text.clone(),
                                        }),
                                    )))
                                    .await;
                                stream_content.push_str(&delta.text);

                                let mut chunk_clone = chunk.clone();
                                chunk_clone.choices.push(ChatCompletionChunkChoice {
                                    index: 0,
                                    delta: ChatCompletionDelta {
                                        role: Some("assistant".to_string()),
                                        content: Some(delta.text.clone()),
                                        tool_calls: None,
                                    },
                                    finish_reason: None,
                                    logprobs: None,
                                });
                                let _ = tx_response.send(Ok(chunk_clone)).await;
                            }
                            clust::messages::ContentBlockDelta::ThinkingDeltaContentBlock(
                                delta,
                            ) => {
                                let _ = tx
                                    .send(Some(ModelEvent::new(
                                        &tracing::Span::current(),
                                        ModelEventType::LlmContent(LLMContentEvent {
                                            content: delta.thinking.clone(),
                                        }),
                                    )))
                                    .await;
                                stream_content.push_str(&delta.thinking);

                                let mut chunk_clone = chunk.clone();
                                chunk_clone.choices.push(ChatCompletionChunkChoice {
                                    index: 0,
                                    delta: ChatCompletionDelta {
                                        role: Some("assistant".to_string()),
                                        content: Some(delta.thinking.clone()),
                                        tool_calls: None,
                                    },
                                    finish_reason: None,
                                    logprobs: None,
                                });
                                let _ = tx_response.send(Ok(chunk_clone)).await;
                            }
                            clust::messages::ContentBlockDelta::SignatureDeltaContentBlock(_) => {}
                            clust::messages::ContentBlockDelta::InputJsonDeltaBlock(
                                input_json_block,
                            ) => {
                                json_states
                                    .entry(block.index)
                                    .and_modify(|v| {
                                        v.push_str(&input_json_block.partial_json);
                                    })
                                    .or_default();
                            }
                        },
                        MessageChunk::MessageStart(start) => {
                            response_id = start.message.id;
                            usage.input_tokens = start.message.usage.input_tokens;
                            usage.cache_read_input_tokens =
                                start.message.usage.cache_read_input_tokens;
                            usage.cache_creation_input_tokens =
                                start.message.usage.cache_creation_input_tokens;
                            usage.cache_creation = start.message.usage.cache_creation;
                        }

                        MessageChunk::Ping(_) => {}
                        MessageChunk::ContentBlockStop(stop_block) => {
                            let json = json_states.get(&stop_block.index);
                            if let Some(json) = json {
                                let input: Value =
                                    serde_json::from_str(json).unwrap_or(serde_json::json!({}));
                                tool_call_states.entry(stop_block.index).and_modify(|t| {
                                    t.input = input;
                                });
                            }

                            if let Some(tool_call) = tool_call_states.get(&stop_block.index) {
                                let mut chunk_clone = chunk.clone();
                                chunk_clone.choices.push(ChatCompletionChunkChoice {
                                    index: 0,
                                    delta: ChatCompletionDelta {
                                        role: Some("assistant".to_string()),
                                        content: None,
                                        tool_calls: Some(vec![ToolCall {
                                            index: Some(stop_block.index as usize),
                                            id: tool_call.id.clone(),
                                            r#type: "function".to_string(),
                                            function: FunctionCall {
                                                name: tool_call.name.clone(),
                                                arguments: serde_json::to_string(&tool_call.input)
                                                    .unwrap(),
                                            },
                                            extra_content: None,
                                        }]),
                                    },
                                    finish_reason: None,
                                    logprobs: None,
                                });
                                let _ = tx_response.send(Ok(chunk_clone)).await;
                            }
                        }
                        MessageChunk::MessageDelta(delta) => {
                            usage.output_tokens = delta.usage.output_tokens;

                            if let Some(stop_reason) = delta.delta.stop_reason {
                                let response = self.build_response(
                                    response_id.clone(),
                                    &tool_call_states.values().cloned().collect::<Vec<_>>(),
                                    usage,
                                    stream_content,
                                    &stop_reason,
                                );
                                return Ok((
                                    stop_reason,
                                    tool_call_states.values().cloned().collect(),
                                    usage,
                                    response,
                                ));
                            }
                        }
                        MessageChunk::MessageStop(s) => {
                            tracing::error!("Stream ended with error: {:#?}", s);
                        }
                    }
                }
                last_result => {
                    tracing::error!("Error in stream: {last_result:?}");
                    break;
                }
            }
        }

        unreachable!();
    }

    async fn execute_inner(
        &self,
        span: Span,
        request: MessagesRequestBody,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> LLMResult<InnerExecutionResult> {
        let system_message = request.system.clone();
        let input_messages = request.messages.clone();

        let _ = tx
            .send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStart(LLMStartEvent {
                    provider_name: SPAN_ANTHROPIC.to_string(),
                    model_name: self
                        .params
                        .model
                        .clone()
                        .map(|m| m.to_string())
                        .unwrap_or_default(),
                    input: serde_json::to_string(&input_messages)?,
                }),
            )))
            .await;

        let response = async move {
            let result = self
                .client
                .create_a_message(request, self.endpoint.clone())
                .await;
            let _ = result
                .as_ref()
                .map(|response| serde_json::to_value(response).unwrap())
                .as_ref()
                .map(JsonValue)
                .record();
            let response = result.map_err(custom_err)?;

            let span = Span::current();
            span.record("output", serde_json::to_string(&response)?);

            span.record(
                "raw_usage",
                JsonValue(&serde_json::to_value(response.usage).unwrap()).as_value(),
            );
            let usage = Self::map_usage(&response.usage);
            span.record(
                "usage",
                JsonValue(&serde_json::to_value(usage).unwrap()).as_value(),
            );

            Ok::<_, LLMError>(response)
        }
        .instrument(span.clone().or_current())
        .await?;

        // Alwayss present in non streamin mode
        let stop_reason = response.stop_reason.unwrap();

        match stop_reason {
            clust::messages::StopReason::EndTurn | clust::messages::StopReason::StopSequence => {
                let message_content = response.content;

                let prompt_tokens_details = PromptTokensDetails::new(
                    response.usage.cache_read_input_tokens,
                    response.usage.cache_creation_input_tokens,
                    None,
                );
                let input_tokens = response.usage.input_tokens
                    + response.usage.cache_read_input_tokens.unwrap_or(0)
                    + response.usage.cache_creation_input_tokens.unwrap_or(0);
                let usage = GatewayModelUsage {
                    input_tokens,
                    output_tokens: response.usage.output_tokens,
                    total_tokens: input_tokens + response.usage.output_tokens,
                    prompt_tokens_details: Some(prompt_tokens_details),
                    ..Default::default()
                };

                match message_content {
                    Content::SingleText(content) => {
                        let _ = tx
                            .send(Some(ModelEvent::new(
                                &span,
                                ModelEventType::LlmStop(LLMFinishEvent {
                                    provider_name: SPAN_ANTHROPIC.to_string(),
                                    model_name: self
                                        .params
                                        .model
                                        .clone()
                                        .map(|m| m.to_string())
                                        .unwrap_or_default(),
                                    output: Some(content.clone()),
                                    usage: Some(usage.clone()),
                                    finish_reason: ModelFinishReason::Stop,
                                    tool_calls: vec![],
                                    credentials_ident: self.credentials_ident.clone(),
                                }),
                            )))
                            .await;

                        Ok(InnerExecutionResult::Finish(
                            ChatCompletionMessageWithFinishReason::new(
                                ChatCompletionMessage {
                                    content: Some(ChatCompletionContent::Text(content.to_owned())),
                                    role: "assistant".to_string(),
                                    ..Default::default()
                                },
                                ModelFinishReason::Stop,
                                response.id.clone(),
                                chrono::Utc::now().timestamp() as u32,
                                response.model.to_string(),
                                Some(usage),
                            )
                            .into(),
                        ))
                    }
                    Content::MultipleBlocks(blocks) => {
                        let mut final_text = String::new();
                        for b in blocks.iter() {
                            match b {
                                ContentBlock::Text(text) => {
                                    final_text.push_str(&text.text);
                                }
                                ContentBlock::Thinking(thinking) => {
                                    final_text
                                        .push_str(&format!("thinking: {}\n\n", thinking.thinking));
                                }
                                _ => {
                                    return Err(ModelError::CustomError(
                                        "unexpected content block".to_string(),
                                    )
                                    .into());
                                }
                            }
                        }

                        let _ = tx
                            .send(Some(ModelEvent::new(
                                &span,
                                ModelEventType::LlmStop(LLMFinishEvent {
                                    provider_name: SPAN_ANTHROPIC.to_string(),
                                    model_name: self
                                        .params
                                        .model
                                        .clone()
                                        .map(|m| m.to_string())
                                        .unwrap_or_default(),
                                    output: Some(final_text.clone()),
                                    usage: Some(usage.clone()),
                                    finish_reason: ModelFinishReason::Stop,
                                    tool_calls: vec![],
                                    credentials_ident: self.credentials_ident.clone(),
                                }),
                            )))
                            .await;

                        Ok(InnerExecutionResult::Finish(
                            ChatCompletionMessageWithFinishReason::new(
                                ChatCompletionMessage {
                                    content: Some(ChatCompletionContent::Text(final_text)),
                                    role: "assistant".to_string(),
                                    ..Default::default()
                                },
                                ModelFinishReason::Stop,
                                response.id.clone(),
                                chrono::Utc::now().timestamp() as u32,
                                response.model.to_string(),
                                Some(usage),
                            )
                            .into(),
                        ))
                    }
                }
            }
            clust::messages::StopReason::MaxTokens => Err(Self::handle_max_tokens_error()),
            clust::messages::StopReason::ToolUse => {
                let content = response.content.clone();
                let blocks = if let Content::MultipleBlocks(blocks) = response.content {
                    blocks
                } else {
                    return Err(ModelError::CustomError(
                        "Expected multiple tool blocks".to_string(),
                    )
                    .into());
                };

                let mut messages: Vec<ClustMessage> = vec![ClustMessage::assistant(content)];
                let mut tool_runs = Vec::new();
                let mut text_content = None;
                for b in blocks.iter() {
                    match b {
                        ContentBlock::ToolUse(tool) => {
                            tool_runs.push(tool.tool_use.clone());
                        }
                        ContentBlock::Text(t) => {
                            // Ignore text for now
                            // messages.push(ClustMessage::assistant(t.text.clone()))
                            text_content = Some(t.text.clone());
                        }
                        block => {
                            tracing::error!("Unexpected content block in response: {}", block);
                            tracing::error!("All blocks {:?}", blocks);
                            return Err(ModelError::CustomError(
                                "Unexpected content block in response".to_string(),
                            )
                            .into());
                        }
                    }
                }

                let tool_calls_str = serde_json::to_string(&tool_runs)?;
                let tools_span = tracing::info_span!(
                    target: target!(),
                    SPAN_TOOLS,
                    tool_calls=tool_calls_str,
                    tool.name=tool_runs.iter().map(|t| t.name.clone()).collect::<Vec<String>>().join(",")
                );
                tools_span.follows_from(span.id());

                let tool = self.tools.get(&tool_runs[0].name).unwrap();
                if tool.stop_at_call() {
                    let usage = Some(GatewayModelUsage {
                        input_tokens: response.usage.input_tokens,
                        output_tokens: response.usage.output_tokens,
                        total_tokens: response.usage.input_tokens + response.usage.output_tokens,
                        ..Default::default()
                    });
                    let _ = tx
                        .send(Some(ModelEvent::new(
                            &span,
                            ModelEventType::LlmStop(LLMFinishEvent {
                                provider_name: SPAN_ANTHROPIC.to_string(),
                                model_name: self
                                    .params
                                    .model
                                    .clone()
                                    .map(|m| m.to_string())
                                    .unwrap_or_default(),
                                output: text_content.clone(),
                                usage: usage.clone(),
                                finish_reason: ModelFinishReason::ToolCalls,
                                tool_calls: tool_runs
                                    .iter()
                                    .map(|tool_call| ModelToolCall {
                                        tool_id: tool_call.id.clone(),
                                        tool_name: tool_call.name.clone(),
                                        input: serde_json::to_string(&tool_call.input).unwrap(),
                                        extra_content: None,
                                    })
                                    .collect(),
                                credentials_ident: self.credentials_ident.clone(),
                            }),
                        )))
                        .await;

                    Ok(InnerExecutionResult::Finish(
                        ChatCompletionMessageWithFinishReason::new(
                            ChatCompletionMessage {
                                role: "assistant".to_string(),
                                content: text_content.map(ChatCompletionContent::Text),
                                tool_calls: Some(
                                    tool_runs
                                        .iter()
                                        .enumerate()
                                        .map(|(index, tool_call)| {
                                            Ok(ToolCall {
                                                index: Some(index),
                                                id: tool_call.id.clone(),
                                                r#type: "function".to_string(),
                                                function: FunctionCall {
                                                    name: tool_call.name.clone(),
                                                    arguments: serde_json::to_string(
                                                        &tool_call.input,
                                                    )?,
                                                },
                                                extra_content: None,
                                            })
                                        })
                                        .collect::<Result<Vec<ToolCall>, LLMError>>()?,
                                ),
                                ..Default::default()
                            },
                            ModelFinishReason::ToolCalls,
                            response.id.clone(),
                            chrono::Utc::now().timestamp() as u32,
                            response.model.to_string(),
                            usage,
                        )
                        .into(),
                    ))
                } else {
                    let result_tool_calls =
                        Self::handle_tool_calls(tool_runs.iter(), &self.tools, tx, tags.clone())
                            .instrument(tools_span.clone())
                            .await;
                    messages.extend(result_tool_calls);

                    let conversation_messages = [input_messages, messages].concat();
                    Ok(InnerExecutionResult::NextCall((
                        system_message,
                        conversation_messages,
                    )))
                }
            }
        }
    }

    async fn execute(
        &self,
        system_message: Option<SystemPrompt>,
        input_messages: Vec<ClustMessage>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> LLMResult<ChatCompletionMessageWithFinishReason> {
        let mut calls = vec![(system_message, input_messages)];
        let mut retries_left = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some((system_message, input_messages)) = calls.pop() {
            let input = serde_json::to_string(&input_messages)?;
            let call_span = create_model_span!(
                SPAN_ANTHROPIC,
                target!("chat"),
                tags,
                retries_left,
                input = input,
                system_prompt = field::Empty
            );

            let request = self
                .build_request(system_message.as_ref(), input_messages.clone(), false)
                .map_err(custom_err)?;
            call_span.record(
                "request",
                serde_json::to_string(&request).unwrap_or_default(),
            );
            if let Some(system_message) = &system_message {
                call_span.record("system_prompt", format!("{system_message}"));
            }

            match self
                .execute_inner(call_span.clone(), request, tx, tags.clone())
                .await
            {
                Ok(InnerExecutionResult::Finish(message)) => return Ok(message.deref().clone()),
                Ok(InnerExecutionResult::NextCall((system_prompt, messages))) => {
                    calls.push((system_prompt, messages));
                }
                Err(e) => {
                    call_span.record("error", e.to_string());
                    if retries_left == 0 {
                        return Err(e);
                    } else {
                        calls.push((system_message, input_messages));
                    }
                    retries_left -= 1;
                }
            }
        }

        unreachable!();
    }

    async fn execute_stream(
        &self,
        system_message: Option<SystemPrompt>,
        input_messages: Vec<ClustMessage>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tx_response: &tokio::sync::mpsc::Sender<LLMResult<ChatCompletionChunk>>,
        tags: HashMap<String, String>,
    ) -> LLMResult<()> {
        let mut calls = vec![(system_message, input_messages)];
        let mut retries_left = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some((system_message, input_messages)) = calls.pop() {
            let input = serde_json::to_string(&input_messages)?;
            let call_span = create_model_span!(
                SPAN_ANTHROPIC,
                target!("chat"),
                tags,
                retries_left,
                input = input,
                system_prompt = field::Empty
            );

            let request = self
                .build_request(system_message.as_ref(), input_messages.clone(), true)
                .map_err(custom_err)?;
            call_span.record(
                "request",
                serde_json::to_string(&request).unwrap_or_default(),
            );
            if let Some(system_message) = &system_message {
                call_span.record("system_prompt", format!("{system_message}"));
            }

            match self
                .execute_stream_inner(request, call_span.clone(), tx, tx_response, tags.clone())
                .await
            {
                Ok(InnerExecutionResult::Finish(_)) => return Ok(()),
                Ok(InnerExecutionResult::NextCall((system_prompt, messages))) => {
                    calls.push((system_prompt, messages));
                }
                Err(e) => {
                    call_span.record("error", e.to_string());
                    if retries_left == 0 {
                        return Err(e);
                    } else {
                        calls.push((system_message, input_messages));
                    }
                    retries_left -= 1;
                }
            }
        }

        Ok(())
    }

    fn map_usage(usage: &Usage) -> GatewayModelUsage {
        let input_tokens = usage.input_tokens
            + usage.cache_read_input_tokens.unwrap_or(0)
            + usage.cache_creation_input_tokens.unwrap_or(0);
        GatewayModelUsage {
            input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.output_tokens + input_tokens,
            prompt_tokens_details: Some(PromptTokensDetails::new(
                usage.cache_read_input_tokens,
                usage.cache_creation_input_tokens,
                None,
            )),
            ..Default::default()
        }
    }

    fn map_finish_reason(reason: &StopReason) -> ModelFinishReason {
        match reason {
            StopReason::EndTurn => ModelFinishReason::Stop,
            StopReason::StopSequence => ModelFinishReason::StopSequence,
            StopReason::ToolUse => ModelFinishReason::ToolCalls,
            StopReason::MaxTokens => ModelFinishReason::Length,
        }
    }

    fn map_tool_call(t: &ToolUse) -> Result<ModelToolCall, LLMError> {
        Ok(ModelToolCall {
            tool_id: t.id.clone(),
            tool_name: t.name.clone(),
            input: serde_json::to_string(&t.input)?,
            extra_content: None,
        })
    }

    async fn execute_stream_inner(
        &self,
        request: MessagesRequestBody,
        span: Span,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tx_response: &tokio::sync::mpsc::Sender<LLMResult<ChatCompletionChunk>>,
        tags: HashMap<String, String>,
    ) -> LLMResult<InnerExecutionResult> {
        let system_message = request.system.clone();
        let input_messages = request.messages.clone();
        let credentials_ident = self.credentials_ident.clone();

        let _ = tx
            .send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStart(LLMStartEvent {
                    provider_name: SPAN_ANTHROPIC.to_string(),
                    model_name: self
                        .params
                        .model
                        .clone()
                        .map(|m| m.to_string())
                        .unwrap_or_default(),
                    input: serde_json::to_string(&input_messages)?,
                }),
            )))
            .await;

        let started_at = std::time::Instant::now();
        let stream = self
            .client
            .create_a_message_stream(request, self.endpoint.clone())
            .await
            .map_err(custom_err)?;
        let (stop_reason, tool_calls, usage, response) = self
            .process_stream(stream, tx, tx_response, started_at)
            .instrument(span.clone())
            .await?;

        span.record("output", serde_json::to_string(&response)?);
        let trace_finish_reason = Self::map_finish_reason(&stop_reason);

        let chunk = ChatCompletionChunk {
            id: response.id.clone(),
            object: "chat.completion.chunk".to_string(),
            created: chrono::Utc::now().timestamp(),
            model: response.model.to_string(),
            choices: vec![],
            usage: None,
        };

        let mut chunk_clone = chunk.clone();

        chunk_clone.choices.push(ChatCompletionChunkChoice {
            index: 0,
            delta: ChatCompletionDelta::default(),
            finish_reason: Some(trace_finish_reason.to_string()),
            logprobs: None,
        });
        let _ = tx_response.send(Ok(chunk_clone)).await;

        let mut chunk_clone = chunk.clone();
        chunk_clone.usage = Some(usage.into());
        let _ = tx_response.send(Ok(chunk_clone)).await;

        span.record(
            "raw_usage",
            JsonValue(&serde_json::to_value(usage).unwrap()).as_value(),
        );
        let usage = Self::map_usage(&usage);
        span.record(
            "usage",
            JsonValue(&serde_json::to_value(usage.clone())?).as_value(),
        );
        let _ = tx
            .send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStop(LLMFinishEvent {
                    provider_name: SPAN_ANTHROPIC.to_string(),
                    model_name: self
                        .params
                        .model
                        .clone()
                        .map(|m| m.to_string())
                        .unwrap_or_default(),
                    output: None,
                    usage: Some(usage.clone()),
                    finish_reason: trace_finish_reason.clone(),
                    credentials_ident: credentials_ident.clone(),
                    tool_calls: tool_calls
                        .iter()
                        .map(Self::map_tool_call)
                        .collect::<Result<Vec<ModelToolCall>, LLMError>>()?,
                }),
            )))
            .await;

        match stop_reason {
            StopReason::EndTurn | StopReason::StopSequence => Ok(InnerExecutionResult::Finish(
                ChatCompletionMessageWithFinishReason::new(
                    ChatCompletionMessage {
                        ..Default::default()
                    },
                    ModelFinishReason::Stop,
                    response.id.clone(),
                    chrono::Utc::now().timestamp() as u32,
                    response.model.to_string(),
                    Some(usage),
                )
                .into(),
            )),
            StopReason::MaxTokens => Err(Self::handle_max_tokens_error()),
            StopReason::ToolUse => {
                let tool_calls_str = serde_json::to_string(&tool_calls)?;
                let tools_span = tracing::info_span!(
                    target: target!(),
                    SPAN_TOOLS,
                    tool_calls=tool_calls_str,
                    tool.name=tool_calls.iter().map(|t| t.name.clone()).collect::<Vec<String>>().join(",")
                );
                tools_span.follows_from(span.id());
                let tool = self.tools.get(&tool_calls[0].name).unwrap();
                if tool.stop_at_call() {
                    let _ = tx
                        .send(Some(ModelEvent::new(
                            &span,
                            ModelEventType::ToolStart(ToolStartEvent {
                                tool_id: tool_calls[0].id.clone(),
                                tool_name: tool_calls[0].name.clone(),
                                input: serde_json::to_string(&tool_calls[0].input)?,
                            }),
                        )))
                        .await;

                    Ok(InnerExecutionResult::Finish(
                        ChatCompletionMessageWithFinishReason::new(
                            ChatCompletionMessage {
                                ..Default::default()
                            },
                            ModelFinishReason::ToolCalls,
                            response.id.clone(),
                            chrono::Utc::now().timestamp() as u32,
                            response.model.to_string(),
                            Some(usage),
                        )
                        .into(),
                    ))
                } else {
                    let mut messages = vec![ClustMessage::assistant(Content::MultipleBlocks(
                        tool_calls
                            .iter()
                            .map(|t| ContentBlock::ToolUse(ToolUseContentBlock::new(t.clone())))
                            .collect(),
                    ))];
                    let result_tool_calls =
                        Self::handle_tool_calls(tool_calls.iter(), &self.tools, tx, tags.clone())
                            .instrument(tools_span.clone())
                            .await;
                    messages.extend(result_tool_calls);

                    let conversation_messages = [input_messages, messages].concat();

                    Ok(InnerExecutionResult::NextCall((
                        system_message,
                        conversation_messages,
                    )))
                }
            }
        }
    }

    fn map_previous_messages(messages_dto: Vec<Message>) -> LLMResult<Vec<ClustMessage>> {
        // convert serde::Map into HashMap
        let mut messages: Vec<ClustMessage> = vec![];

        let mut tool_results_remaining = 0;
        let mut tool_calls_collected = vec![];

        for m in messages_dto.iter() {
            match m.r#type {
                MessageType::SystemMessage => {}
                MessageType::AIMessage => {
                    if let Some(tool_calls) = &m.tool_calls {
                        tool_results_remaining = tool_calls.len();
                        tool_calls_collected = vec![];

                        messages.push(ClustMessage::assistant(Content::MultipleBlocks(
                            tool_calls
                                .iter()
                                .map(|t| {
                                    Ok(ContentBlock::ToolUse(ToolUseContentBlock::new(
                                        ToolUse::new(
                                            t.id.clone(),
                                            t.function.name.clone(),
                                            serde_json::from_str(&t.function.arguments)?,
                                        ),
                                    )))
                                })
                                .collect::<Result<Vec<ContentBlock>, LLMError>>()?,
                        )));
                    } else {
                        messages.push(ClustMessage::assistant(Content::SingleText(
                            m.content.clone().unwrap_or_default(),
                        )));
                    }
                }
                MessageType::HumanMessage => {
                    messages.push(construct_user_message(&m.clone().into()));
                }
                MessageType::ToolResult => {
                    tool_results_remaining -= 1;
                    tool_calls_collected.push(ContentBlock::ToolResult(
                        ToolResultContentBlock::new(ToolResult::success(
                            m.tool_call_id.as_ref().expect("Missing tool call id"),
                            m.content.clone(),
                        )),
                    ));
                    if tool_results_remaining == 0 {
                        messages.push(ClustMessage::user(Content::MultipleBlocks(
                            tool_calls_collected.clone(),
                        )));
                    }
                }
            }
        }

        Ok(messages)
    }
}

#[async_trait]
impl ModelInstance for AnthropicModel {
    async fn invoke(
        &self,
        input_variables: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> LLMResult<ChatCompletionMessageWithFinishReason> {
        let (system_prompt, conversational_messages) =
            self.construct_messages(input_variables, previous_messages)?;
        self.execute(system_prompt, conversational_messages, &tx, tags)
            .await
    }

    async fn stream(
        &self,
        input_variables: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> LLMResult<ResultStream> {
        let (system_prompt, conversational_messages) =
            self.construct_messages(input_variables, previous_messages)?;
        let (tx_response, rx_response) = tokio::sync::mpsc::channel(10000);
        let model = (*self).clone();
        let tx_clone = tx.clone();
        tokio::spawn(
            async move {
                let result = model
                    .execute_stream(
                        system_prompt,
                        conversational_messages,
                        &tx_clone,
                        &tx_response,
                        tags,
                    )
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

impl AnthropicModel {
    fn construct_messages(
        &self,
        input_variables: HashMap<String, Value>,
        previous_messages: Vec<Message>,
    ) -> LLMResult<(Option<SystemPrompt>, Vec<ClustMessage>)> {
        let mut conversational_messages = vec![];
        let system_message = previous_messages
            .iter()
            .find(|m| m.r#type == MessageType::SystemMessage)
            .map(|message| {
                if let Some(content) = &message.content {
                    SystemPrompt::new(render(content.clone(), &input_variables))
                } else {
                    SystemPrompt::from_content_blocks(
                        message
                            .content_array
                            .iter()
                            .map(|c| match &c.cache_control {
                                Some(cache_control) => {
                                    let cache_control = clust::messages::CacheControl {
                                        _type: clust::messages::CacheControlType::Ephemeral,
                                        ttl: cache_control.ttl().map(|t| t.into()),
                                    };
                                    ContentBlock::Text(TextContentBlock::new_with_cache_control(
                                        render(c.value.clone(), &input_variables),
                                        cache_control,
                                    ))
                                }
                                None => ContentBlock::Text(TextContentBlock::new(c.value.clone())),
                            })
                            .collect(),
                    )
                }
            });

        let previous_messages = Self::map_previous_messages(previous_messages)?;
        conversational_messages.extend(previous_messages);

        Ok((system_message, conversational_messages))
    }
}

fn construct_user_message(m: &InnerMessage) -> ClustMessage {
    let content = match m {
        InnerMessage::Text(text) => Content::SingleText(text.to_owned()),
        InnerMessage::Array(content_array) => {
            let mut blocks = vec![];
            for m in content_array {
                let msg: ContentBlock = match m.r#type {
                    MessageContentType::Text => {
                        if let Some(cache_control) = &m.cache_control {
                            let cache_control = clust::messages::CacheControl {
                                _type: clust::messages::CacheControlType::Ephemeral,
                                ttl: cache_control.ttl().map(|t| t.into()),
                            };
                            ContentBlock::Text(TextContentBlock::new_with_cache_control(
                                m.value.clone(),
                                cache_control.clone(),
                            ))
                        } else {
                            ContentBlock::Text(TextContentBlock::new(m.value.clone()))
                        }
                    }
                    MessageContentType::ImageUrl => {
                        let url = m.value.clone();
                        let base64_data = url
                            .split_once(',')
                            .map_or_else(|| url.as_str(), |(_, data)| data);
                        ContentBlock::Image(ImageContentBlock::from(ImageContentSource::base64(
                            clust::messages::ImageMediaType::Png,
                            base64_data,
                        )))
                    }
                    MessageContentType::InputAudio => {
                        todo!()
                    }
                };
                blocks.push(msg)
            }

            Content::MultipleBlocks(blocks)
        }
    };

    ClustMessage::user(content)
}

pub fn record_map_err(e: impl Into<LLMError> + ToString, span: tracing::Span) -> LLMError {
    span.record("error", e.to_string());
    e.into()
}
