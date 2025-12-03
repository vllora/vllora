use async_openai_compat::types::{
    ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use tokio_stream::StreamExt;

use tracing::{info, Level};
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

use vllora_llm::client::VlloraLLMClient;
use vllora_llm::error::LLMResult;
use vllora_llm::types::credentials::{ApiKeyCredentials, Credentials};

fn init_tracing() {
    // Initialize tracing with a console (stdout) formatter.
    //
    // Control verbosity with RUST_LOG, for example:
    //   RUST_LOG=info,tracing_console_example=debug
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .with_span_events(FmtSpan::FULL) // log span enter/exit
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .init();
}

#[tokio::main]
async fn main() -> LLMResult<()> {
    // 1) Set up console logging for spans and events
    init_tracing();

    info!("starting tracing_console_example");

    // 2) Build an OpenAI-style request using async-openai-compatible types
    let openai_req = CreateChatCompletionRequestArgs::default()
        .model("gpt-4.1-mini")
        .messages([
            ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content("You are a helpful assistant that logs to tracing.")
                    .build()?,
            ),
            ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessageArgs::default()
                    .content("Say hello in three short sentences.")
                    .build()?,
            ),
        ])
        .build()?;

    // 3) Construct a VlloraLLMClient (direct OpenAI key)
    let client = VlloraLLMClient::new().with_credentials(Credentials::ApiKey(
        ApiKeyCredentials {
            api_key: std::env::var("VLLORA_OPENAI_API_KEY")
                .expect("VLLORA_OPENAI_API_KEY must be set"),
        },
    ));

    info!("sending non-streaming completion request");

    // 4) Non-streaming: send the request and print the final reply
    let response = client
        .completions()
        .create(openai_req.clone())
        .await?;

    if let Some(content) = &response.message().content {
        if let Some(text) = content.as_string() {
            info!("received non-streaming reply");
            println!("Non-streaming reply:\n{text}");
        }
    }

    info!("sending streaming completion request");

    // 5) Streaming: send the same request and print chunks as they arrive
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

    info!("finished tracing_console_example");

    Ok(())
}
