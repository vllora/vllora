use crate::types::{CompletionConfig, GatewayConfig, ModelSettings};
use crate::InvokeError;
use async_openai::types::{
    ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
    CreateChatCompletionRequestArgs,
};
use clap::{Parser, Subcommand};
use tracing::{debug, error};

pub async fn completions(input: String) -> Result<String, InvokeError> {
    debug!("Received input: {}", input);

    // Get the config
    let config = match parse_completion_config() {
        Ok(config) => {
            debug!("Parsed config successfully: {:?}", config);
            config
        }
        Err(e) => {
            error!("Failed to parse config: {:?}", e);
            return Err(InvokeError::CustomError(e.to_string()));
        }
    };

    completions_with_config(input, config).await
}

async fn completions_with_config(
    input: String,
    config: CompletionConfig,
) -> Result<String, InvokeError> {
    // Create the message
    let message =
        async_openai::types::ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
            content: ChatCompletionRequestUserMessageContent::Text(input),
            name: None,
        });

    // Create the completion request with optional parameters
    let messages = [message];
    let mut request = CreateChatCompletionRequestArgs::default();

    request = request.model(&config.model_settings.model).to_owned();
    request = request.messages(messages).to_owned();

    // Add optional parameters
    if let Some(fp) = config.model_settings.frequency_penalty {
        request = request.frequency_penalty(fp).to_owned();
    }
    if let Some(mt) = config.model_settings.max_tokens {
        request = request.max_tokens(mt).to_owned();
    }
    if let Some(n) = config.model_settings.n {
        request = request.n(n).to_owned();
    }
    if let Some(pp) = config.model_settings.presence_penalty {
        request = request.presence_penalty(pp).to_owned();
    }
    if let Some(stop) = config.model_settings.stop {
        request = request.stop([stop]).to_owned();
    }
    if let Some(seed) = config.model_settings.seed {
        request = request.seed(seed).to_owned();
    }

    let request = request.build()?;

    // Create client and send request
    let client = async_openai::Client::with_config(config.config);
    let response = client.chat().create(request).await?;

    // Extract the first choice's message content
    let content = response
        .choices
        .first()
        .and_then(|choice| choice.message.content.clone())
        .unwrap_or_default();

    Ok(content)
}

#[derive(Parser, Debug)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Completions(CompletionConfigArgs),
}

#[derive(Parser, Debug)]
pub struct CompletionConfigArgs {
    // GatewayConfig fields
    #[arg(long = "api-base")]
    api_base: String,

    #[arg(long = "api-key")]
    api_key: String,

    #[arg(long = "project-id")]
    project_id: String,

    // ModelSettings fields
    #[arg(long, default_value = "gpt-3.5-turbo")]
    model: String,

    #[arg(long = "frequency-penalty", value_parser = parse_optional_f32)]
    frequency_penalty: Option<f32>,

    #[arg(long = "max-tokens", value_parser = parse_optional_u32)]
    max_tokens: Option<u32>,

    #[arg(long, value_parser = parse_optional_u8)]
    n: Option<u8>,

    #[arg(long = "presence-penalty", value_parser = parse_optional_f32)]
    presence_penalty: Option<f32>,

    #[arg(long)]
    stop: Option<String>,

    #[arg(long, value_parser = parse_optional_i64)]
    seed: Option<i64>,
}

fn parse_optional_u32(arg: &str) -> Result<Option<u32>, String> {
    if arg.is_empty() || arg.starts_with("${") {
        Ok(None)
    } else {
        arg.parse::<u32>().map(Some).map_err(|e| e.to_string())
    }
}

fn parse_optional_u8(arg: &str) -> Result<Option<u8>, String> {
    if arg.is_empty() || arg.starts_with("${") {
        Ok(None)
    } else {
        arg.parse::<u8>().map(Some).map_err(|e| e.to_string())
    }
}

fn parse_optional_i64(arg: &str) -> Result<Option<i64>, String> {
    if arg.is_empty() || arg.starts_with("${") {
        Ok(None)
    } else {
        arg.parse::<i64>().map(Some).map_err(|e| e.to_string())
    }
}

