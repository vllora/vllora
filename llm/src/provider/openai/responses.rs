use std::collections::HashMap;

use crate::client::error::ModelError;
use crate::client::responses::stream::ResponsesResultStream;
use crate::client::responses::Responses;
use crate::error::LLMResult;
use crate::provider::openai::openai_client;
use crate::types::credentials::ApiKeyCredentials;
use crate::types::credentials_ident::CredentialsIdent;
use crate::types::gateway::GatewayModelUsage;
use crate::types::LLMContentEvent;
use crate::types::LLMFinishEvent;
use crate::types::LLMFirstToken;
use crate::types::LLMStartEvent;
use crate::types::ModelEvent;
use crate::types::ModelEventType;
use crate::types::ModelFinishReason;
use crate::types::ToolResultEvent;
use crate::types::ToolStartEvent;
use async_openai::config::OpenAIConfig;
use async_openai::error::OpenAIError;
use async_openai::error::StreamError;
use async_openai::types::responses::CreateResponse;
use async_openai::types::responses::OutputItem;
use async_openai::types::responses::OutputMessageContent;
use async_openai::types::responses::Response;
use async_openai::types::responses::ResponseCompletedEvent;
use async_openai::types::responses::ResponseStream;
use async_openai::types::responses::ResponseStreamEvent;
use async_openai::types::responses::ResponseUsage;
use async_openai::types::responses::Status;
use async_openai::Client;
use serde::Serialize;
use serde_json::json;
use tokio_stream::StreamExt;
use tracing::{field, Span};
use tracing_futures::Instrument;
use valuable::Valuable;
use vllora_telemetry::events;
use vllora_telemetry::events::JsonValue;
use vllora_telemetry::events::SPAN_OPENAI;

macro_rules! target {
    () => {
        "vllora::user_tracing::models::openai"
    };
    ($subtgt:literal) => {
        concat!("vllora::user_tracing::models::openai::", $subtgt)
    };
}

#[derive(Clone, Serialize)]
struct ToolCall {
    id: String,
    function: Option<FunctionCall>,
}

#[derive(Clone, Serialize)]
struct FunctionCall {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Clone)]
pub struct OpenAIResponses {
    client: Client<OpenAIConfig>,
    #[allow(dead_code)]
    credentials_ident: CredentialsIdent,
}

