use vllora_llm::async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
};
use tokio_stream::StreamExt;

use opentelemetry::global;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};
use vllora_telemetry::events;

use vllora_llm::client::VlloraLLMClient;
use vllora_llm::error::LLMResult;
use vllora_llm::types::credentials::{ApiKeyCredentials, Credentials};

use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_otlp::WithHttpConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;

fn init_tracing_with_otlp() {
    // Build OTLP exporter targeting a generic OTLP endpoint.
    // Configure endpoint via OTLP_HTTP_ENDPOINT (e.g. https://otlp.nr-data.net/v1/traces)
    // or it will fall back to the OpenTelemetry defaults.
    let mut provider_builder = SdkTracerProvider::builder();

    if let Ok(endpoint) = std::env::var("OTLP_HTTP_ENDPOINT") {
        tracing::info!("OTLP_HTTP_ENDPOINT set, exporting traces to {endpoint}");

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .with_headers(std::collections::HashMap::from([(
                "api-key".into(),
                std::env::var("OTLP_API_KEY").expect("OTLP_API_KEY must be set"),
            )]))
            .with_protocol(opentelemetry_otlp::Protocol::HttpJson)
            .build()
            .expect("failed to build OTLP HTTP exporter");

        provider_builder = provider_builder.with_batch_exporter(exporter);
    }

    // Build single provider and single layer (INFO only)
    let provider = provider_builder.build();
    let tracer = provider.tracer("vllora-llm-example");
    global::set_tracer_provider(provider);

    let otel_layer = events::layer("*", tracing::level_filters::LevelFilter::INFO, tracer);

    Registry::default().with(otel_layer).init();
}

#[tokio::main]
async fn main() -> LLMResult<()> {
    // 1) Initialize tracing + OTLP exporter
    init_tracing_with_otlp();

    info!("starting tracing_otlp_example");

    // 2) Build an OpenAI-style request using async-openai-compatible types
    let openai_req = CreateChatCompletionRequestArgs::default()
        .model("gpt-4.1-mini")
        .messages([
            ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content("You are a helpful assistant that is traced via OTLP.")
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
    let client = VlloraLLMClient::new().with_credentials(Credentials::ApiKey(ApiKeyCredentials {
        api_key: std::env::var("VLLORA_OPENAI_API_KEY").expect("VLLORA_OPENAI_API_KEY must be set"),
    }));

    info!("sending non-streaming completion request");

    // 4) Non-streaming: send the request and print the final reply
    let response = client.completions().create(openai_req.clone()).await?;

    if let Some(content) = &response.message().content {
        if let Some(text) = content.as_string() {
            info!("received non-streaming reply");
            println!("Non-streaming reply:\n{text}");
        }
    }

    info!("sending streaming completion request");

    // 5) Streaming: send the same request and print chunks as they arrive
    let mut stream = client.completions().create_stream(openai_req).await?;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        for choice in chunk.choices {
            if let Some(delta) = choice.delta.content {
                print!("{delta}");
            }
        }
    }

    info!("finished tracing_otlp_example");

    Ok(())
}
