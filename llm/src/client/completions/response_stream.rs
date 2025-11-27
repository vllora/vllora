use std::collections::VecDeque;

use crate::error::LLMError;
use crate::types::gateway::CompletionModelUsage;
use crate::types::{ModelEvent, ModelEventType, ModelFinishReason, ModelToolCall};
use async_openai::error::OpenAIError;
use async_openai::types::ChatCompletionResponseStream;
use async_openai::types::{
    ChatChoiceStream, ChatCompletionMessageToolCallChunk, ChatCompletionStreamResponseDelta,
    ChatCompletionToolType, CompletionUsage, CreateChatCompletionStreamResponse,
    FunctionCallStream, Role,
};
use futures::stream;

pub fn create_response_stream(state: StreamState) -> ChatCompletionResponseStream {
    let response_stream = stream::unfold(state, |mut state| async move {
        loop {
            if let Some(chunk) = state.buffer.pop_front() {
                return Some((Ok(chunk), state));
            }

            match state.receiver.recv().await {
                Some(StreamMessage::Event(event)) => {
                    let new_chunks = map_event_to_chunks(
                        &event,
                        &state.stream_id,
                        state.created,
                        &state.model,
                        state.include_usage,
                    );
                    if new_chunks.is_empty() {
                        continue;
                    }
                    state.buffer.extend(new_chunks);
                }
                Some(StreamMessage::Error(err)) => {
                    return Some((Err(OpenAIError::StreamError(err.to_string())), state));
                }
                Some(StreamMessage::Done) | None => {
                    return None;
                }
            }
        }
    });

    Box::pin(response_stream)
}

pub(crate) enum StreamMessage {
    Event(Box<ModelEvent>),
    Error(LLMError),
    Done,
}

pub(crate) struct StreamState {
    pub(crate) receiver: tokio::sync::mpsc::Receiver<StreamMessage>,
    pub(crate) buffer: VecDeque<CreateChatCompletionStreamResponse>,
    pub(crate) stream_id: String,
    pub(crate) model: String,
    pub(crate) created: u32,
    pub(crate) include_usage: bool,
}

fn map_event_to_chunks(
    event: &ModelEvent,
    stream_id: &str,
    created: u32,
    model: &str,
    include_usage: bool,
) -> Vec<CreateChatCompletionStreamResponse> {
    match &event.event {
        ModelEventType::LlmContent(content) => {
            let delta = {
                #[allow(deprecated)]
                {
                    ChatCompletionStreamResponseDelta {
                        content: Some(content.content.clone()),
                        role: Some(Role::Assistant),
                        refusal: None,
                        tool_calls: None,
                        function_call: None,
                    }
                }
            };
            vec![build_chunk(
                stream_id,
                model,
                created,
                vec![ChatChoiceStream {
                    index: 0,
                    delta,
                    finish_reason: None,
                    logprobs: None,
                }],
                None,
            )]
        }
        ModelEventType::ToolStart(tool_start) => {
            let delta = {
                #[allow(deprecated)]
                {
                    ChatCompletionStreamResponseDelta {
                        content: None,
                        role: Some(Role::Assistant),
                        refusal: None,
                        tool_calls: Some(vec![ChatCompletionMessageToolCallChunk {
                            index: 0,
                            id: Some(tool_start.tool_id.clone()),
                            r#type: Some(ChatCompletionToolType::Function),
                            function: Some(FunctionCallStream {
                                name: Some(tool_start.tool_name.clone()),
                                arguments: Some(tool_start.input.clone()),
                            }),
                        }]),
                        function_call: None,
                    }
                }
            };
            vec![build_chunk(
                stream_id,
                model,
                created,
                vec![ChatChoiceStream {
                    index: 0,
                    delta,
                    finish_reason: None,
                    logprobs: None,
                }],
                None,
            )]
        }
        ModelEventType::LlmStop(finish_event) => {
            let delta = {
                #[allow(deprecated)]
                {
                    ChatCompletionStreamResponseDelta {
                        content: None,
                        role: None,
                        refusal: None,
                        tool_calls: if finish_event.finish_reason == ModelFinishReason::ToolCalls {
                            Some(map_tool_calls(&finish_event.tool_calls))
                        } else {
                            None
                        },
                        function_call: None,
                    }
                }
            };
            let mut chunks = vec![build_chunk(
                stream_id,
                model,
                created,
                vec![ChatChoiceStream {
                    index: 0,
                    delta,
                    finish_reason: Some(finish_event.finish_reason.clone().into()),
                    logprobs: None,
                }],
                None,
            )];

            if include_usage {
                if let Some(usage) = finish_event.usage.as_ref() {
                    chunks.push(build_chunk(
                        stream_id,
                        model,
                        created,
                        vec![],
                        Some(map_usage(usage)),
                    ));
                }
            }

            chunks
        }
        _ => vec![],
    }
}

fn build_chunk(
    stream_id: &str,
    model: &str,
    created: u32,
    choices: Vec<ChatChoiceStream>,
    usage: Option<CompletionUsage>,
) -> CreateChatCompletionStreamResponse {
    CreateChatCompletionStreamResponse {
        id: stream_id.to_string(),
        choices,
        created,
        model: model.to_string(),
        service_tier: None,
        system_fingerprint: None,
        object: Some("chat.completion.chunk".to_string()),
        usage,
    }
}

fn map_usage(usage: &CompletionModelUsage) -> CompletionUsage {
    CompletionUsage {
        prompt_tokens: usage.input_tokens,
        completion_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        prompt_tokens_details: usage
            .prompt_tokens_details
            .clone()
            .map(|details| details.into()),
        completion_tokens_details: usage
            .completion_tokens_details
            .clone()
            .map(|details| details.into()),
    }
}

fn map_tool_calls(tool_calls: &[ModelToolCall]) -> Vec<ChatCompletionMessageToolCallChunk> {
    tool_calls
        .iter()
        .enumerate()
        .map(|(index, call)| ChatCompletionMessageToolCallChunk {
            index: index as u32,
            id: Some(call.tool_id.clone()),
            r#type: Some(ChatCompletionToolType::Function),
            function: Some(FunctionCallStream {
                name: Some(call.tool_name.clone()),
                arguments: Some(call.input.clone()),
            }),
        })
        .collect()
}
