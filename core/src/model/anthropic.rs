use super::error::{AuthorizationError, ModelError};
use super::tools::Tool;
use super::types::{
    LLMContentEvent, LLMFinishEvent, LLMStartEvent, ModelEvent, ModelEventType, ModelFinishReason,
    ModelToolCall, ToolStartEvent,
};
use super::{CredentialsIdent, ModelInstance};
use crate::error::GatewayError;
use crate::events::JsonValue;
use crate::events::SPAN_ANTHROPIC;
use crate::events::{self, RecordResult};
use crate::model::error::AnthropicError;
use crate::model::handler::handle_tool_call;
use crate::model::types::LLMFirstToken;
use crate::model::{async_trait, DEFAULT_MAX_RETRIES};
use crate::types::credentials::ApiKeyCredentials;
use crate::types::engine::{AnthropicModelParams, ExecutionOptions, Prompt};
use crate::types::gateway::CompletionModelUsage;
use crate::types::gateway::{ChatCompletionContent, ChatCompletionMessage, ToolCall};
use crate::types::message::{MessageType, PromptMessage};
use crate::types::threads::{InnerMessage, Message};
use crate::GatewayResult;
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
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::field;
use tracing::Instrument;
use tracing::Span;
use valuable::Valuable;

macro_rules! target {
    () => {
        "langdb::user_tracing::models::anthropic"
    };
    ($subtgt:literal) => {
        concat!("langdb::user_tracing::models::anthropic::", $subtgt)
    };
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
        std::env::var("LANGDB_ANTHROPIC_API_KEY").map_err(|_| AuthorizationError::InvalidApiKey)?
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
    prompt: Prompt,
    tools: Arc<HashMap<String, Box<dyn Tool>>>,
    credentials_ident: CredentialsIdent,
}

