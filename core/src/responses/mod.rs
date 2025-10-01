use crate::model::error::ModelError;
use crate::model::openai::openai_client;
use crate::model::types::ModelEvent;
use crate::model::CredentialsIdent;
use crate::telemetry::events::SPAN_OPENAI;
use crate::types::credentials::ApiKeyCredentials;
use crate::GatewayResult;
use async_openai::config::OpenAIConfig;
use async_openai::types::responses::CreateResponse;
use async_openai::types::responses::Response;
use async_openai::Client;
use tracing::Instrument;
use tracing::{field, Span};
macro_rules! target {
    () => {
        "langdb::user_tracing::models::openai"
    };
    ($subtgt:literal) => {
        concat!("langdb::user_tracing::models::openai::", $subtgt)
    };
}

#[allow(async_fn_in_trait)]
pub trait Responses: Sync + Send {
    async fn invoke(
        &self,
        input_text: CreateResponse,
        tx: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    ) -> GatewayResult<Response>;
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
            .unwrap_or(CredentialsIdent::Langdb);

        Ok(Self {
            client,
            credentials_ident,
        })
    }

    async fn execute(
        &self,
        input: CreateResponse,
        _span: Span,
        _tx: Option<&tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    ) -> GatewayResult<Response> {
        let response = self.client.responses().create(input).await.unwrap();

        Ok(response)
    }
}

impl Responses for OpenAIResponses {
    async fn invoke(
        &self,
        input_text: CreateResponse,
        tx: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    ) -> GatewayResult<Response> {
        let input = serde_json::to_string(&input_text)?;
        let call_span = tracing::info_span!(target: target!("responses"), SPAN_OPENAI, input = input, output = field::Empty, ttft = field::Empty, error = field::Empty, usage = field::Empty);

        self.execute(input_text, call_span.clone(), tx.as_ref())
            .instrument(call_span.clone())
            .await
    }
}
