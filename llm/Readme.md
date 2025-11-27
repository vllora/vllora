## Vllora LLM crate (`vllora_llm`)

This crate powers the Vllora AI Gateway’s LLM layer. It provides:

- **Unified chat-completions client** over multiple providers (OpenAI-compatible, Anthropic, Gemini, Bedrock, …)
- **Gateway-native types** (`ChatCompletionRequest`, `ChatCompletionMessage`, routing & tools support)
- **Streaming responses and telemetry hooks** via a common `ModelInstance` trait

Use it when you want to talk to the gateway’s LLM engine from Rust code, without worrying about provider-specific SDKs.

---

## Using the async-openai-compatible types (streaming)

If you already build OpenAI-compatible requests (e.g. via `async-openai-compat`), you can stream completions through the same engine setup the gateway uses (`CompletionEngineParamsBuilder`).

```rust
use async_openai::types::{
    ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs,
    ChatCompletionRequestMessage,
    CreateChatCompletionRequestArgs,
};
use tokio_stream::StreamExt;

use vllora_llm::client::VlloraLLMClient;
use vllora_llm::types::credentials::Credentials;
use vllora_llm::types::engine::CompletionEngineParamsBuilder;
use vllora_llm::types::gateway::ChatCompletionRequest;
use vllora_llm::types::provider::InferenceModelProvider;
use vllora_llm::error::LLMResult;

#[tokio::main]
async fn main() -> LLMResult<()> {
    // 1) Build an OpenAI-style request using async-openai-compatible types
    let openai_req = CreateChatCompletionRequestArgs::default()
        .model("gpt-4.1-mini")
        .messages([
            ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content("You are a helpful assistant.")
                    .build()?,
            ),
            ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessageArgs::default()
                    .content("Stream numbers 1 to 3, comma-separated.")
                    .build()?,
            ),
        ])
        .build()?;

    // 2) Build engine params the same way the gateway core does
    let engine_params_builder = CompletionEngineParamsBuilder::new(
        // Use OpenAI directly, or Proxy("name") for a routed/provider alias
        InferenceModelProvider::OpenAI,
        ChatCompletionRequest::from(openai_req.clone()),
    )
    .with_credentials(Credentials::ApiKeyWithEndpoint {
        api_key: std::env::var("VLLORA_OPENAI_API_KEY")
            .expect("VLLORA_OPENAI_API_KEY must be set"),
        endpoint: "https://api.openai.com/v1".to_string(),
    });

    // 3) Construct a VlloraLLMClient from the engine params builder
    let client = VlloraLLMClient::new_with_engine_params_builder(engine_params_builder)
        .await?;

    // 4) Stream the completion using the original OpenAI-style request
    let mut stream = client
        .completions()
        .create_stream(openai_req.into())
        .await?;

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
```

---

## Basic usage: completions client (gateway-native)

The main entrypoint is `VlloraLLMClient`, which gives you a `CompletionsClient` for chat completions using the gateway-native request/response types.

```rust
use std::sync::Arc;

use vllora_llm::client::{VlloraLLMClient, ModelInstance, DummyModelInstance};
use vllora_llm::types::gateway::{ChatCompletionRequest, ChatCompletionMessage};
use vllora_llm::error::LLMResult;

#[tokio::main]
async fn main() -> LLMResult<()> {
    // In production you would pass a real ModelInstance implementation
    // that knows how to call your configured providers / router.
    let instance: Arc<Box<dyn ModelInstance>> = Arc::new(Box::new(DummyModelInstance {}));

    // Build the high-level client
    let client = VlloraLLMClient::new_with_instance(instance);

    // Build a simple chat completion request
    let request = ChatCompletionRequest {
        model: "gpt-4.1-mini".to_string(), // or any gateway-configured model id
        messages: vec![
            ChatCompletionMessage::new_text(
                "system".to_string(),
                "You are a helpful assistant.".to_string(),
            ),
            ChatCompletionMessage::new_text(
                "user".to_string(),
                "Say hello in one short sentence.".to_string(),
            ),
        ],
        ..Default::default()
    };

    // Send the request and get a single response message
    let response = client.completions().create(request).await?;

    let message = response.message();
    if let Some(content) = &message.content {
        if let Some(text) = content.as_string() {
            println!("Model reply: {text}");
        }
    }

    Ok(())
}
```

Key pieces:

- **`VlloraLLMClient`**: wraps a `ModelInstance` and exposes `.completions()`.
- **`CompletionsClient::create`**: sends a one-shot completion request and returns a `ChatCompletionMessageWithFinishReason`.
- **Gateway types** (`ChatCompletionRequest`, `ChatCompletionMessage`) abstract over provider-specific formats.

---

## Streaming completions

`CompletionsClient::create_stream` returns a `ResultStream` that yields streaming chunks:

```rust
use std::sync::Arc;

use vllora_llm::client::{VlloraLLMClient, ModelInstance, DummyModelInstance};
use vllora_llm::types::gateway::{ChatCompletionRequest, ChatCompletionMessage};
use vllora_llm::error::LLMResult;

#[tokio::main]
async fn main() -> LLMResult<()> {
    let instance: Arc<Box<dyn ModelInstance>> = Arc::new(Box::new(DummyModelInstance {}));
    let client = VlloraLLMClient::new_with_instance(instance);

    let request = ChatCompletionRequest {
        model: "gpt-4.1-mini".to_string(),
        messages: vec![ChatCompletionMessage::new_text(
            "user".to_string(),
            "Stream the alphabet, one chunk at a time.".to_string(),
        )],
        ..Default::default()
    };

    let mut stream = client.completions().create_stream(request).await?;

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
```

The stream API mirrors OpenAI-style streaming but uses gateway-native `ChatCompletionChunk` types.

---

## Notes

- **Real usage**: In the full LangDB / Vllora gateway, concrete `ModelInstance` implementations are created by the core executor based on your `models.yaml` and routing rules; the examples above use `DummyModelInstance` only to illustrate the public API of the `CompletionsClient`.
- **Error handling**: All client methods return `LLMResult<T>`, which wraps rich `LLMError` variants (network, mapping, provider errors, etc.).
- **More features**: The same types in `vllora_llm::types::gateway` are used for tools, MCP, routing, embeddings, and image generation; see the main repository docs at `https://vllora.dev/docs` for higher-level gateway features.