impl OpenAIResponses {
    pub fn new(
        credentials: Option<&ApiKeyCredentials>,
        endpoint: Option<&str>,
    ) -> Result<Self, ModelError> {
        let client = openai_client(credentials, endpoint)?;

        let credentials_ident = credentials
            .map(|_c| CredentialsIdent::Own)
            .unwrap_or(CredentialsIdent::Vllora);

        Ok(Self {
            client,
            credentials_ident,
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(
        &self,
        request: &CreateResponse,
        tx: Option<&tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    ) -> LLMResult<Response> {
        let span = Span::current();

        let response = self.client.responses().create(request.clone()).await?;
        let finish_reason = Self::map_finish_reason(&response.status);
        let mapped_usage = Self::map_usage(response.usage.as_ref());

        span.record("output", serde_json::to_string(&response)?);
        if let Some(usage) = &response.usage {
            span.record(
                "raw_usage",
                JsonValue(&serde_json::to_value(usage).unwrap()).as_value(),
            );
            span.record(
                "usage",
                JsonValue(&serde_json::to_value(mapped_usage.clone()).unwrap()).as_value(),
            );
        }

        let mut content = String::new();
        for output in &response.output {
            if let OutputItem::Message(message) = output {
                for c in &message.content {
                    match c {
                        OutputMessageContent::OutputText(text) => {
                            content.push_str(&text.text);
                        }
                        OutputMessageContent::Refusal(refusal) => {
                            content.push_str(&refusal.refusal);
                        }
                    }
                }
            }
        }

        Self::send_event(
            tx,
            ModelEvent::new(
                &span,
                ModelEventType::LlmStop(LLMFinishEvent {
                    provider_name: SPAN_OPENAI.to_string(),
                    model_name: request.model.clone().unwrap_or_default(),
                    output: Some(content),
                    usage: mapped_usage,
                    finish_reason: finish_reason.clone(),
                    tool_calls: vec![],
                    credentials_ident: self.credentials_ident.clone(),
                }),
            ),
        )
        .await;

        Ok(response)
    }

    async fn execute_stream(
        &self,
        request: &CreateResponse,
        tx: Option<&tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
        tx_response: &tokio::sync::mpsc::Sender<LLMResult<ResponseStreamEvent>>,
    ) -> LLMResult<()> {
        let span = Span::current();
        Self::send_event(
            tx,
            ModelEvent::new(
                &span,
                ModelEventType::LlmStart(LLMStartEvent {
                    provider_name: SPAN_OPENAI.to_string(),
                    model_name: request.model.clone().unwrap_or_default(),
                    input: serde_json::to_string(&request)?,
                }),
            ),
        )
        .await;
        let started_at = std::time::Instant::now();
        let response = self
            .client
            .responses()
            .create_stream(request.clone())
            .await?;

        let result = self
            .process_stream(response, tx, tx_response, started_at)
            .await?;

        if let Some(response_completed) = result {
            let finish_reason = Self::map_finish_reason(&response_completed.response.status);
            let mapped_usage = Self::map_usage(response_completed.response.usage.as_ref());
            let response = "".to_string();
            let _ = Self::send_event(
                tx,
                ModelEvent::new(
                    &span,
                    ModelEventType::LlmStop(LLMFinishEvent {
                        provider_name: SPAN_OPENAI.to_string(),
                        model_name: request.model.clone().unwrap_or_default(),
                        output: None,
                        usage: mapped_usage.clone(),
                        finish_reason,
                        tool_calls: vec![],
                        credentials_ident: self.credentials_ident.clone(),
                    }),
                ),
            )
            .await;

            span.record("output", serde_json::to_string(&response)?);
            span.record(
                "raw_usage",
                JsonValue(&serde_json::to_value(response_completed.response.usage).unwrap())
                    .as_value(),
            );
            span.record(
                "usage",
                JsonValue(&serde_json::to_value(mapped_usage).unwrap()).as_value(),
            );
        }

        Ok(())
    }

    async fn process_stream(
        &self,
        mut response: ResponseStream,
        tx: Option<&tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
        tx_response: &tokio::sync::mpsc::Sender<LLMResult<ResponseStreamEvent>>,
        started_at: std::time::Instant,
    ) -> LLMResult<Option<ResponseCompletedEvent>> {
        let mut response_completed = None;
        let mut tool_calls = HashMap::new();
        while let Some(event) = response.next().await {
            if let Err(OpenAIError::StreamError(StreamError::ReqwestEventSource(
                reqwest_eventsource::Error::StreamEnded,
            ))) = event
            {
                if response_completed.is_some() {
                    break;
                }
            }

            let event = event?;

            if let ResponseStreamEvent::ResponseCreated(_) = &event {
                Self::send_event(
                    tx,
                    ModelEvent::new(
                        &Span::current(),
                        ModelEventType::LlmFirstToken(LLMFirstToken {}),
                    ),
                )
                .await;
                Span::current().record("ttft", started_at.elapsed().as_micros());
            }

            if let ResponseStreamEvent::ResponseCompleted(c) = &event {
                response_completed = Some(c.clone());
            }

            let _ = Self::match_response_event(&event, &Span::current(), tx, &mut tool_calls).await;
            let _ = tx_response.send(Ok(event)).await;
        }

        Ok(response_completed)
    }

    async fn send_event(
        tx: Option<&tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
        event: ModelEvent,
    ) {
        if let Some(tx) = tx {
            tx.send(Some(event)).await.unwrap();
        }
    }

    fn map_usage(usage: Option<&ResponseUsage>) -> Option<GatewayModelUsage> {
        usage.map(GatewayModelUsage::from)
    }

    fn map_finish_reason(finish_status: &Status) -> ModelFinishReason {
        match finish_status {
            Status::Completed => ModelFinishReason::Stop,
            Status::Failed => ModelFinishReason::Error,
            Status::InProgress => ModelFinishReason::InProgress,
            Status::Incomplete => ModelFinishReason::Incomplete,
            Status::Queued => ModelFinishReason::Queued,
            Status::Cancelled => ModelFinishReason::Cancelled,
        }
    }

    async fn match_response_event(
        response_event: &ResponseStreamEvent,
        span: &tracing::Span,
        tx: Option<&tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
        tool_calls: &mut HashMap<String, (Option<String>, Option<Span>)>,
    ) {
        let Some(tx) = tx else {
            return;
        };

        let mut events = vec![];
        match response_event {
            ResponseStreamEvent::ResponseCreated(_) => {}
            ResponseStreamEvent::ResponseCompleted(_) => {}
            ResponseStreamEvent::ResponseFailed(_) => {}
            ResponseStreamEvent::ResponseIncomplete(_) => {}
            ResponseStreamEvent::ResponseQueued(_) => {}
            ResponseStreamEvent::ResponseOutputItemAdded(_) => {}
            ResponseStreamEvent::ResponseContentPartAdded(_) => {}
            ResponseStreamEvent::ResponseOutputTextDelta(delta) => {
                events.push(ModelEventType::LlmContent(LLMContentEvent {
                    content: delta.delta.clone(),
                }));
            }
            ResponseStreamEvent::ResponseOutputTextDone(_) => {}
            ResponseStreamEvent::ResponseRefusalDelta(_) => {}
            ResponseStreamEvent::ResponseRefusalDone(_) => {}
            ResponseStreamEvent::ResponseContentPartDone(_) => {}
            ResponseStreamEvent::ResponseOutputItemDone(item) => {
                if let OutputItem::WebSearchCall(call) = &item.item {
                    if let Some((_, tool_span)) = tool_calls.remove(&call.id) {
                        if let Some(tool_span) = tool_span {
                            let tool_calls = vec![ToolCall {
                                id: call.id.clone(),
                                function: Some(FunctionCall {
                                    name: "web_search".to_string(),
                                    arguments: json!(call.action),
                                }),
                            }];
                            tool_span.record(
                                "tool_calls",
                                JsonValue(&serde_json::to_value(tool_calls).unwrap()).as_value(),
                            );
                            tool_span.record("tool.name", "web_search".to_string());
                            // Drop the span by letting it go out of scope
                            // The span will be exited when dropped
                        }
                    }

                    events.push(ModelEventType::ToolResult(ToolResultEvent {
                        tool_id: call.id.clone(),
                        tool_name: "web_search".to_string(),
                        is_error: false,
                        output: "{}".to_string(),
                    }));
                }
            }
            ResponseStreamEvent::ResponseFunctionCallArgumentsDelta(_) => {}
            ResponseStreamEvent::ResponseFunctionCallArgumentsDone(_) => {}
            ResponseStreamEvent::ResponseFileSearchCallInProgress(_) => {}
            ResponseStreamEvent::ResponseFileSearchCallSearching(_) => {}
            ResponseStreamEvent::ResponseFileSearchCallCompleted(_) => {}
            ResponseStreamEvent::ResponseWebSearchCallInProgress(call) => {
                let tool_span = tracing::info_span!(
                    target: target!(),
                    parent: span.clone(),
                    events::SPAN_TOOLS,
                    tool_calls=field::Empty,
                    tool_results=field::Empty,
                    tool.name=field::Empty
                );
                tool_span.follows_from(span.id());
                // Enter the span when created - clone it first since entered() consumes it
                let _entered = tool_span.clone().entered();
                // Store the span so we can record on it and drop it later
                tool_calls.insert(call.item_id.clone(), (None, Some(tool_span)));
                // The _entered guard is dropped here, exiting the span
                // But the span itself remains stored until ResponseOutputItemDone

                events.push(ModelEventType::ToolStart(ToolStartEvent {
                    tool_id: call.item_id.clone(),
                    tool_name: "web_search".to_string(),
                    input: "{}".to_string(),
                }));
            }
            ResponseStreamEvent::ResponseWebSearchCallSearching(_) => {}
            ResponseStreamEvent::ResponseWebSearchCallCompleted(_) => {}
            ResponseStreamEvent::ResponseReasoningSummaryPartAdded(_) => {}
            ResponseStreamEvent::ResponseReasoningSummaryPartDone(_) => {}
            ResponseStreamEvent::ResponseReasoningSummaryTextDelta(_) => {}
            ResponseStreamEvent::ResponseReasoningSummaryTextDone(_) => {}
            ResponseStreamEvent::ResponseImageGenerationCallInProgress(_) => {}
            ResponseStreamEvent::ResponseImageGenerationCallGenerating(_) => {}
            ResponseStreamEvent::ResponseImageGenerationCallPartialImage(_) => {}
            ResponseStreamEvent::ResponseImageGenerationCallCompleted(_) => {}
            ResponseStreamEvent::ResponseCodeInterpreterCallInProgress(_) => {}
            ResponseStreamEvent::ResponseCodeInterpreterCallInterpreting(_) => {}
            ResponseStreamEvent::ResponseCodeInterpreterCallCompleted(_) => {}
            ResponseStreamEvent::ResponseCodeInterpreterCallCodeDelta(_) => {}
            ResponseStreamEvent::ResponseCodeInterpreterCallCodeDone(_) => {}
            ResponseStreamEvent::ResponseOutputTextAnnotationAdded(_) => {}
            ResponseStreamEvent::ResponseError(_) => {}
            _ => {}
        }

        for event in events {
            Self::send_event(Some(tx), ModelEvent::new(span, event)).await;
        }
    }
}

#[async_trait::async_trait]
impl Responses for OpenAIResponses {
    async fn invoke(
        &self,
        request: CreateResponse,
        tx: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    ) -> LLMResult<Response> {
        let input_str = serde_json::to_string(&request)?;
        let call_span = tracing::info_span!(
            target: target!("responses"),
            SPAN_OPENAI,
            input = input_str,
            output = field::Empty,
            ttft = field::Empty,
            error = field::Empty,
            usage = field::Empty
        );

        let _ = Self::send_event(
            tx.as_ref(),
            ModelEvent::new(
                &call_span,
                ModelEventType::LlmStart(LLMStartEvent {
                    provider_name: SPAN_OPENAI.to_string(),
                    model_name: request.model.clone().unwrap_or_default(),
                    input: serde_json::to_string(&request)?,
                }),
            ),
        )
        .await;

        self.execute(&request, tx.as_ref())
            .instrument(call_span.clone())
            .await
    }

    async fn stream(
        &self,
        request: CreateResponse,
        tx: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    ) -> LLMResult<ResponsesResultStream> {
        let input_str = serde_json::to_string(&request)?;
        let call_span = tracing::info_span!(
            target: target!("responses"),
            SPAN_OPENAI,
            input = input_str,
            output = field::Empty,
            ttft = field::Empty,
            error = field::Empty,
            usage = field::Empty
        );

        let (tx_response, rx_response) = tokio::sync::mpsc::channel(10000);
        let model = (*self).clone();
        let tx_clone = tx.clone();
        tokio::spawn(
            async move {
                let result = model
                    .execute_stream(&request, tx_clone.as_ref(), &tx_response)
                    .instrument(tracing::Span::current())
                    .await;

                if let Err(e) = result {
                    let _ = tx_response.send(Err(e)).await;
                }
            }
            .instrument(call_span.clone()),
        );

        Ok(ResponsesResultStream::create(rx_response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn read_fixture_file(event_file: &str) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push(format!("src/provider/openai/tests/fixtures/{event_file}"));
        fs::read_to_string(path).expect("Failed to read fixture file")
    }

    fn parse_fixture_events(content: &str) -> Vec<ResponseStreamEvent> {
        let mut events = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            if lines[i].starts_with("event: ") {
                if i + 1 < lines.len() && lines[i + 1].starts_with("data: ") {
                    let json_str = &lines[i + 1][6..]; // Skip "data: "
                    match serde_json::from_str::<ResponseStreamEvent>(json_str) {
                        Ok(event) => events.push(event),
                        Err(e) => panic!("Failed to parse event at line {}: {}", i + 1, e),
                    }
                    i += 2; // Skip both event and data lines
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }

        events
    }

    #[tokio::test]
    async fn test_match_response_event() {
        let fixture_content = read_fixture_file("basic_responses_stream");
        let response_events = parse_fixture_events(&fixture_content);

        let span = tracing::Span::current();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10000);
        let mut tool_calls = HashMap::new();

        for response_event in &response_events {
            // println!("Response event: {:?}", response_event);
            let _ = OpenAIResponses::match_response_event(
                response_event,
                &span,
                Some(&tx),
                &mut tool_calls,
            )
            .await;
        }

        let _ = tx.send(None).await;

        let expected_deltas = vec!["1", "  \n", "2", "  \n", "3", "  \n", "4", "  \n", "5"];
        let mut index = 0;
        while let Some(Some(event)) = rx.recv().await {
            if let ModelEventType::LlmContent(LLMContentEvent { content }) = &event.event {
                assert_eq!(content, expected_deltas[index]);
                index += 1;
            }
        }

        // Test that all events were processed without panicking
        assert!(!response_events.is_empty());
    }

    #[tokio::test]
    async fn test_match_response_event_web_search() {
        let fixture_content = read_fixture_file("web_search_example");
        let response_events = parse_fixture_events(&fixture_content);

        let span = tracing::Span::current();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10000);
        let mut tool_calls = HashMap::new();

        for response_event in &response_events {
            // println!("Response event: {:?}", response_event);
            let _ = OpenAIResponses::match_response_event(
                response_event,
                &span,
                Some(&tx),
                &mut tool_calls,
            )
            .await;
        }

        let _ = tx.send(None).await;

        let mut tool_start_found = false;
        let mut tool_result_found = false;
        let expected_tool_id = "ws_044dc2229a1fbfb200693bd4dfe09c8197b3cb2e8e2ee9bf80";

        while let Some(Some(event)) = rx.recv().await {
            match &event.event {
                ModelEventType::ToolStart(ToolStartEvent {
                    tool_id, tool_name, ..
                }) => {
                    assert_eq!(tool_id, expected_tool_id);
                    assert_eq!(tool_name, "web_search");
                    tool_start_found = true;
                }
                ModelEventType::ToolResult(ToolResultEvent {
                    tool_id,
                    tool_name,
                    is_error,
                    ..
                }) => {
                    assert_eq!(tool_id, expected_tool_id);
                    assert_eq!(tool_name, "web_search");
                    assert_eq!(*is_error, false);
                    tool_result_found = true;
                }
                _ => {}
            }
        }

        // Test that all events were processed without panicking
        assert!(!response_events.is_empty());
        // Verify that tool events were found
        assert!(tool_start_found, "ToolStart event should be found");
        assert!(tool_result_found, "ToolResult event should be found");
    }
}
