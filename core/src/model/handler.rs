use std::collections::HashMap;

use crate::{
    error::GatewayError,
    events::{JsonValue, RecordResult},
    GatewayResult,
};

use super::{
    types::{ModelEvent, ModelEventType, ModelToolCall, ToolResultEvent, ToolStartEvent},
    Tool,
};
use serde_json::Value;
use tracing::Span;

// macro_rules! target {
//     () => {
//         "langdb::user_tracing::models"
//     };
//     ($subtgt:literal) => {
//         concat!("langdb::user_tracing::models::", $subtgt)
//     };
// }

pub(crate) async fn handle_tool_call(
    tool_use: &ModelToolCall,
    tools: &HashMap<String, Box<dyn Tool>>,
    tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
    tags: HashMap<String, String>,
) -> GatewayResult<String> {
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
        .ok_or(GatewayError::CustomError(format!(
            "Tool Not Found {}",
            tool_name
        )))?;

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
        .map_err(|e| GatewayError::CustomError(e.to_string()))?;
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
        .map_err(|e| GatewayError::CustomError(e.to_string()))?;
        result
    }
    // .instrument(span.or_current())
    .await
}
