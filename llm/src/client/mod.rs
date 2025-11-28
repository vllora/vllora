pub mod completions;
pub mod error;
pub mod message_mapper;
pub mod tools;

use crate::types::engine::CompletionEngineParamsBuilder;
pub use crate::types::instance::DummyModelInstance;
pub use crate::types::instance::ModelInstance;

use crate::client::completions::CompletionsClient;
use crate::client::error::ModelError;
use crate::types::instance::init_model_instance;
use std::collections::HashMap;
use std::sync::Arc;

pub const DEFAULT_MAX_RETRIES: u32 = 0;

pub struct VlloraLLMClient {
    instance: Arc<Box<dyn ModelInstance>>,
}

impl Default for VlloraLLMClient {
    fn default() -> Self {
        Self::new()
    }
}

impl VlloraLLMClient {
    pub fn new() -> Self {
        Self {
            instance: Arc::new(Box::new(DummyModelInstance {})),
        }
    }

    pub fn new_with_instance(instance: Box<dyn ModelInstance>) -> Self {
        Self {
            instance: Arc::new(instance),
        }
    }

    pub async fn new_with_engine_params_builder(
        engine_params_builder: CompletionEngineParamsBuilder,
    ) -> Result<Self, ModelError> {
        let instance =
            init_model_instance(engine_params_builder.build().unwrap(), HashMap::new()).await?;
        Ok(Self::new_with_instance(instance))
    }

    pub fn completions(&self) -> CompletionsClient {
        CompletionsClient::new(self.instance.clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::provider::tests::MockStreamServer;
    use crate::types::credentials::Credentials;
    use crate::types::engine::CompletionEngineParamsBuilder;
    use crate::types::provider::InferenceModelProvider;
    use async_openai::types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
    };
    use tokio_stream::StreamExt;

    use super::*;

    #[tokio::test]
    async fn test_new_with_instance_with_openai_compat() {
        let server = MockStreamServer::start()
            .await
            .expect("Failed to start mock server");
        let server_url = server.url();

        // Set some test events (you can modify these later)
        server.set_events(vec![
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-3.5-turbo-0125","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#.to_string(),
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-3.5-turbo-0125","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}"#.to_string(),
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-3.5-turbo-0125","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#.to_string(),
        ]).await;

        // Give the server a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let openai_req = CreateChatCompletionRequestArgs::default()
            .model("gpt-4.1-mini")
            .messages([
                ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content("You are a helpful assistant.")
                        .build()
                        .unwrap(),
                ),
                ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessageArgs::default()
                        .content("Explain Rust ownership in one sentence.")
                        .build()
                        .unwrap(),
                ),
            ])
            .build()
            .unwrap();

        let mut engine_params_builder = CompletionEngineParamsBuilder::new(
            InferenceModelProvider::Proxy("test".to_string()),
            openai_req.clone().into(),
        );

        engine_params_builder =
            engine_params_builder.with_credentials(Credentials::ApiKeyWithEndpoint {
                api_key: "test".to_string(),
                endpoint: server_url.clone(),
            });

        let mut stream = VlloraLLMClient::new_with_engine_params_builder(engine_params_builder)
            .await
            .unwrap()
            .completions()
            .create_stream(openai_req)
            .await
            .unwrap();

        while let Some(Ok(chunk)) = stream.next().await {
            println!("{:?}", chunk);
        }
    }
}
