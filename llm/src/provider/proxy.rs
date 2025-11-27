use crate::client::completions::response_stream::ResultStream;
use crate::client::error::ModelError;
use crate::error::LLMResult;
use crate::provider::openai::OpenAIModel;
use crate::provider::openai_spec_client::openai_spec_client;
use crate::types::credentials::ApiKeyCredentials;
use crate::types::engine::ExecutionOptions;
use crate::types::engine::OpenAiModelParams;
use crate::types::gateway::ChatCompletionMessageWithFinishReason;
use crate::types::instance::ModelInstance;
use crate::types::message::Message;
use crate::types::tools::Tool;
use crate::types::ModelEvent;
use async_openai::config::OpenAIConfig;
use async_openai::Client;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::Span;
use tracing_futures::Instrument;

#[derive(Clone)]
pub struct OpenAISpecModel {
    openai_model: OpenAIModel<OpenAIConfig>,
}

impl OpenAISpecModel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mut params: OpenAiModelParams,
        credentials: Option<&ApiKeyCredentials>,
        execution_options: ExecutionOptions,
        tools: HashMap<String, Arc<Box<dyn Tool>>>,
        endpoint: Option<&str>,
        provider_name: &str,
    ) -> Result<Self, ModelError> {
        if provider_name == "togetherai" {
            if let Some(model_name) = &params.model {
                if params.max_tokens.is_none() && model_name.starts_with("google/gemma-2") {
                    // restrict max tokens because of bug in togetherai
                    params.max_tokens = Some(4096);
                }
            }
        }

        let client: Client<OpenAIConfig> =
            openai_spec_client(credentials, endpoint, provider_name)?;
        let openai_model = OpenAIModel::new(
            params,
            credentials,
            execution_options,
            tools,
            Some(client),
            None,
        )?;

        Ok(Self { openai_model })
    }
}

#[async_trait]
impl ModelInstance for OpenAISpecModel {
    async fn invoke(
        &self,
        input_variables: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> LLMResult<ChatCompletionMessageWithFinishReason> {
        let span = Span::current();
        self.openai_model
            .invoke(input_variables, tx, previous_messages, tags)
            .instrument(span.clone())
            .await
    }

    async fn stream(
        &self,
        input_variables: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> LLMResult<ResultStream> {
        let span = Span::current();
        self.openai_model
            .stream(input_variables, tx, previous_messages, tags)
            .instrument(span.clone())
            .await
    }
}
