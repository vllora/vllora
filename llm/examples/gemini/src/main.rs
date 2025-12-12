use vllora_llm::async_openai::types::{
    ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use tokio_stream::StreamExt;

use vllora_llm::client::VlloraLLMClient;
use vllora_llm::error::LLMResult;
use vllora_llm::types::credentials::{ApiKeyCredentials, Credentials};
use vllora_llm::types::provider::InferenceModelProvider;

#[tokio::main]
async fn main() -> LLMResult<()> {
    // 1) Build an OpenAI-style request using async-openai-compatible types
    //    (the gateway will route it to Gemini under the hood)
    let request = CreateChatCompletionRequestArgs::default()
        .model("gemini-2.0-flash-exp")
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

    // 2) Construct a VlloraLLMClient, configured to use Gemini
    let client = VlloraLLMClient::new()
        .with_credentials(Credentials::ApiKey(ApiKeyCredentials {
            api_key: std::env::var("VLLORA_GEMINI_API_KEY")
                .expect("VLLORA_GEMINI_API_KEY must be set"),
        }))
        .with_model_provider(InferenceModelProvider::Gemini);

    // 3) Non-streaming: send the request and print the final reply
    let response = client
        .completions()
        .create(request.clone())
        .await?;

    if let Some(content) = &response.message().content {
        if let Some(text) = content.as_string() {
            println!("Non-streaming Gemini reply:");
            println!("{text}");
        }
    }

    // 4) Streaming: send the same request and print chunks as they arrive
    let mut stream = client
        .completions()
        .create_stream(request)
        .await?;

    println!("Streaming Gemini response...");

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
