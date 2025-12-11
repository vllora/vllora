pub mod completions;
pub mod error;
pub mod message_mapper;
pub mod responses;
pub mod tools;

use crate::client::responses::ResponsesClient;
use crate::types::credentials::Credentials;
use crate::types::engine::CompletionEngineParamsBuilder;
use crate::types::engine::ResponsesEngineParamsBuilder;
pub use crate::types::instance::DummyModelInstance;
pub use crate::types::instance::ModelInstance;

use crate::client::completions::CompletionsClient;
use crate::types::models::InferenceProvider;
use crate::types::provider::InferenceModelProvider;

pub const DEFAULT_MAX_RETRIES: u32 = 0;

#[derive(Default)]
pub struct VlloraLLMClientBuilderParams {
    provider: Option<InferenceProvider>,
    credentials: Option<Credentials>,
    model_provider: Option<InferenceModelProvider>,
    inference_endpoint: Option<String>,
}

#[derive(Default)]
pub struct VlloraLLMClientBuilder {
    params: VlloraLLMClientBuilderParams,
}

impl VlloraLLMClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_provider(mut self, provider: InferenceProvider) -> Self {
        self.params.provider = Some(provider);
        self
    }

    pub fn with_credentials(mut self, credentials: Credentials) -> Self {
        self.params.credentials = Some(credentials);
        self
    }

    pub fn with_model_provider(mut self, model_provider: InferenceModelProvider) -> Self {
        self.params.model_provider = Some(model_provider);
        self
    }

    pub fn with_inference_endpoint(mut self, inference_endpoint: String) -> Self {
        self.params.inference_endpoint = Some(inference_endpoint);
        self
    }

    pub fn build(self) -> VlloraLLMClient {
        VlloraLLMClient::new(self.params)
    }
}

pub struct VlloraLLMClient {
    params: VlloraLLMClientBuilderParams,
}

impl Default for VlloraLLMClient {
    fn default() -> Self {
        Self::new(VlloraLLMClientBuilderParams::default())
    }
}

impl VlloraLLMClient {
    pub fn new(params: VlloraLLMClientBuilderParams) -> Self {
        Self { params }
    }

    pub fn new_with_engine_params_builder(params: VlloraLLMClientBuilderParams) -> Self {
        Self { params }
    }

    pub fn completions(&self) -> CompletionsClient {
        let mut builder = CompletionEngineParamsBuilder::new();

        if let Some(provider) = &self.params.provider {
            builder = builder.with_provider(provider.clone());
        }
        if let Some(credentials) = &self.params.credentials.clone() {
            builder = builder.with_credentials(credentials.clone());
        }
        if let Some(model_provider) = &self.params.model_provider.clone() {
            builder = builder.with_model_provider(model_provider.clone());
        }
        if let Some(inference_endpoint) = self.params.inference_endpoint.clone() {
            builder = builder.with_inference_endpoint(inference_endpoint);
        }

        CompletionsClient::new(builder)
    }

    pub fn responses(&self) -> ResponsesClient {
        let mut builder = ResponsesEngineParamsBuilder::new();

        if let Some(provider) = &self.params.provider {
            builder = builder.with_provider(provider.clone());
        }
        if let Some(credentials) = &self.params.credentials.clone() {
            builder = builder.with_credentials(credentials.clone());
        }

        ResponsesClient::new(builder)
    }

    pub fn with_provider(mut self, provider: InferenceProvider) -> Self {
        self.params.provider = Some(provider);
        self
    }

    pub fn with_credentials(mut self, credentials: Credentials) -> Self {
        self.params.credentials = Some(credentials);
        self
    }

    pub fn with_model_provider(mut self, model_provider: InferenceModelProvider) -> Self {
        self.params.model_provider = Some(model_provider);
        self
    }

    pub fn with_inference_endpoint(mut self, inference_endpoint: String) -> Self {
        self.params.inference_endpoint = Some(inference_endpoint);
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::provider::tests::MockStreamServer;
    use crate::types::credentials::ApiKeyCredentials;
    use crate::types::credentials::Credentials;
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

        let client = VlloraLLMClientBuilder::default()    
            .with_provider(InferenceProvider {
                provider: InferenceModelProvider::Proxy("test".to_string()),
                model_name: "test".to_string(),
                endpoint: Some(server_url.clone()),
            })
            .with_credentials(Credentials::ApiKey(ApiKeyCredentials {
                api_key: "test".to_string(),
            }))
            .build();

        let mut stream = client
            .completions()
            .create_stream(openai_req)
            .await
            .unwrap();

        while let Some(Ok(chunk)) = stream.next().await {
            println!("{:?}", chunk);
        }
    }
}
