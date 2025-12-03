## Vllora LLM crate (`vllora_llm`)

![Crates.io](https://img.shields.io/crates/v/vllora_llm)

This crate powers the Vllora AI Gateway’s LLM layer. It provides:

- **Unified chat-completions client** over multiple providers (OpenAI-compatible, Anthropic, Gemini, Bedrock, …)
- **Gateway-native types** (`ChatCompletionRequest`, `ChatCompletionMessage`, routing & tools support)
- **Streaming responses and telemetry hooks** via a common `ModelInstance` trait
- **Tracing integration**: out-of-the-box `tracing` support, with a console example in `llm/examples/tracing` (spans/events to stdout) and an OTLP example in `llm/examples/tracing_otlp` (send spans to external collectors such as New Relic)
- **Supported parameters**: See [Supported parameters](#supported-parameters) for a detailed table of which parameters are honored by each provider

Use it when you want to talk to the gateway’s LLM engine from Rust code, without worrying about provider-specific SDKs.

---

## Installation

Run `cargo add vllora_llm` or add to your `Cargo.toml`:

```toml
[dependencies]
vllora_llm = "0.1"
```

---

## Quick start

Here's a minimal example to get started:

```rust
use vllora_llm::client::VlloraLLMClient;
use vllora_llm::types::gateway::{ChatCompletionRequest, ChatCompletionMessage};
use vllora_llm::error::LLMResult;

#[tokio::main]
async fn main() -> LLMResult<()> {
    // 1) Build a chat completion request using gateway-native types
    let request = ChatCompletionRequest {
        model: "gpt-4.1-mini".to_string(),
        messages: vec![
            ChatCompletionMessage::new_text(
                "system".to_string(),
                "You are a helpful assistant.".to_string(),
            ),
            ChatCompletionMessage::new_text(
                "user".to_string(),
                "Stream numbers 1 to 20 in separate lines.".to_string(),
            ),
        ],
        ..Default::default()
    };

    // 2) Construct a VlloraLLMClient
    let client = VlloraLLMClient::new();

    // 3) Non-streaming: send the request and print the final reply
    let response = client
        .completions()
        .create(request.clone())
        .await?;
    
    // ... handle response
    Ok(())
}
```

**Note**: By default, `VlloraLLMClient::new()` fetches API keys from environment variables following the pattern `VLLORA_{PROVIDER_NAME}_API_KEY`. For example, for OpenAI, it will look for `VLLORA_OPENAI_API_KEY`.

---

## Quick start with async-openai-compatible types

If you already build OpenAI-compatible requests (e.g. via `async-openai-compat`), you can send **both non‑streaming and streaming** completions through `VlloraLLMClient`.

```rust
use async_openai::types::{
    ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use tokio_stream::StreamExt;

use vllora_llm::client::VlloraLLMClient;
use vllora_llm::error::LLMResult;
use vllora_llm::types::credentials::{ApiKeyCredentials, Credentials};

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
                    .content("Stream numbers 1 to 20 in separate lines.")
                    .build()?,
            ),
        ])
        .build()?;

    // 2) Construct a VlloraLLMClient (here: direct OpenAI key)
    let client = VlloraLLMClient::new().with_credentials(Credentials::ApiKey(
        ApiKeyCredentials {
            api_key: std::env::var("VLLORA_OPENAI_API_KEY")
                .expect("VLLORA_OPENAI_API_KEY must be set"),
        },
    ));

    // 3) Non-streaming: send the request and print the final reply
    let response = client
        .completions()
        .create(openai_req.clone())
        .await?;

    if let Some(content) = &response.message().content {
        if let Some(text) = content.as_string() {
            println!("Non-streaming reply:\\n{text}");
        }
    }

    // 4) Streaming: send the same request and print chunks as they arrive
    let mut stream = client
        .completions()
        .create_stream(openai_req)
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

## Supported parameters

The table below lists which `ChatCompletionRequest` (and provider-specific) parameters are honored by each provider when using `VlloraLLMClient`:

| **Parameter**                            | **OpenAI / Proxy** | **Anthropic** | **Gemini** | **Bedrock** | **Notes** |
|------------------------------------------|---------------------|---------------|------------|-------------|----------|
| `model`                                  | yes                 | yes           | yes        | yes         | Taken from `ChatCompletionRequest.model` or engine config. |
| `max_tokens`                             | yes                 | yes           | yes        | yes         | Mapped to provider-specific `max_tokens` / `max_output_tokens`. |
| `temperature`                            | yes                 | yes           | yes        | yes         | Sampling temperature. |
| `top_p`                                  | yes                 | yes           | yes        | yes         | Nucleus sampling. |
| `n`                                      | no                  | no            | yes        | no          | For Gemini, mapped to `candidate_count`; other providers always use `n = 1`. |
| `stop` / `stop_sequences`                | yes                 | yes           | yes        | yes         | Converted to each provider’s stop / stop-sequences field. |
| `presence_penalty`                       | yes                 | no            | yes        | no          | OpenAI / Gemini only. |
| `frequency_penalty`                      | yes                 | no            | yes        | no          | OpenAI / Gemini only. |
| `logit_bias`                             | yes                 | no            | no         | no          | OpenAI-only token bias map. |
| `user`                                   | yes                 | no            | no         | no          | OpenAI “end-user id” field. |
| `seed`                                   | yes                 | no            | yes        | no          | Deterministic sampling where supported. |
| `response_format` (JSON schema, etc.)    | yes                 | no            | yes        | no          | Gemini additionally normalizes JSON schema for its API. |
| `prompt_cache_key`                       | yes                 | no            | no         | no          | OpenAI-only prompt caching hint. |
| `provider_specific.top_k`                | no                  | yes           | no         | no          | Anthropic-only: maps to Claude `top_k`. |
| `provider_specific.thinking`             | no                  | yes           | no         | no          | Anthropic “thinking” options (e.g. budget tokens). |
| Bedrock `additional_parameters` map      | no                  | no            | no         | yes         | Free-form JSON, passed through to Bedrock model params. |

Additionally, for **Anthropic**, the **first `system` message** in the conversation is mapped into a `SystemPrompt` (either as a single text string or as multiple `TextContentBlock`s), and any `cache_control` options on those blocks are translated into Anthropic’s ephemeral cache-control settings.

All other fields on `ChatCompletionRequest` (such as `stream`, `tools`, `tool_choice`, `functions`, `function_call`) are handled at the gateway layer and/or per-provider tool integration, but are not mapped 1:1 into provider primitive parameters.


## Provider-specific examples

There are runnable examples under `llm/examples/` that mirror the patterns above:

- **`openai`**: Direct OpenAI chat completions using `VlloraLLMClient` (non-streaming + streaming).
- **`anthropic`**: Anthropic (Claude) chat completions via the unified client.
- **`gemini`**: Gemini chat completions via the unified client.
- **`bedrock`**: AWS Bedrock chat completions (Nova etc.) via the unified client.
- **`proxy_langdb`**: Using `InferenceModelProvider::Proxy("langdb")` to call a LangDB OpenAI-compatible endpoint.
- **`tracing`**: Same OpenAI-style flow as `openai`, but with `tracing_subscriber::fmt()` configured to emit spans and events to the console (stdout).
- **`tracing_otlp`**: Shows how to wire `vllora_telemetry::events::layer` to an OTLP HTTP exporter (e.g. New Relic / any OTLP collector) and emit spans from `VlloraLLMClient` calls to a remote telemetry backend.

Each example is a standalone Cargo binary; you can `cd` into a directory and run:

```bash
cargo run
```

after setting the provider-specific environment variables noted in the example’s `main.rs`.

## Notes

- **Real usage**: In the full LangDB / Vllora gateway, concrete `ModelInstance` implementations are created by the core executor based on your `models.yaml` and routing rules; the examples above use `DummyModelInstance` only to illustrate the public API of the `CompletionsClient`.
- **Error handling**: All client methods return `LLMResult<T>`, which wraps rich `LLMError` variants (network, mapping, provider errors, etc.).
- **More features**: The same types in `vllora_llm::types::gateway` are used for tools, MCP, routing, embeddings, and image generation; see the main repository docs at `https://vllora.dev/docs` for higher-level gateway features.

---

## Roadmap and issues

- **GitHub issues / roadmap**: See [open LLM crate issues](https://github.com/vllora/vllora/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22LLM%20Crate%22) for planned and outstanding work.
- **Planned enhancements**:
  - Integrate responses API
  - Support builtin MCP tool calls
  - Gemini prompt caching supported
  - Full thinking messages support


--- 

## License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a>.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>