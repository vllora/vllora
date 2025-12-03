pub mod response_stream;

use std::collections::HashMap;

use serde_json::Value;

use crate::client::completions::response_stream::ResultStream;
use crate::client::message_mapper::{MessageMapper, MessageMapperError};
use crate::client::ModelInstance;
use crate::error::LLMResult;
use crate::types::engine::CompletionEngineParamsBuilder;
use crate::types::gateway::{
    ChatCompletionMessage, ChatCompletionMessageWithFinishReason, ChatCompletionRequest,
};
use crate::types::instance::init_model_instance;
use crate::types::message::Message;
use crate::types::ModelEvent;
use tracing::Instrument;

pub struct CompletionsClient {
    builder: CompletionEngineParamsBuilder,
    input_variables: HashMap<String, Value>,
    tx: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    tags: HashMap<String, String>,
    instance: Option<Box<dyn ModelInstance>>,
}

impl CompletionsClient {
    pub fn new(builder: CompletionEngineParamsBuilder) -> Self {
        Self {
            instance: None,
            builder,
            input_variables: HashMap::new(),
            tx: None,
            tags: HashMap::new(),
        }
    }

    pub fn with_instance(mut self, instance: Box<dyn ModelInstance>) -> Self {
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

    fn map_messages(
        messages: &[ChatCompletionMessage],
        model: &str,
        user: Option<String>,
    ) -> Result<Vec<Message>, MessageMapperError> {
        messages
            .iter()
            .map(|message| {
                MessageMapper::map_completions_message_to_vllora_message(
                    message,
                    model,
                    &user.clone().unwrap_or_default(),
                )
            })
            .collect::<Result<Vec<Message>, MessageMapperError>>()
    }

    pub async fn create(
        &self,
        request: impl Into<ChatCompletionRequest>,
    ) -> LLMResult<ChatCompletionMessageWithFinishReason> {
        let r = request.into();

        let messages = Self::map_messages(&r.messages, &r.model, r.user.clone())?;

        let tx = match &self.tx {
            Some(tx) => tx.clone(),
            None => {
                let (tx, mut _rx) = tokio::sync::mpsc::channel(100);
                tx
            }
        };

        match &self.instance {
            Some(instance) => {
                instance
                    .invoke(
                        self.input_variables.clone(),
                        tx,
                        messages,
                        self.tags.clone(),
                    )
                    .await
            }
            None => {
                let engine = self.builder.build(&r)?;
                let instance = init_model_instance(engine, HashMap::new()).await?;
                instance
                    .invoke(
                        self.input_variables.clone(),
                        tx,
                        messages,
                        self.tags.clone(),
                    )
                    .await
            }
        }
    }

    pub async fn create_stream(
        &self,
        request: impl Into<ChatCompletionRequest>,
    ) -> LLMResult<ResultStream> {
        let mut r = request.into();
        r.stream = Some(true);

        let messages = Self::map_messages(&r.messages, &r.model, r.user.clone())?;

        let tx = match &self.tx {
            Some(tx) => tx.clone(),
            None => {
                let (tx, mut _rx) = tokio::sync::mpsc::channel(10000);
                tx
            }
        };

        match &self.instance {
            Some(instance) => {
                instance
                    .stream(
                        self.input_variables.clone(),
                        tx,
                        messages,
                        self.tags.clone(),
                    )
                    .instrument(tracing::Span::current())
                    .await
            }
            None => {
                let engine = self.builder.build(&r)?;
                let instance = init_model_instance(engine, HashMap::new()).await?;
                instance
                    .stream(
                        self.input_variables.clone(),
                        tx,
                        messages,
                        self.tags.clone(),
                    )
                    .instrument(tracing::Span::current())
                    .await
            }
        }
    }
}
