use std::collections::HashMap;

use crate::types::engine::ResponsesEngineParamsBuilder;
use crate::types::instance::init_responses_model_instance;
use crate::{
    client::responses::stream::ResponsesResultStream, error::LLMResult, types::ModelEvent,
};
use async_openai::types::responses::CreateResponse;
use async_openai::types::responses::Response;
use serde_json::Value;
use tracing_futures::Instrument;

pub mod stream;

#[async_trait::async_trait]
pub trait Responses: Sync + Send {
    async fn invoke(
        &self,
        input_response: CreateResponse,
        tx: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    ) -> LLMResult<Response>;

    async fn stream(
        &self,
        input_response: CreateResponse,
        tx: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    ) -> LLMResult<ResponsesResultStream>;
}

pub struct ResponsesClient {
    builder: ResponsesEngineParamsBuilder,
    input_variables: HashMap<String, Value>,
    tx: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    tags: HashMap<String, String>,
    instance: Option<Box<dyn Responses>>,
}

impl ResponsesClient {
    pub fn new(builder: ResponsesEngineParamsBuilder) -> Self {
        Self {
            instance: None,
            builder,
            input_variables: HashMap::new(),
            tx: None,
            tags: HashMap::new(),
        }
    }

    pub fn with_instance(mut self, instance: Box<dyn Responses>) -> Self {
        self.instance = Some(instance);
        self
    }

    pub fn with_input_variables(mut self, input_variables: HashMap<String, Value>) -> Self {
        self.input_variables = input_variables;
        self
    }

    pub fn with_tx(mut self, tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>) -> Self {
        self.tx = Some(tx);
        self
    }

    pub fn with_tags(mut self, tags: HashMap<String, String>) -> Self {
        self.tags = tags;
        self
    }

    pub async fn create(&self, request: CreateResponse) -> LLMResult<Response> {
        let tx = self.tx.clone();

        match &self.instance {
            Some(instance) => instance.invoke(request.clone(), tx).await,
            None => {
                let engine = self.builder.build()?;
                let instance = init_responses_model_instance(engine, HashMap::new()).await?;
                instance.invoke(request.clone(), tx).await
            }
        }
    }

    pub async fn create_stream(&self, request: CreateResponse) -> LLMResult<ResponsesResultStream> {
        let tx = self.tx.clone();

        match &self.instance {
            Some(instance) => {
                instance
                    .stream(request.clone(), tx)
                    .instrument(tracing::Span::current())
                    .await
            }
            None => {
                let engine = self.builder.build()?;
                let instance = init_responses_model_instance(engine, HashMap::new()).await?;
                instance
                    .stream(request.clone(), tx)
                    .instrument(tracing::Span::current())
                    .await
            }
        }
    }
}
