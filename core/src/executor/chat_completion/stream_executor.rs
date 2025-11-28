use crate::handler::{CallbackHandlerFn, ModelEventWithDetails};
use crate::GatewayApiError;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::Span;
use tracing_futures::Instrument;
use vllora_llm::client::completions::response_stream::ResultStream;
use vllora_llm::types::engine::CompletionModelDefinition;
use vllora_llm::types::engine::ParentCompletionOptions;
use vllora_llm::types::engine::ParentDefinition;
use vllora_llm::types::instance::ModelInstance;
use vllora_llm::types::message::Message;
use vllora_llm::types::{ModelEvent, ModelEventType};

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
    _cached_context: StreamCacheContext,
) -> Result<ResultStream, GatewayApiError> {
    let parent_definition =
        ParentDefinition::CompletionModel(Box::new(completion_model_definition.clone()));
    let model_options = ParentCompletionOptions {
        definition: Box::new(parent_definition),
        named_args: Default::default(),
        verbose: true,
    };

    let db_model = model_options.definition.get_db_model();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Option<ModelEvent>>(10000);

    tokio::spawn(async move {
        let mut assistant_msg = String::new();
        while let Some(Some(mut msg)) = rx.recv().await {
            if let ModelEventType::LlmContent(event) = &mut msg.event {
                assistant_msg.push_str(event.content.as_str());
            }

            callback_handler.on_message(ModelEventWithDetails::new(
                msg.clone(),
                Some(db_model.clone()),
            ));
        }

        let span = Span::current();
        span.record("response", assistant_msg.clone());
    });

    model
        .stream(input_vars, tx, messages, tags)
        .instrument(Span::current())
        .await
        .map_err(GatewayApiError::LLMError)
}
