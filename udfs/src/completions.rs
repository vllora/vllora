use crate::types::CompletionConfig;
use crate::InvokeError;
use async_openai::types::{
    ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
    CreateChatCompletionRequestArgs,
};
use clap::{Parser, Subcommand};
use serde_json;
use tracing::{debug, error};

pub async fn completions(input: String, last: &str) -> Result<String, InvokeError> {
    debug!("Received input: {}", input);

    // Get the config
    let config = match parse_completion_config(last) {
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
    #[arg(long)]
    config: String, // This will contain the JSON string of all parameters
}

pub fn parse_completion_config(last: &str) -> Result<CompletionConfig, InvokeError> {
    let config: CompletionConfig = serde_json::from_str(last)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use crate::completions;

    #[tokio::test]
    async fn test_model() {
        let response = completions::completions("sdfs".to_string(), "{}")
            .await
            .unwrap();
        println!("{response}");
    }
}
