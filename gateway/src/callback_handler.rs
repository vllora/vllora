use std::sync::Arc;

use langdb_core::usage::InMemoryStorage;
use langdb_core::{
    handler::CallbackHandlerFn, model::types::ModelEventType,
    types::gateway::ImageGenerationModelUsage,
};

use crate::{cost::GatewayCostCalculator, usage::update_usage};
use tokio::sync::Mutex;

pub fn init_callback_handler(
    storage: Arc<Mutex<InMemoryStorage>>,
    calculator: GatewayCostCalculator,
) -> CallbackHandlerFn {
    let (tx, mut rx) = tokio::sync::broadcast::channel(100);

    let callback_handler = CallbackHandlerFn(Some(tx));

    tokio::spawn(async move {
        loop {
            if let Ok(model_event) = rx.recv().await {
                tracing::debug!(target: "model_event", "Received model event: {model_event:#?}");
                if let ModelEventType::LlmStop(finish_event) = &model_event.event.event {
                    let model_name = finish_event.model_name.clone();
                    let usage = finish_event.usage.clone();
                    let result = update_usage(
                        storage.clone(),
                        &calculator,
                        &model_name,
                        &model_event.model.provider_name,
                        usage
                            .map(langdb_core::types::gateway::Usage::CompletionModelUsage)
                            .as_ref(),
                    )
                    .await;

                    if let Err(e) = result {
                        tracing::error!("Error setting model usage: {e}");
                    }
                }

                if let ModelEventType::ImageGenerationFinish(finish_event) =
                    &model_event.event.event
                {
                    let model_name = finish_event.model_name.clone();
                    let result = update_usage(
                        storage.clone(),
                        &calculator,
                        &model_name,
                        &model_event.model.provider_name,
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
                    )
                    .await;

                    if let Err(e) = result {
                        tracing::error!("Error setting model usage: {e}");
                    }
                }
            }
        }
    });

    callback_handler
}