impl AnthropicModel {
    pub fn new(
        params: AnthropicModelParams,
        execution_options: ExecutionOptions,
        credentials: Option<&ApiKeyCredentials>,
        prompt: Prompt,
        tools: HashMap<String, Box<dyn Tool>>,
    ) -> Result<Self, ModelError> {
        let client: Client = anthropic_client(credentials)?;
        Ok(Self {
            params,
            execution_options,
            client,
            prompt,
            tools: Arc::new(tools),
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Langdb),
        })
    }

    async fn handle_tool_calls(
        function_calls: impl Iterator<Item = &ToolUse>,
        tools: &HashMap<String, Box<dyn Tool>>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> Vec<ClustMessage> {
        futures::future::join_all(function_calls.map(|tool_use| {
            let tags_value = tags.clone();
            async move {
                let tool_call = Self::map_tool_call(tool_use);
                let tool_call = tool_call.map_err(|e| GatewayError::CustomError(e.to_string()));
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
        system_message: SystemPrompt,
        messages: Vec<ClustMessage>,
        stream: bool,
    ) -> Result<MessagesRequestBody, AnthropicError> {
        let model = self.params.model.as_ref().unwrap();
        let builder = MessagesRequestBuilder::new(**model).system(system_message);
        let model_params = &self.params;
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

        let builder = builder.messages(messages.clone());

        let builder = match stream {
            true => builder.stream(StreamOption::ReturnStream),
            false => builder.stream(StreamOption::ReturnOnce),
        };
        let builder = if !self.tools.is_empty() {
            let mut tools: Vec<ToolDefinition> = vec![];
            for (_, tool) in self.tools.clone().iter() {
                tools.push(tool_definition(tool.deref()));
            }

            builder.tools(tools)
        } else {
            builder
        };

        Ok(builder.build())
    }

    fn handle_max_tokens_error() -> GatewayError {
        GatewayError::ModelError(ModelError::FinishError(
            "the maximum number of tokens specified in the request was reached".to_string(),
        ))
    }
    async fn process_stream(
        &self,
        stream: impl Stream<Item = Result<MessageChunk, StreamError>>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
    ) -> GatewayResult<(StopReason, Vec<ToolUse>, Option<Usage>)> {
        let mut tool_call_states: HashMap<u32, ToolUse> = HashMap::new();
        tokio::pin!(stream);
        let mut json_states: HashMap<u32, String> = HashMap::new();
        let mut input_tokens = 0;
        let mut first_response_received = false;

        loop {
            let r = stream.next().await.transpose();
            if !first_response_received {
                first_response_received = true;
                let now: u64 = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_micros() as u64;
                let _ = tx
                    .send(Some(ModelEvent::new(
                        &Span::current(),
                        ModelEventType::LlmFirstToken(LLMFirstToken { ttft: now }),
                    )))
                    .await;
            }
            match r {
                Ok(Some(result)) => match result {
                    MessageChunk::ContentBlockStart(block) => match block.content_block {
                        clust::messages::ContentBlockStart::TextContentBlock(block) => {
                            tx.send(Some(ModelEvent::new(
                                &tracing::Span::current(),
                                ModelEventType::LlmContent(LLMContentEvent {
                                    content: block.text,
                                }),
                            )))
                            .await
                            .map_err(|e| GatewayError::CustomError(e.to_string()))?;
                        }
                        clust::messages::ContentBlockStart::ToolUseContentBlock(tool_use_block) => {
                            tool_call_states.insert(block.index, tool_use_block.tool_use);
                            json_states.insert(block.index, String::new());
                        }
                    },
                    MessageChunk::ContentBlockDelta(block) => match block.delta {
                        clust::messages::ContentBlockDelta::TextDeltaContentBlock(delta) => {
                            tx.send(Some(ModelEvent::new(
                                &tracing::Span::current(),
                                ModelEventType::LlmContent(LLMContentEvent {
                                    content: delta.text,
                                }),
                            )))
                            .await
                            .map_err(|e| GatewayError::CustomError(e.to_string()))?;
                        }
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
                        input_tokens = start.message.usage.input_tokens;
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
                    }
                    MessageChunk::MessageDelta(delta) => {
                        let usage = Some(clust::messages::Usage {
                            input_tokens,
                            output_tokens: delta.usage.output_tokens,
                        });

                        if let Some(stop_reason) = delta.delta.stop_reason {
                            return Ok((
                                stop_reason,
                                tool_call_states.values().cloned().collect(),
                                usage,
                            ));
                        }
                    }
                    MessageChunk::MessageStop(s) => {
                        tracing::error!("Stream ended with error: {:#?}", s);
                    }
                },
                last_result => {
                    tracing::error!("Error in stream: {last_result:?}");
                    break;
                }
            }
        }

        unreachable!();
    }

    async fn execute(
        &self,
        system_message: SystemPrompt,
        input_messages: Vec<ClustMessage>,
        call_span: Span,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessage> {
        let request = self
            .build_request(system_message.clone(), input_messages, false)
            .map_err(custom_err)?;
        let mut calls = vec![(request, call_span.clone())];
        let mut retries = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        let credential_ident = self.credentials_ident.clone();
        while let Some((call, span)) = calls.pop() {
            if retries == 0 {
                return Err(ModelError::MaxRetriesReached.into());
            } else {
                retries -= 1;
            }
            let system_message = system_message.clone();
            let input_messages = call.messages.clone();

            tx.send(Some(ModelEvent::new(
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
            .await
            .map_err(|e| GatewayError::CustomError(e.to_string()))?;

            let response = async move {
                let result = self.client.create_a_message(call).await;
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
                    "usage",
                    JsonValue(&serde_json::to_value(response.usage).unwrap()).as_value(),
                );

                Ok::<_, GatewayError>(response)
            }
            .instrument(span.clone().or_current())
            .await?;

            // Alwayss present in non streamin mode
            let stop_reason = response.stop_reason.unwrap();

            match stop_reason {
                clust::messages::StopReason::EndTurn
                | clust::messages::StopReason::StopSequence => {
                    let message_content = response.content;

                    let usage = CompletionModelUsage {
                        input_tokens: response.usage.input_tokens,
                        output_tokens: response.usage.output_tokens,
                        total_tokens: response.usage.input_tokens + response.usage.output_tokens,
                        ..Default::default()
                    };

                    match message_content {
                        Content::SingleText(content) => {
                            tx.send(Some(ModelEvent::new(
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
                                    usage: Some(usage),
                                    finish_reason: ModelFinishReason::Stop,
                                    tool_calls: vec![],
                                    credentials_ident: credential_ident,
                                }),
                            )))
                            .await
                            .map_err(|e| GatewayError::CustomError(e.to_string()))?;

                            return Ok(ChatCompletionMessage {
                                content: Some(ChatCompletionContent::Text(content.to_owned())),
                                role: "assistant".to_string(),
                                ..Default::default()
                            });
                        }
                        Content::MultipleBlocks(blocks) => {
                            let mut final_text = String::new();
                            for b in blocks.iter() {
                                match b {
                                    ContentBlock::Text(text) => {
                                        final_text.push_str(&text.text);
                                    }
                                    _ => {
                                        return Err(GatewayError::ModelError(
                                            ModelError::CustomError(
                                                "unexpected content block".to_string(),
                                            ),
                                        ));
                                    }
                                }
                            }

                            tx.send(Some(ModelEvent::new(
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
                                    usage: Some(usage),
                                    finish_reason: ModelFinishReason::Stop,
                                    tool_calls: vec![],
                                    credentials_ident: credential_ident,
                                }),
                            )))
                            .await
                            .map_err(|e| GatewayError::CustomError(e.to_string()))?;

                            return Ok(ChatCompletionMessage {
                                content: Some(ChatCompletionContent::Text(final_text)),
                                role: "assistant".to_string(),
                                ..Default::default()
                            });
                        }
                    }
                }
                clust::messages::StopReason::MaxTokens => Self::handle_max_tokens_error(),
                clust::messages::StopReason::ToolUse => {
                    let content = response.content.clone();
                    let blocks = if let Content::MultipleBlocks(blocks) = response.content {
                        blocks
                    } else {
                        return Err(GatewayError::ModelError(ModelError::CustomError(
                            "Expected multiple tool blocks".to_string(),
                        )));
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
                                return Err(GatewayError::ModelError(ModelError::CustomError(
                                    "Unexpected content block in response".to_string(),
                                )));
                            }
                        }
                    }

                    let tool_calls_str = serde_json::to_string(&tool_runs)?;
                    let tools_span = tracing::info_span!(target: target!(), events::SPAN_TOOLS, tool_calls=tool_calls_str, label=tool_runs.iter().map(|t| t.name.clone()).collect::<Vec<String>>().join(","));
                    tools_span.follows_from(span.id());

                    let tool = self.tools.get(&tool_runs[0].name).unwrap();
                    if tool.stop_at_call() {
                        let usage = Some(CompletionModelUsage {
                            input_tokens: response.usage.input_tokens,
                            output_tokens: response.usage.output_tokens,
                            total_tokens: response.usage.input_tokens
                                + response.usage.output_tokens,
                            ..Default::default()
                        });
                        tx.send(Some(ModelEvent::new(
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
                                usage,
                                finish_reason: ModelFinishReason::ToolCalls,
                                tool_calls: tool_runs
                                    .iter()
                                    .map(|tool_call| ModelToolCall {
                                        tool_id: tool_call.id.clone(),
                                        tool_name: tool_call.name.clone(),
                                        input: serde_json::to_string(&tool_call.input).unwrap(),
                                    })
                                    .collect(),
                                credentials_ident: credential_ident,
                            }),
                        )))
                        .await
                        .map_err(|e| GatewayError::CustomError(e.to_string()))?;

                        return Ok(ChatCompletionMessage {
                            role: "assistant".to_string(),
                            content: text_content.map(ChatCompletionContent::Text),
                            tool_calls: Some(
                                tool_runs
                                    .iter()
                                    .map(|tool_call| {
                                        Ok(ToolCall {
                                            id: tool_call.id.clone(),
                                            r#type: "function".to_string(),
                                            function: crate::types::gateway::FunctionCall {
                                                name: tool_call.name.clone(),
                                                arguments: serde_json::to_string(&tool_call.input)?,
                                            },
                                        })
                                    })
                                    .collect::<Result<Vec<ToolCall>, GatewayError>>()?,
                            ),
                            ..Default::default()
                        });
                    } else {
                        let result_tool_calls = Self::handle_tool_calls(
                            tool_runs.iter(),
                            &self.tools,
                            tx,
                            tags.clone(),
                        )
                        .instrument(tools_span.clone())
                        .await;
                        messages.extend(result_tool_calls);

                        let conversation_messages = [input_messages, messages].concat();
                        let request = self
                            .build_request(system_message, conversation_messages, false)
                            .map_err(custom_err)?;
                        let input = serde_json::to_string(&request)?;
                        let call_span = tracing::info_span!(target: target!("chat"), SPAN_ANTHROPIC, input=input, output = field::Empty, ttft = field::Empty, error = field::Empty, usage = field::Empty);
                        call_span.follows_from(tools_span.id());
                        calls.push((request, call_span));
                        continue;
                    }
                }
            };
        }
        unreachable!();
    }

    fn map_usage(usage: Option<&Usage>) -> Option<CompletionModelUsage> {
        usage.map(|u| CompletionModelUsage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            total_tokens: u.input_tokens + u.output_tokens,
            ..Default::default()
        })
    }

    fn map_finish_reason(reason: &StopReason) -> ModelFinishReason {
        match reason {
            StopReason::EndTurn => ModelFinishReason::Stop,
            StopReason::StopSequence => ModelFinishReason::StopSequence,
            StopReason::ToolUse => ModelFinishReason::ToolCalls,
            StopReason::MaxTokens => ModelFinishReason::Length,
        }
    }

    fn map_tool_call(t: &ToolUse) -> Result<ModelToolCall, GatewayError> {
        Ok(ModelToolCall {
            tool_id: t.id.clone(),
            tool_name: t.name.clone(),
            input: serde_json::to_string(&t.input)?,
        })
    }

    async fn execute_stream(
        &self,
        system_message: SystemPrompt,
        input_messages: Vec<ClustMessage>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        call_span: Span,
        tags: HashMap<String, String>,
    ) -> GatewayResult<()> {
        let request = self
            .build_request(system_message.clone(), input_messages, true)
            .map_err(custom_err)?;
        let mut anthropic_calls = vec![(request, call_span)];
        let mut retries = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        let credentials_ident = self.credentials_ident.clone();
        while let Some((call, span)) = anthropic_calls.pop() {
            if retries == 0 {
                return Err(ModelError::MaxRetriesReached.into());
            } else {
                retries -= 1;
            }
            let system_message = system_message.clone();
            let input_messages = call.messages.clone();

            tx.send(Some(ModelEvent::new(
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
            .await
            .map_err(|e| GatewayError::CustomError(e.to_string()))?;

            let stream = self
                .client
                .create_a_message_stream(call)
                .await
                .map_err(custom_err)?;
            let (stop_reason, tool_calls, usage) = self
                .process_stream(stream, &tx)
                .instrument(span.clone())
                .await?;

            let trace_finish_reason = Self::map_finish_reason(&stop_reason);
            let usage = Self::map_usage(usage.as_ref());
            if let Some(usage) = &usage {
                span.record("usage", JsonValue(&serde_json::to_value(usage)?).as_value());
            }
            tx.send(Some(ModelEvent::new(
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
                    usage,
                    finish_reason: trace_finish_reason.clone(),
                    credentials_ident: credentials_ident.clone(),
                    tool_calls: tool_calls
                        .iter()
                        .map(Self::map_tool_call)
                        .collect::<Result<Vec<ModelToolCall>, GatewayError>>()?,
                }),
            )))
            .await
            .map_err(|e| GatewayError::CustomError(e.to_string()))?;

            let response = serde_json::json!({
                "stop_reason": trace_finish_reason,
                "tool_calls": tool_calls
            });
            span.record("output", response.to_string());
            match stop_reason {
                StopReason::EndTurn | StopReason::StopSequence => return Ok(()),
                StopReason::MaxTokens => return Err(Self::handle_max_tokens_error()),
                StopReason::ToolUse => {
                    let tool_calls_str = serde_json::to_string(&tool_calls)?;
                    let tools_span = tracing::info_span!(target: target!(), events::SPAN_TOOLS, tool_calls=tool_calls_str, label=tool_calls.iter().map(|t| t.name.clone()).collect::<Vec<String>>().join(","));
                    tools_span.follows_from(span.id());
                    let tool = self.tools.get(&tool_calls[0].name).unwrap();
                    if tool.stop_at_call() {
                        tx.send(Some(ModelEvent::new(
                            &span,
                            ModelEventType::ToolStart(ToolStartEvent {
                                tool_id: tool_calls[0].id.clone(),
                                tool_name: tool_calls[0].name.clone(),
                                input: serde_json::to_string(&tool_calls[0].input)?,
                            }),
                        )))
                        .await
                        .map_err(|e| GatewayError::CustomError(e.to_string()))?;

                        return Ok(());
                    } else {
                        let mut messages = vec![ClustMessage::assistant(Content::MultipleBlocks(
                            tool_calls
                                .iter()
                                .map(|t| ContentBlock::ToolUse(ToolUseContentBlock::new(t.clone())))
                                .collect(),
                        ))];
                        let result_tool_calls = Self::handle_tool_calls(
                            tool_calls.iter(),
                            &self.tools,
                            &tx,
                            tags.clone(),
                        )
                        .instrument(tools_span.clone())
                        .await;
                        messages.extend(result_tool_calls);

                        let conversation_messages = [input_messages, messages].concat();
                        let request = self
                            .build_request(system_message, conversation_messages, true)
                            .map_err(custom_err)?;
                        let input = serde_json::to_string(&request)?;
                        let call_span = tracing::info_span!(target: target!("chat"), SPAN_ANTHROPIC, input = input,output = field::Empty, ttft = field::Empty, error = field::Empty, usage = field::Empty);
                        call_span.follows_from(tools_span.id());
                        anthropic_calls.push((request, call_span));
                        continue;
                    }
                }
            }
        }

        Ok(())
    }

    fn map_previous_messages(messages_dto: Vec<Message>) -> GatewayResult<Vec<ClustMessage>> {
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
                                .collect::<Result<Vec<ContentBlock>, GatewayError>>()?,
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
    ) -> GatewayResult<ChatCompletionMessage> {
        let (system_prompt, conversational_messages) =
            self.construct_messages(input_variables, previous_messages)?;
        let input = serde_json::to_string(&conversational_messages)?;
        let call_span = tracing::info_span!(target: target!("chat"), SPAN_ANTHROPIC, input = input, output = field::Empty, error = field::Empty, ttft = field::Empty, usage = field::Empty, tags = JsonValue(&serde_json::to_value(tags.clone()).unwrap_or_default()).as_value());
        self.execute(
            system_prompt,
            conversational_messages,
            call_span.clone(),
            &tx,
            tags,
        )
        .instrument(call_span.clone())
        .await
        .map_err(|e| record_map_err(e, call_span))
    }

    async fn stream(
        &self,
        input_variables: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<()> {
        let (system_prompt, conversational_messages) =
            self.construct_messages(input_variables, previous_messages)?;
        let input = serde_json::to_string(&conversational_messages)?;
        let call_span = tracing::info_span!(target: target!("chat"), SPAN_ANTHROPIC, input = input, output = field::Empty, ttft = field::Empty, error = field::Empty, usage = field::Empty, tags = JsonValue(&serde_json::to_value(tags.clone()).unwrap_or_default()).as_value());
        self.execute_stream(
            system_prompt,
            conversational_messages,
            tx,
            call_span.clone(),
            tags,
        )
        .instrument(call_span.clone())
        .await
        .map_err(|e| record_map_err(e, call_span))
    }
}

impl AnthropicModel {
    fn construct_messages(
        &self,
        input_variables: HashMap<String, Value>,
        previous_messages: Vec<Message>,
    ) -> GatewayResult<(SystemPrompt, Vec<ClustMessage>)> {
        let mut conversational_messages = vec![];
        let mut system_message = self
            .prompt
            .messages
            .iter()
            .find(|m| m.r#type == MessageType::SystemMessage)
            .map(|message| map_system_message(message.to_owned(), &input_variables));

        if system_message.is_none() {
            system_message = previous_messages
                .iter()
                .find(|m| m.r#type == MessageType::SystemMessage)
                .map(|message| SystemPrompt::new(message.content.clone().unwrap_or_default()));
        }

        let Some(system_message) = system_message else {
            return Err(GatewayError::CustomError(
                "System prompt is missing".to_string(),
            ));
        };

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

        Ok((system_message, conversational_messages))
    }
}

fn map_system_message(prompt: PromptMessage, variables: &HashMap<String, Value>) -> SystemPrompt {
    let raw_message = Prompt::render(prompt.msg, variables.clone());
    SystemPrompt::new(raw_message)
}
fn map_chat_messages(
    prompt: PromptMessage,
    variables: &HashMap<String, Value>,
) -> GatewayResult<ClustMessage> {
    let message = match prompt.r#type {
        MessageType::AIMessage => {
            let raw_message = Prompt::render(prompt.msg, variables.clone());
            ClustMessage::assistant(Content::SingleText(raw_message))
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
        _ => {
            return Err(GatewayError::CustomError(
                "Unexpected system message".to_string(),
            ));
        }
    };
    Ok(message)
}

fn construct_user_message(m: &InnerMessage) -> ClustMessage {
    let content = match m {
        crate::types::threads::InnerMessage::Text(text) => Content::SingleText(text.to_owned()),
        crate::types::threads::InnerMessage::Array(content_array) => {
            let mut blocks = vec![];
            for m in content_array {
                let msg: ContentBlock = match m.r#type {
                    crate::types::threads::MessageContentType::Text => {
                        ContentBlock::Text(TextContentBlock::new(m.value.clone()))
                    }
                    crate::types::threads::MessageContentType::ImageUrl => {
                        let url = m.value.clone();
                        let base64_data = url
                            .split_once(',')
                            .map_or_else(|| url.as_str(), |(_, data)| data);
                        ContentBlock::Image(ImageContentBlock::from(ImageContentSource::base64(
                            clust::messages::ImageMediaType::Png,
                            base64_data,
                        )))
                    }
                    crate::types::threads::MessageContentType::InputAudio => {
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

pub fn record_map_err(e: impl Into<GatewayError> + ToString, span: tracing::Span) -> GatewayError {
    span.record("error", e.to_string());
    e.into()
}
