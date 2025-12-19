use crate::client::completions::response_stream::ResultStream;
use crate::client::error::ModelError;
use crate::client::responses::Responses;
use crate::error::LLMError;
use crate::error::LLMResult;
use crate::provider::anthropic::AnthropicModel;
use crate::provider::bedrock::BedrockModel;
use crate::provider::gemini::GeminiModel;
use crate::provider::openai::completions::OpenAIModel;
use crate::provider::openai::responses::OpenAIResponses;
use crate::provider::proxy::OpenAISpecModel;
use crate::types::credentials_ident::CredentialsIdent;
use crate::types::engine::CompletionEngineParams;
use crate::types::engine::ResponsesEngineParams;
use crate::types::gateway::ChatCompletionChunk;
use crate::types::gateway::ChatCompletionChunkChoice;
use crate::types::gateway::ChatCompletionContent;
use crate::types::gateway::ChatCompletionDelta;
use crate::types::gateway::ChatCompletionMessage;
use crate::types::gateway::ChatCompletionMessageWithFinishReason;
use crate::types::message::Message;
use crate::types::tools::Tool;
use crate::types::LLMContentEvent;
use crate::types::LLMFinishEvent;
use crate::types::ModelEvent;
use crate::types::ModelEventType;
use crate::types::ModelFinishReason;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
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
    ) -> LLMResult<ResultStream>;
}

pub async fn init_responses_model_instance(
    engine: ResponsesEngineParams,
    _tools: HashMap<String, Arc<Box<dyn Tool + 'static>>>,
) -> Result<Box<dyn Responses>, ModelError> {
    let instance = match engine {
        ResponsesEngineParams::OpenAi { credentials } => {
            Box::new(OpenAIResponses::new(credentials.as_ref(), None)?) as Box<dyn Responses>
        }
    };

    Ok(instance)
}

pub async fn init_model_instance(
    engine: CompletionEngineParams,
    tools: HashMap<String, Arc<Box<dyn Tool + 'static>>>,
) -> Result<Box<dyn ModelInstance>, ModelError> {
    let instance = match engine {
        CompletionEngineParams::OpenAi {
            params,
            execution_options,
            credentials,
            endpoint,
        } => {
            // Check if the endpoint is an Azure OpenAI endpoint
            if let Some(ep) = endpoint.as_ref() {
                if ep.contains("azure.com") {
                    // Use the Azure implementation
                    return Ok(Box::new(OpenAIModel::from_azure_url(
                        params.clone(),
                        credentials.as_ref(),
                        execution_options.clone(),
                        tools,
                        ep,
                    )?));
                }
            }

            Box::new(OpenAIModel::new(
                params.clone(),
                credentials.as_ref(),
                execution_options.clone(),
                tools,
                None,
                endpoint.as_deref(),
            )?) as Box<dyn ModelInstance>
        }
        CompletionEngineParams::Bedrock {
            params,
            execution_options,
            credentials,
            ..
        } => Box::new(
            BedrockModel::new(
                params.clone(),
                execution_options.clone(),
                credentials.as_ref(),
                tools,
            )
            .await?,
        ) as Box<dyn ModelInstance>,
        CompletionEngineParams::Anthropic {
            params,
            execution_options,
            credentials,
            endpoint,
        } => Box::new(AnthropicModel::new(
            params.clone(),
            execution_options.clone(),
            credentials.as_ref(),
            tools,
            endpoint,
        )?) as Box<dyn ModelInstance>,
        CompletionEngineParams::Gemini {
            params,
            execution_options,
            credentials,
            api_url,
            ..
        } => Box::new(GeminiModel::new(
            params.clone(),
            execution_options.clone(),
            credentials.as_ref(),
            tools,
            api_url,
        )?) as Box<dyn ModelInstance>,
        CompletionEngineParams::Proxy {
            params,
            execution_options,
            credentials,
            provider_name,
            endpoint,
        } => Box::new(OpenAISpecModel::new(
            params.clone(),
            credentials.as_ref(),
            execution_options.clone(),
            tools,
            endpoint.as_deref(),
            &provider_name,
        )?) as Box<dyn ModelInstance>,
    };

    Ok(instance)
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
    ) -> LLMResult<ResultStream> {
        let (tx_response, rx_response) = tokio::sync::mpsc::channel(10000);
        tokio::spawn(async move {
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
                    tx_response
                        .send(Ok(ChatCompletionChunk {
                            id: "test".to_string(),
                            object: "test".to_string(),
                            created: 0,
                            model: "test".to_string(),
                            choices: vec![ChatCompletionChunkChoice {
                                index: 0,
                                delta: ChatCompletionDelta {
                                    content: Some(chunk.to_owned()),
                                    role: Some("assistant".to_string()),
                                    tool_calls: None,
                                },
                                finish_reason: Some("stop".to_string()),
                                logprobs: None,
                            }],
                            usage: None,
                        }))
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
            .map_err(|e| LLMError::CustomError(e.to_string()))
            .unwrap();
        });

        Ok(ResultStream::create(rx_response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::completions::CompletionsClient;
    use crate::types::engine::CompletionEngineParamsBuilder;
    use crate::types::gateway::ChatCompletionChunk;
    use crate::types::gateway::ChatCompletionRequest;
    use async_openai::types::CreateChatCompletionResponse;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_create() {
        let client = CompletionsClient::new(CompletionEngineParamsBuilder::new())
            .with_instance(Box::new(DummyModelInstance {}));
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
        let client = CompletionsClient::new(CompletionEngineParamsBuilder::new())
            .with_instance(Box::new(DummyModelInstance {}));
        let request = ChatCompletionRequest {
            model: "test".to_string(),
            messages: vec![ChatCompletionMessage {
                role: "user".to_string(),
                content: Some(ChatCompletionContent::Text("Hello, world!".to_string())),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut response = client.create_stream(request).await.unwrap();
        let mut chunks = vec![];
        while let Some(chunk) = response.next().await {
            chunks.push(chunk);
        }
        assert_eq!(5, chunks.len());
        assert_eq!(
            "Hello, world!",
            chunks
                .iter()
                .filter_map(|c| {
                    let chunk: ChatCompletionChunk = c.as_ref().unwrap().clone().into();
                    chunk.choices[0].delta.content.as_ref().map(|c| c.clone())
                })
                .collect::<Vec<String>>()
                .join("")
        );

        let last_chunk: ChatCompletionChunk =
            chunks.last().unwrap().as_ref().unwrap().clone().into();
        assert_eq!(
            Some(ModelFinishReason::Stop.to_string()),
            last_chunk.choices[0].finish_reason
        );
        assert_eq!("test", last_chunk.model);
    }
}
