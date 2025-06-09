use std::collections::HashMap;
use std::sync::Arc;

use crate::model::types::LLMFinishEvent;
use crate::model::types::ModelEvent;
use futures::future::join;
use futures::StreamExt;
use futures::TryStreamExt;

use crate::{
    model::{
        types::{ModelEventType, ModelFinishReason},
        ModelInstance,
    },
    types::{
        engine::ParentCompletionOptions,
        gateway::{ChatCompletionDelta, FunctionCall, ToolCall},
        threads::Message,
    },
};
use tokio_stream::wrappers::ReceiverStream;
use tracing::Span;
use tracing_futures::Instrument;

use super::stream_wrapper::wrap_stream;
use crate::executor::chat_completion::ChatCompletionStream;
use crate::handler::{CallbackHandlerFn, ModelEventWithDetails};
use crate::types::engine::CompletionModelDefinition;
use crate::types::engine::ParentDefinition;
use crate::GatewayApiError;

#[derive(Default)]
pub struct StreamCacheContext {
    pub events_sender: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    pub cached_events: Option<Vec<ModelEvent>>,
}

pub async fn stream_chunks(
    completion_model_definition: CompletionModelDefinition,
    model: Box<dyn ModelInstance>,
    messages: Vec<Message>,
    callback_handler: Arc<CallbackHandlerFn>,
    tags: HashMap<String, String>,
    input_vars: HashMap<String, serde_json::Value>,
    cached_context: StreamCacheContext,
) -> Result<ChatCompletionStream, GatewayApiError> {
    let parent_definition =
        ParentDefinition::CompletionModel(Box::new(completion_model_definition.clone()));
    let model_options = ParentCompletionOptions {
        definition: Box::new(parent_definition),
        named_args: Default::default(),
        verbose: true,
    };

    let db_model = model_options.definition.get_db_model();
    let (outer_tx, rx) = tokio::sync::mpsc::channel(100);

    tokio::spawn(
        async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<Option<ModelEvent>>(100);
            let forward_fut = async {
                let mut assistant_msg = String::new();
                while let Some(Some(mut msg)) = rx.recv().await {
                    if let ModelEventType::LlmContent(event) = &mut msg.event {
                        assistant_msg.push_str(event.content.as_str());
                    }

                    callback_handler
                        .on_message(ModelEventWithDetails::new(msg.clone(), db_model.clone()));
                    let e = outer_tx.send(Ok(msg)).await;
                    match e {
                        Ok(_) => {}
                        Err(e) => {
                            tracing::error!("Error in sending message: {e}");
                        }
                    }
                }

                let span = Span::current();
                span.record("response", assistant_msg.clone());
            };

            tracing::warn!("Cached events: {:#?}", cached_context.cached_events);
            match cached_context.cached_events {
                Some(cached_events) => {
                    for event in cached_events {
                        tracing::warn!("Cached event: {:#?}", event);
                        tx.send(Some(event)).await.unwrap();
                    }

                    tx.send(None).await.unwrap();

                    forward_fut.await;
                }
                None => {
                    let result_fut = model
                        .stream(input_vars, tx, messages, tags)
                        .instrument(Span::current());

                    let (result, _) = join(result_fut, forward_fut).await;
                    if let Err(e) = result {
                        outer_tx
                            .send(Err(GatewayApiError::GatewayError(e)))
                            .await
                            .unwrap();
                    }
                }
            }
        }
        .in_current_span(),
    );
    let event_stream = ReceiverStream::new(rx)
        .into_stream()
        .then(move |e| {
            let events_sender = cached_context.events_sender.clone();
            async move {
                if let Ok(event) = &e {
                    if let Some(events_sender) = events_sender {
                        events_sender.send(Some(event.clone())).await.unwrap();
                    }
                }

                e
            }
        })
        .filter_map(|e: Result<ModelEvent, GatewayApiError>| async move {
            e.map_or_else(
                |e| Some(Err(e)),
                |model_event| match model_event.event {
                    ModelEventType::LlmContent(_)
                    | ModelEventType::ToolStart(_)
                    | ModelEventType::LlmStop(_) => Some(Ok(model_event)),
                    _ => None,
                },
            )
        })
        .then(move |e: Result<ModelEvent, GatewayApiError>| async move {
            match e {
                Ok(e) => match e.event {
                    ModelEventType::LlmContent(content) => Ok((
                        Some(ChatCompletionDelta {
                            role: Some("assistant".to_string()),
                            content: Some(content.content),
                            tool_calls: None,
                        }),
                        None,
                        None,
                    )),
                    ModelEventType::ToolStart(tool_call) => Ok((
                        Some(ChatCompletionDelta {
                            role: Some("assistant".to_string()),
                            content: None,
                            tool_calls: Some(vec![ToolCall {
                                index: Some(0),
                                id: tool_call.tool_id.clone(),
                                r#type: "function".into(),
                                function: FunctionCall {
                                    name: tool_call.tool_name.clone(),
                                    arguments: tool_call.input.clone(),
                                },
                            }]),
                        }),
                        None,
                        None,
                    )),
                    ModelEventType::LlmStop(LLMFinishEvent {
                        usage,
                        finish_reason,
                        tool_calls,
                        ..
                    }) => {
                        let ev = match finish_reason {
                            ModelFinishReason::ToolCalls => Some(ChatCompletionDelta {
                                role: Some("assistant".to_string()),
                                content: None,
                                tool_calls: Some(
                                    tool_calls
                                        .into_iter()
                                        .enumerate()
                                        .map(|(index, tc)| ToolCall {
                                            index: Some(index),
                                            id: tc.tool_id.clone(),
                                            r#type: "function".into(),
                                            function: FunctionCall {
                                                name: tc.tool_name.clone(),
                                                arguments: tc.input.clone(),
                                            },
                                        })
                                        .collect(),
                                ),
                            }),
                            _ => None,
                        };

                        Ok((ev, usage, None))
                    }
                    _ => Err(GatewayApiError::CustomError(
                        "Unsupported event".to_string(),
                    )),
                },
                Err(e) => {
                    tracing::error!("Error in event: {e}");
                    Err(e)
                }
            }
        });

    Ok(wrap_stream(event_stream))
}
