mod response_stream;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::client::completions::response_stream::{
    create_response_stream, StreamMessage, StreamState,
};
use crate::client::message_mapper::{MessageMapper, MessageMapperError};
use crate::types::gateway::{
    ChatCompletionMessage, ChatCompletionMessageWithFinishReason, ChatCompletionRequest,
};
use crate::types::message::Message;
use crate::types::ModelEvent;
use crate::{client::ModelInstance, error::LLMResult};
use tracing::Instrument;

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

    pub async fn create_stream(
        &self,
        request: impl Into<ChatCompletionRequest>,
    ) -> LLMResult<async_openai::types::ChatCompletionResponseStream> {
        self.stream(request)
    }

    pub fn stream(
        &self,
        request: impl Into<ChatCompletionRequest>,
    ) -> LLMResult<async_openai::types::ChatCompletionResponseStream> {
        let mut r = request.into();
        r.stream = Some(true);

        let include_usage = r.stream_options.map(|o| o.include_usage).unwrap_or(false);
        let messages = Self::map_messages(&r.messages, &r.model, r.user.clone())?;

        let stream_id = format!("chatcmpl-{}", Uuid::new_v4());
        let created = Utc::now().timestamp() as u32;
        let model_name = r.model.clone();

        let (model_tx, model_rx) = tokio::sync::mpsc::channel::<Option<ModelEvent>>(10_000);
        let (out_tx, out_rx) = tokio::sync::mpsc::channel::<StreamMessage>(10_000);

        let instance = self.instance.clone();
        let input_variables = self.input_variables.clone();
        let tags = self.tags.clone();
        let runner_tx = model_tx.clone();
        let runner_out_tx = out_tx.clone();

        let span = tracing::Span::current();
        tokio::spawn(async move {
            let result = instance
                .stream(input_variables, runner_tx.clone(), messages, tags)
                .instrument(span.clone())
                .await;
            if let Err(err) = result {
                let _ = runner_out_tx.send(StreamMessage::Error(err)).await;
            }
            let _ = runner_tx.send(None).await;
        });

        let user_tx = self.tx.clone();
        tokio::spawn(async move {
            let mut rx = model_rx;
            while let Some(event_opt) = rx.recv().await {
                match event_opt {
                    Some(event) => {
                        if let Some(sender) = &user_tx {
                            let _ = sender.send(Some(event.clone())).await;
                        }
                        if out_tx
                            .send(StreamMessage::Event(Box::new(event)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    None => {
                        let _ = out_tx.send(StreamMessage::Done).await;
                        break;
                    }
                }
            }
        });

        let state = StreamState {
            receiver: out_rx,
            buffer: VecDeque::new(),
            stream_id,
            model: model_name,
            created,
            include_usage,
        };

        Ok(create_response_stream(state))
    }
}
