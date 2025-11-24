use crate::error::LLMResult;
use crate::types::gateway::ChatCompletionContent;
use crate::types::gateway::ChatCompletionMessage;
use crate::types::gateway::ChatCompletionMessageWithFinishReason;
use crate::types::message::Message;
use crate::types::ModelEvent;
use crate::types::ModelFinishReason;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::mpsc;

#[cfg(test)]
use uuid;

#[cfg(test)]
fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(not(test))]
fn generate_id() -> String {
    format!("dummy-{}", Utc::now().timestamp_millis())
}

#[async_trait]
pub trait ModelInstance: Sync + Send {
    async fn invoke(
        &self,
        input_vars: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> LLMResult<ChatCompletionMessageWithFinishReason>;

    async fn stream(
        &self,
        input_vars: HashMap<String, Value>,
        tx: mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> LLMResult<()>;
}

pub struct DummyModelInstance {}

#[async_trait]
impl ModelInstance for DummyModelInstance {
    async fn invoke(
        &self,
        _input_vars: HashMap<String, Value>,
        _tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        _previous_messages: Vec<Message>,
        _tags: HashMap<String, String>,
    ) -> LLMResult<ChatCompletionMessageWithFinishReason> {
        Ok(ChatCompletionMessageWithFinishReason::new(
            ChatCompletionMessage {
                role: "assistant".to_string(),
                content: Some(ChatCompletionContent::Text("Hello, world!".to_string())),
                ..Default::default()
            },
            ModelFinishReason::Stop,
            generate_id(),
            Utc::now().timestamp_millis() as u32,
            "test".to_string(),
            None,
        ))
    }

    async fn stream(
        &self,
        _input_vars: HashMap<String, Value>,
        _tx: mpsc::Sender<Option<ModelEvent>>,
        _previous_messages: Vec<Message>,
        _tags: HashMap<String, String>,
    ) -> LLMResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use async_openai::types::CreateChatCompletionResponse;
    use crate::client::completions::CompletionsClient;
    use std::sync::Arc;
    use super::*;
    use crate::types::gateway::ChatCompletionRequest;

    #[tokio::test]
    async fn test_create() {
        let client = CompletionsClient::new(Arc::new(Box::new(DummyModelInstance {})));
        let request = ChatCompletionRequest {
            model: "test".to_string(),
            messages: vec![ChatCompletionMessage {
                role: "user".to_string(),
                content: Some(ChatCompletionContent::Text("Hello, world!".to_string())),
                ..Default::default()
            }],
            ..Default::default()
        };
        let response: ChatCompletionMessageWithFinishReason = client.create(request).await.unwrap();
        let message = response.message().content.as_ref().unwrap().as_string().unwrap();
        assert_eq!("Hello, world!", message);

        let response: CreateChatCompletionResponse = response.into();
        assert_eq!("test", response.model);
        assert_greater_than!(response.created, Utc::now().timestamp_millis() as u32);
        assert_eq!(uuid::Uuid::new_v4().to_string(), response.id);
        assert_eq!(None, response.usage);
    }
}
