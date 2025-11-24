use crate::error::LLMError;
use crate::error::LLMResult;
use crate::types::credentials_ident::CredentialsIdent;
use crate::types::gateway::ChatCompletionContent;
use crate::types::gateway::ChatCompletionMessage;
use crate::types::gateway::ChatCompletionMessageWithFinishReason;
use crate::types::message::Message;
use crate::types::LLMContentEvent;
use crate::types::LLMFinishEvent;
use crate::types::ModelEvent;
use crate::types::ModelEventType;
use crate::types::ModelFinishReason;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::Span;

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
        tx: mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        _tags: HashMap<String, String>,
    ) -> LLMResult<()> {
        for message in previous_messages {
            let content = message.content.unwrap_or_default();
            let chars: Vec<char> = content.chars().collect();

            let chunks: Vec<String> = chars
                .chunks(3)
                .map(|chunk| chunk.iter().collect())
                .collect();
            for chunk in chunks {
                tx.send(Some(ModelEvent::new(
                    &Span::current(),
                    ModelEventType::LlmContent(LLMContentEvent {
                        content: chunk.to_owned(),
                    }),
                )))
                .await
                .ok();
            }
        }

        tx.send(Some(ModelEvent::new(
            &Span::current(),
            ModelEventType::LlmStop(LLMFinishEvent {
                provider_name: "dummy".to_string(),
                model_name: "dummy_model".to_string(),
                output: None,
                usage: None,
                finish_reason: ModelFinishReason::Stop,
                tool_calls: vec![],
                credentials_ident: CredentialsIdent::Own,
            }),
        )))
        .await
        .map_err(|e| LLMError::CustomError(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::completions::CompletionsClient;
    use crate::types::gateway::ChatCompletionRequest;
    use async_openai::types::ChatCompletionResponseStream;
    use async_openai::types::CreateChatCompletionResponse;
    use futures::StreamExt;
    use std::sync::Arc;

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
        let message = response
            .message()
            .content
            .as_ref()
            .unwrap()
            .as_string()
            .unwrap();
        assert_eq!("Hello, world!", message);

        let response: CreateChatCompletionResponse = response.into();
        assert_eq!("test", response.model);
    }

    #[tokio::test]
    async fn test_create_stream() {
        let client = CompletionsClient::new(Arc::new(Box::new(DummyModelInstance {})));
        let request = ChatCompletionRequest {
            model: "dummy_model".to_string(),
            messages: vec![ChatCompletionMessage {
                role: "user".to_string(),
                content: Some(ChatCompletionContent::Text("Hello, world!".to_string())),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut response: ChatCompletionResponseStream =
            client.create_stream(request).await.unwrap();
        let mut chunks = vec![];
        while let Some(chunk) = response.next().await {
            chunks.push(chunk);
        }
        assert_eq!(6, chunks.len());
        assert_eq!(
            "Hello, world!",
            chunks
                .iter()
                .filter_map(|c| c.as_ref().unwrap().choices[0]
                    .delta
                    .content
                    .as_ref()
                    .map(|c| c.clone()))
                .collect::<Vec<String>>()
                .join("")
        );

        let last_chunk = chunks.last().unwrap().as_ref().unwrap();
        assert_eq!(
            Some(ModelFinishReason::Stop.into()),
            last_chunk.choices[0].finish_reason
        );
        assert_eq!("dummy_model", last_chunk.model);
    }
}
