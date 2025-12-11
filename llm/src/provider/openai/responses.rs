use crate::client::error::ModelError;
use crate::client::responses::stream::ResponsesResultStream;
use crate::client::responses::Responses;
use crate::error::LLMResult;
use crate::provider::openai::openai_client;
use crate::types::credentials::ApiKeyCredentials;
use crate::types::credentials_ident::CredentialsIdent;
use crate::types::gateway::GatewayModelUsage;
use crate::types::LLMFinishEvent;
use crate::types::LLMStartEvent;
use crate::types::ModelEvent;
use crate::types::ModelEventType;
use crate::types::ModelFinishReason;
use async_openai::config::OpenAIConfig;
use async_openai::types::responses::Content;
use async_openai::types::responses::CreateResponse;
use async_openai::types::responses::OutputContent;
use async_openai::types::responses::Response;
use async_openai::types::responses::ResponseEvent;
use async_openai::types::responses::ResponseStream;
use async_openai::types::responses::Status;
use async_openai::types::responses::Usage;
use async_openai::Client;
use tokio_stream::StreamExt;
use tracing::{field, Span};
use tracing_futures::Instrument;
use valuable::Valuable;
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
            if let OutputContent::Message(message) = output {
                for message_content in &message.content {
                    if let Content::OutputText(text) = message_content {
                        content.push_str(&text.text);
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
                    model_name: request.model.clone(),
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
        tx_response: &tokio::sync::mpsc::Sender<LLMResult<ResponseEvent>>,
    ) -> LLMResult<()> {
        let response = self
            .client
            .responses()
            .create_stream(request.clone())
            .await?;

        let _ = self.process_stream(response, tx, tx_response).await;

        Ok(())
    }

    async fn process_stream(
        &self,
        mut response: ResponseStream,
        _tx: Option<&tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
        tx_response: &tokio::sync::mpsc::Sender<LLMResult<ResponseEvent>>,
    ) -> LLMResult<()> {
        while let Some(event) = response.next().await {
            let event = event?;
            // Self::send_event(tx, ModelEvent::new(
            //     &Span::current(),
            //     ModelEventType::LlmStream(LLMStreamEvent {
            //         provider_name: SPAN_OPENAI.to_string(),
            //         model_name: request.model.clone(),
            //         output: Some(event.to_string()),
            //     })),
            // ).await;
            // TODO: send events to tx
            let _ = tx_response.send(Ok(event)).await;
        }
        Ok(())
    }

    async fn send_event(
        tx: Option<&tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
        event: ModelEvent,
    ) {
        if let Some(tx) = tx {
            let _ = tx.send(Some(event)).await;
        }
    }

    fn map_usage(usage: Option<&Usage>) -> Option<GatewayModelUsage> {
        usage.map(GatewayModelUsage::from)
    }

    fn map_finish_reason(finish_status: &Status) -> ModelFinishReason {
        match finish_status {
            Status::Completed => ModelFinishReason::Stop,
            Status::Failed => ModelFinishReason::Error,
            Status::InProgress => ModelFinishReason::InProgress,
            Status::Incomplete => ModelFinishReason::Incomplete,
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
                    model_name: request.model.clone(),
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
