use vllora_llm::async_openai::types::{
    ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use tokio_stream::StreamExt;

use vllora_llm::client::VlloraLLMClient;
use vllora_llm::error::LLMResult;
use vllora_llm::types::credentials::{AwsApiKeyCredentials, BedrockCredentials, Credentials};
use vllora_llm::types::provider::InferenceModelProvider;

#[tokio::main]
async fn main() -> LLMResult<()> {
    // 1) Build an OpenAI-style request using async-openai-compatible types
    //    (the gateway will route it to Bedrock under the hood)
    let request = CreateChatCompletionRequestArgs::default()
        // Example Bedrock model ID (update to whatever you use)
        .model("us.amazon.nova-micro-v1:0")
        .messages([
            ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content("You are a helpful assistant that streams responses.")
                    .build()?,
            ),
            ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessageArgs::default()
                    .content("Stream numbers 1 to 10, one per line.")
                    .build()?,
            ),
        ])
        .build()?;

    // 2) Construct a VlloraLLMClient, configured to use Bedrock
    let api_key = std::env::var("VLLORA_BEDROCK_API_KEY")
        .expect("VLLORA_BEDROCK_API_KEY must be set (Bedrock API key)");
    let region = std::env::var("AWS_DEFAULT_REGION").unwrap_or_else(|_| "us-west-2".to_string());

    let client = VlloraLLMClient::new()
        .with_credentials(Credentials::Aws(BedrockCredentials::ApiKey(
            AwsApiKeyCredentials {
                api_key,
                region: Some(region),
            },
        )))
        .with_model_provider(InferenceModelProvider::Bedrock);

    // 3) Non-streaming: send the request and print the final reply
    let response = client
        .completions()
        .create(request.clone())
        .await?;

    if let Some(content) = &response.message().content {
        if let Some(text) = content.as_string() {
            println!("Non-streaming Bedrock reply:");
            println!("{text}");
        }
    }

    // 4) Streaming: send the same request and print chunks as they arrive
    let mut stream = client
        .completions()
        .create_stream(request)
        .await?;

    println!("Streaming Bedrock response...");

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        for choice in chunk.choices {
            if let Some(delta) = choice.delta.content {
                print!("{delta}");
            }
        }
    }

    Ok(())
}
