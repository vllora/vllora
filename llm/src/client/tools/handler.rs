use std::collections::HashMap;
use std::sync::Arc;

use crate::error::LLMError;
use crate::error::LLMResult;
use vllora_telemetry::events::{JsonValue, RecordResult};

use crate::types::tools::Tool;
use crate::types::{ModelEvent, ModelEventType, ModelToolCall, ToolResultEvent, ToolStartEvent};
use opentelemetry::propagation::Injector;
use serde_json::Value;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

// macro_rules! target {
//     () => {
//         "vllora::user_tracing::models"
//     };
//     ($subtgt:literal) => {
//         concat!("vllora::user_tracing::models::", $subtgt)
//     };
// }

pub(crate) struct LlmToolCallCarrier<'a> {
    properties: &'a mut HashMap<String, String>,
}

impl<'a> LlmToolCallCarrier<'a> {
    pub fn new(properties: &'a mut HashMap<String, String>) -> Self {
        LlmToolCallCarrier { properties }
    }
}

impl Injector for LlmToolCallCarrier<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.properties.insert(key.into(), value);
    }
}

pub async fn handle_tool_call(
    tool_use: &ModelToolCall,
    tools: &HashMap<String, Arc<Box<dyn Tool>>>,
    tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
    mut tags: HashMap<String, String>,
) -> LLMResult<String> {
    let tool_name = tool_use.tool_name.clone();
    let arguments = tool_use.input.clone();
    let arguments_value = serde_json::from_str::<HashMap<String, Value>>(&arguments)?;
    // let span = tracing::info_span!(
    //     target: target!("tool"),
    //     crate::events::SPAN_TOOL,
    //     tool_name = tool_name,
    //     arguments = arguments.to_string(),
    //     output = tracing::field::Empty,
    //     error = tracing::field::Empty,
    // );
    let tool = tools
        .get(&tool_name)
        .ok_or(LLMError::CustomError(format!("Tool Not Found {tool_name}")))?;

    async {
        tx.send(Some(ModelEvent::new(
            &Span::current(),
            ModelEventType::ToolStart(ToolStartEvent {
                tool_id: tool_use.tool_id.clone(),
                tool_name: tool_name.clone(),
                input: arguments,
            }),
        )))
        .await
        .map_err(|e| LLMError::CustomError(e.to_string()))?;
        let span_context = Span::current().context();
        opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.inject_context(&span_context, &mut LlmToolCallCarrier::new(&mut tags))
        });

        let result = tool.run(arguments_value, tags).await;
        let _ = result.as_ref().map(JsonValue).record();
        let result = result.map(|v| v.to_string());
        tx.send(Some(ModelEvent::new(
            &Span::current(),
            ModelEventType::ToolResult(ToolResultEvent {
                tool_id: tool_name.clone(),
                tool_name,
                is_error: result.is_err(),
                output: result
                    .as_ref()
                    .map(|r| r.to_string())
                    .unwrap_or_else(|err| err.to_string()),
            }),
        )))
        .await
        .map_err(|e| LLMError::CustomError(e.to_string()))?;
        result
    }
    // .instrument(span.or_current())
    .await
}