fn parse_optional_f32(arg: &str) -> Result<Option<f32>, String> {
    if arg.is_empty() || arg.starts_with("${") {
        Ok(None)
    } else {
        arg.parse::<f32>().map(Some).map_err(|e| e.to_string())
    }
}

impl CompletionConfigArgs {
    fn into_completion_config(self) -> CompletionConfig {
        let model_settings = ModelSettings {
            model: self.model,
            frequency_penalty: self.frequency_penalty,
            logit_bias: None,
            logprobs: None,
            top_logprobs: None,
            max_tokens: self.max_tokens,
            n: self.n,
            presence_penalty: self.presence_penalty,
            response_format: None,
            seed: self.seed,
            stop: self.stop,
        };

        let mut config = GatewayConfig::new();
        if !self.api_base.starts_with("${") {
            config = config.with_api_base(self.api_base);
        }
        if !self.api_key.starts_with("${") {
            config = config.with_api_key(self.api_key);
        }
        if !self.project_id.starts_with("${") {
            config = config.with_project_id(self.project_id);
        }

        CompletionConfig {
            config,
            model_settings,
        }
    }
}

pub fn parse_completion_config() -> Result<CompletionConfig, clap::Error> {
    debug!(
        "Parsing CLI arguments: {:?}",
        std::env::args().collect::<Vec<_>>()
    );

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            error!("Failed to parse CLI arguments: {}", e);
            return Err(e);
        }
    };

    Ok(match cli.command {
        Commands::Completions(args) => {
            debug!("Parsed CompletionConfigArgs: {:?}", args);
            args.into_completion_config()
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use async_openai::types::{
        ChatChoice, ChatCompletionResponseMessage, CreateChatCompletionRequest,
        CreateChatCompletionResponse,
    };
    use mockall::predicate::*;
    use mockall::*;

    mock! {
        Client {
            fn clone(&self) -> Self;
            fn chat(&self) -> MockChat;
        }
    }

    mock! {
        Chat {
            fn clone(&self) -> Self;
            fn create<'a>(&'a self, _request: CreateChatCompletionRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<CreateChatCompletionResponse, async_openai::error::OpenAIError>> + Send + 'a>>;
        }
    }

    #[tokio::test]
    async fn test_completions_basic() {
        let test_input = "Hello, world!".to_string();
        let expected_output = "Test response".to_string();

        // Create a mock response
        let response = Arc::new(CreateChatCompletionResponse {
            choices: vec![ChatChoice {
                #[allow(deprecated)]
                message: ChatCompletionResponseMessage {
                    content: Some(expected_output.clone()),
                    role: async_openai::types::Role::Assistant,
                    function_call: None,
                    tool_calls: None,
                    refusal: None,
                },
                finish_reason: Some(async_openai::types::FinishReason::Stop),
                index: 0,
                logprobs: None,
            }],
            created: 0,
            id: "test".to_string(),
            model: "gpt-3.5-turbo".to_string(),
            object: "chat.completion".to_string(),
            system_fingerprint: None,
            usage: None,
        });

        // Mock the client
        let response_clone = response.clone();
        let mut mock_chat = MockChat::new();
        mock_chat.expect_create().returning(move |_| {
            let response_clone = response_clone.clone();
            Box::pin(async move { Ok((*response_clone).clone()) })
        });

        let mut mock_client = MockClient::new();
        mock_client
            .expect_chat()
            .returning(move || mock_chat.clone());

        // Run the test
        let result = completions(test_input).await;

        // Verify the result
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected_output);
    }

    #[test]
    fn test_parse_completion_config() {
        // Test with command line arguments
        let args = vec![
            "program",
            "completions", // Add subcommand
            "--model",
            "gpt-4",
            "--api-key",
            "test-key",
            "--max-tokens",
            "100",
            "--frequency-penalty",
            "0.5",
        ];

        // Override command line arguments for testing
        let cli = Cli::parse_from(args);
        let completion_config = match cli.command {
            Commands::Completions(args) => args.into_completion_config(),
        };

        assert_eq!(completion_config.model_settings.model, "gpt-4");
        assert_eq!(completion_config.model_settings.max_tokens, Some(100));
        assert_eq!(
            completion_config.model_settings.frequency_penalty,
            Some(0.5)
        );
    }
}
