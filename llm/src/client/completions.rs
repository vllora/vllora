use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::client::message_mapper::{MessageMapper, MessageMapperError};
use crate::types::gateway::{
    ChatCompletionMessage, ChatCompletionMessageWithFinishReason, ChatCompletionRequest,
};
use crate::types::message::Message;
use crate::types::ModelEvent;
use crate::{client::ModelInstance, error::LLMResult};

pub struct CompletionsClient {
    instance: Arc<Box<dyn ModelInstance>>,
    input_variables: HashMap<String, Value>,
    tx: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    tags: HashMap<String, String>,
}

impl CompletionsClient {
    pub fn new(instance: Arc<Box<dyn ModelInstance>>) -> Self {
        Self {
            instance,
            input_variables: HashMap::new(),
            tx: None,
            tags: HashMap::new(),
        }
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

        self.instance
            .invoke(
                self.input_variables.clone(),
                tx,
                messages,
                self.tags.clone(),
            )
            .await
    }

    pub async fn create_stream(&self, request: impl Into<ChatCompletionRequest>) -> LLMResult<()> {
        let mut r = request.into();
        r.stream = Some(true);

        let messages = Self::map_messages(&r.messages, &r.model, r.user.clone())?;
        let tx = match &self.tx {
            Some(tx) => tx.clone(),
            None => {
                let (tx, mut _rx) = tokio::sync::mpsc::channel(100);
                tx
            }
        };

        self.instance
            .stream(
                self.input_variables.clone(),
                tx,
                messages,
                self.tags.clone(),
            )
            .await
    }
}
