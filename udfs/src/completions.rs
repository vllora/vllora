use crate::types::{CommonResponse, CompletionConfig, Usage};
use crate::InvokeError;
use async_openai::types::{
    ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
    ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
    CreateChatCompletionRequestArgs,
};

pub async fn completions(
    values: &mut std::slice::Iter<'_, String>,
    config: &CompletionConfig,
) -> Result<CommonResponse, InvokeError> {
    let system_prompt = values.next().cloned();
    let input = values
        .next()
        .cloned()
        .ok_or_else(|| InvokeError::CustomError("No input provided".to_string()))?;
    tracing::debug!("Calling completions with input: {}", input);

    // Create the message
    let message =
        async_openai::types::ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
            content: ChatCompletionRequestUserMessageContent::Text(input),
            name: None,
        });

    let mut messages = vec![];
    if let Some(system_prompt) = system_prompt {
        messages.push(async_openai::types::ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(system_prompt),
                name: None,
            },
        ));
    }

    messages.push(message);
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

    // Create client and send request with retries
    let client = async_openai::Client::with_config(config.config.clone());
    let mut retries = 0;
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 1000;

    let response = loop {
        match client.chat().create(request.clone()).await {
            Ok(resp) => break resp,
            Err(e) => {
                if retries >= MAX_RETRIES {
                    return Err(InvokeError::Other(format!(
                        "Failed after {MAX_RETRIES} retries: {e}"
                    )));
                }
                retries += 1;
                tokio::time::sleep(tokio::time::Duration::from_millis(
                    RETRY_DELAY_MS * retries as u64,
                ))
                .await;
                continue;
            }
        }
    };

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
        let items = [
            "you are a helpful assistant".to_string(),
            "what is the capital of the moon?".to_string(),
        ];

        let response = completions(&mut items.iter(), &config).await.unwrap();
        println!("{response:?}");
    }
}
