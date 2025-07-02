use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use chrono::{DateTime, Utc};
use langdb_core::usage::InMemoryStorage;
use langdb_core::{
    handler::CallbackHandlerFn, model::types::ModelEventType,
    types::gateway::ImageGenerationModelUsage,
};

use crate::{cost::GatewayCostCalculator, usage::update_usage};

pub fn init_callback_handler(
    storage: Arc<Mutex<InMemoryStorage>>,
    calculator: GatewayCostCalculator,
) -> CallbackHandlerFn {
    let (tx, mut rx) = tokio::sync::broadcast::channel(100);
    let start_times = Arc::new(Mutex::new(HashMap::<String, DateTime<Utc>>::new()));
    let ttft_times = Arc::new(Mutex::new(HashMap::<String, i64>::new()));

    let callback_handler = CallbackHandlerFn(Some(tx));

    tokio::spawn({
        let start_times = start_times.clone();
        let ttft_times = ttft_times.clone();
        async move {
            loop {
                if let Ok(model_event) = rx.recv().await {
                    tracing::debug!(target: "model_event", "Received model event: {model_event:#?}");

                    match &model_event.event.event {
                        ModelEventType::LlmStart(_) => {
                            let mut times = start_times.lock().await;
                            times.insert(
                                model_event.event.trace_id.clone(),
                                model_event.event.timestamp,
                            );
                            tracing::debug!(
                                "Recorded LlmStart time for trace {}",
                                model_event.event.trace_id
                            );
                        }
                        ModelEventType::LlmFirstToken(_) => {
                            let ttft = {
                                let times = start_times.lock().await;
                                if let Some(start_time) = times.get(&model_event.event.trace_id) {
                                    let duration = model_event.event.timestamp - *start_time;
                                    let ttft_ms = duration.num_milliseconds();
                                    let mut ttft_map = ttft_times.lock().await;
                                    ttft_map.insert(model_event.event.trace_id.clone(), ttft_ms);
                                    Some(ttft_ms)
                                } else {
                                    tracing::warn!(
                                        "No start time found for trace {}",
                                        model_event.event.trace_id
                                    );
                                    None
                                }
                            };

                            if let Some(ttft_ms) = ttft {
                                tracing::info!(
                                    "TTFT for trace {}: {} milliseconds",
                                    model_event.event.trace_id,
                                    ttft_ms
                                );
                            }
                        }
                        ModelEventType::LlmStop(finish_event) => {
                            let model_name = finish_event.model_name.clone();
                            let usage = finish_event.usage.clone();

                            // Calculate duration and get ttft
                            let (duration, ttft) = {
                                let mut times = start_times.lock().await;
                                let mut ttft_map = ttft_times.lock().await;
                                let duration =
                                    times.remove(&model_event.event.trace_id).map(|start_time| {
                                        let duration = model_event.event.timestamp - start_time;
                                        duration.num_milliseconds()
                                    });

                                if duration.is_none() {
                                    tracing::warn!(
                                        "No start time found for trace {}",
                                        model_event.event.trace_id
                                    );
                                }

                                let ttft = ttft_map.remove(&model_event.event.trace_id);
                                (duration, ttft)
                            };

                            if let Some(model) = &model_event.model {
                                let result = update_usage(
                                    storage.clone(),
                                    &calculator,
                                    &model_name,
                                    &model.provider_name,
                                    usage
                                        .map(langdb_core::types::gateway::Usage::CompletionModelUsage)
                                        .as_ref(),
                                    duration.map(|d| d as u64),
                                    ttft.map(|t| t as u64),
                                )
                                .await;

                                if let Err(e) = result {
                                    tracing::error!("Error setting model usage: {e}");
                                };
                            }
                        }
                        ModelEventType::ImageGenerationFinish(finish_event) => {
                            if let Some(model) = &model_event.model {
                                let model_name = finish_event.model_name.clone();
                                let result = update_usage(
                                    storage.clone(),
                                    &calculator,
                                    &model_name,
                                    &model.provider_name,
                                    Some(
                                        &langdb_core::types::gateway::Usage::ImageGenerationModelUsage(
                                            ImageGenerationModelUsage {
                                                quality: finish_event.quality.clone(),
                                                size: finish_event.size.clone().into(),
                                                images_count: finish_event.count_of_images,
                                                steps_count: finish_event.steps,
                                            },
                                        ),
                                    ),
                                    None,
                                    None,
                                )
                                .await;

                                if let Err(e) = result {
                                    tracing::error!("Error setting model usage: {e}");
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    callback_handler
}
