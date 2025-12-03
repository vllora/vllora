use async_openai_compat::types::{
    ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use tokio_stream::StreamExt;

use vllora_llm::client::VlloraLLMClient;
use vllora_llm::error::LLMResult;
use vllora_llm::types::credentials::Credentials;
use vllora_llm::types::engine::CompletionEngineParamsBuilder;
use vllora_llm::types::gateway::ChatCompletionRequest;
use vllora_llm::types::provider::InferenceModelProvider;
use vllora_llm::types::credentials::ApiKeyCredentials;

#[tokio::main]
async fn main() -> LLMResult<()> {
    // 1) Build an OpenAI-style request using async-openai-compatible types
    //    This will go through the LangDB proxy (OpenAI-compatible endpoint).
    let openai_req = CreateChatCompletionRequestArgs::default()
        .model("gpt-4.1-mini")
        .messages([
            ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content("You are a helpful assistant that streams responses via LangDB.")
                    .build()?,
            ),
            ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessageArgs::default()
                    .content("Stream numbers 1 to 10, one per line.")
                    .build()?,
            ),
        ])
        .build()?;

    // Env vars:
    // - VLLORA_LANGDB_API_KEY: API key for your LangDB deployment
    // - VLLORA_LANGDB_OPENAI_BASE_URL: base URL of the LangDB OpenAI-compatible endpoint. 
    let langdb_api_key = std::env::var("VLLORA_LANGDB_API_KEY")
        .expect("VLLORA_LANGDB_API_KEY must be set (LangDB API key)");
    let langdb_base_url = std::env::var("VLLORA_LANGDB_OPENAI_BASE_URL")
        .unwrap_or_else(|_| "https://api.us-east-1.langdb.ai/v1".to_string());

    // 2) Construct a VlloraLLMClient pointing at the LangDB proxy
    let client = VlloraLLMClient::new()
        .with_model_provider(InferenceModelProvider::Proxy("langdb".to_string()))
        .with_inference_endpoint(langdb_base_url)
        .with_credentials(Credentials::ApiKey(ApiKeyCredentials {
            api_key: langdb_api_key,
        }));

    // 3) Non-streaming: send the request and print the final reply
    let response = client
        .completions()
        .create(openai_req.clone())
        .await?;

    if let Some(content) = &response.message().content {
        if let Some(text) = content.as_string() {
            println!("Non-streaming Proxy(LangDB) reply:");
            println!("text}");
        }
    }

    // 4) Streaming: send the same request and print chunks as they arrive
    let mut stream = client
        .completions()
        .create_stream(openai_req)
        .await?;

    println!("Streaming Proxy(LangDB) response...");

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
