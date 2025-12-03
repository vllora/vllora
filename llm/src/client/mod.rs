pub mod completions;
pub mod error;
pub mod message_mapper;
pub mod tools;

use crate::types::credentials::Credentials;
use crate::types::engine::CompletionEngineParamsBuilder;
pub use crate::types::instance::DummyModelInstance;
pub use crate::types::instance::ModelInstance;

use crate::client::completions::CompletionsClient;
use crate::types::models::InferenceProvider;
use crate::types::provider::InferenceModelProvider;

pub const DEFAULT_MAX_RETRIES: u32 = 0;

pub struct VlloraLLMClient {
    builder: CompletionEngineParamsBuilder,
}

impl Default for VlloraLLMClient {
    fn default() -> Self {
        Self::new()
    }
}

impl VlloraLLMClient {
    pub fn new() -> Self {
        Self {
            builder: CompletionEngineParamsBuilder::new(),
        }
    }

    pub fn new_with_engine_params_builder(
        engine_params_builder: CompletionEngineParamsBuilder,
    ) -> Self {
        Self {
            builder: engine_params_builder,
        }
    }

    pub fn mut_builder(&mut self) -> &mut CompletionEngineParamsBuilder {
        &mut self.builder
    }

    pub fn completions(&self) -> CompletionsClient {
        CompletionsClient::new(self.builder.clone())
    }

    pub fn with_provider(mut self, provider: InferenceProvider) -> Self {
        self.builder = self.builder.with_provider(provider);
        self
    }

    pub fn with_credentials(mut self, credentials: Credentials) -> Self {
        self.builder = self.builder.with_credentials(credentials);
        self
    }

    pub fn with_model_provider(mut self, model_provider: InferenceModelProvider) -> Self {
        self.builder = self.builder.with_model_provider(model_provider);
        self
    }

    pub fn with_inference_endpoint(mut self, inference_endpoint: String) -> Self {
        self.builder = self.builder.with_inference_endpoint(inference_endpoint);
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::provider::tests::MockStreamServer;
    use crate::types::credentials::ApiKeyCredentials;
    use crate::types::credentials::Credentials;
    use crate::types::engine::CompletionEngineParamsBuilder;
    use crate::types::models::InferenceProvider;
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

        let mut engine_params_builder = CompletionEngineParamsBuilder::new()
            .with_provider(InferenceProvider {
                provider: InferenceModelProvider::Proxy("test".to_string()),
                model_name: "test".to_string(),
                endpoint: Some(server_url.clone()),
            })
            .with_credentials(Credentials::ApiKey(ApiKeyCredentials {
                api_key: "test".to_string(),
            }));

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
