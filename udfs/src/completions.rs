use crate::types::{CommonResponse, CompletionConfig, Usage};
use crate::InvokeError;
use async_openai::types::{
    ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
    ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
    CreateChatCompletionRequestArgs,
};

pub async fn completions(
    input: String,
    config: &CompletionConfig,
) -> Result<CommonResponse, InvokeError> {
    let input = serde_json::from_str::<Vec<String>>(&input)?;
    let system_prompt = input[0].to_string();
    let input = input[1].to_string();
    // Create the message
    let message =
        async_openai::types::ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
            content: ChatCompletionRequestUserMessageContent::Text(input),
            name: None,
        });

    // Create the completion request with optional parameters
    let messages = [
        async_openai::types::ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(system_prompt),
                name: None,
            },
        ),
        message,
    ];
    let mut request = CreateChatCompletionRequestArgs::default();

    request = request.model(&config.model_settings.model).to_owned();
    request = request.messages(messages).to_owned();

    // Add optional parameters
    if let Some(fp) = config.model_settings.frequency_penalty {
        request = request.frequency_penalty(fp).to_owned();
    }
    if let Some(mt) = config.model_settings.max_tokens {
        request = request.max_tokens(mt as u32).to_owned();
    }
    if let Some(n) = config.model_settings.n {
        request = request.n(n).to_owned();
    }
    if let Some(pp) = config.model_settings.presence_penalty {
        request = request.presence_penalty(pp).to_owned();
    }
    if let Some(stop) = &config.model_settings.stop {
        request = request.stop([stop]).to_owned();
    }
    if let Some(seed) = config.model_settings.seed {
        request = request.seed(seed).to_owned();
    }

    let request = request.build()?;

    // Create client and send request
    let client = async_openai::Client::with_config(config.config.clone());
    let response = client.chat().create(request).await?;

    // Extract the first choice's message content
    let content = response
        .choices
        .first()
        .and_then(|choice| choice.message.content.clone())
        .unwrap_or_default();

    Ok(CommonResponse {
        response: content.into(),
        usage: Usage {
            total_tokens: response
                .usage
                .map_or(0, |usage| usage.total_tokens as usize),
        },
    })
}

#[cfg(test)]
mod tests {
    use crate::{completions::completions, types::CompletionConfig};

    #[tokio::test]
    async fn test_model() {
        let config =
            serde_json::from_str::<CompletionConfig>("{\"model\":\"gpt-4o-mini\"}").unwrap();
        let response = completions(
            "[\"you are a helpful assistant,\"what is the capital of the moon?\"]".to_string(),
            &config,
        )
        .await
        .unwrap();
        println!("{:?}", response);
    }
}
