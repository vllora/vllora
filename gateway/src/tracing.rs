use langdb_core::events::{self, BaggageSpanProcessor};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::TracerProvider;
use tokio::sync::mpsc::Sender;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Layer, Registry};

pub fn init_tracing() {
    let log_level = std::env::var("RUST_LOG").unwrap_or("info".to_string());
    let env_filter = EnvFilter::new(log_level).add_directive("actix_server=off".parse().unwrap());
    let color = std::env::var("ANSI_OUTPUT").map_or(true, |v| v == "true");

    // tracing syntax ->
    let builder = tracing_subscriber::fmt::layer()
        .pretty()
        .with_line_number(false)
        .with_file(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_target(false)
        .with_ansi(color)
        .with_filter(env_filter);

    let otlp_exporter = opentelemetry_otlp::new_exporter().tonic();
    let provider = TracerProvider::builder()
        .with_span_processor(BaggageSpanProcessor::new([
            "langdb.parent_trace_id",
            "langdb.run_id",
            "langdb.label",
        ]))
        .with_batch_exporter(
            otlp_exporter.build_span_exporter().unwrap(),
            opentelemetry_sdk::runtime::Tokio,
        )
        .with_config(events::config())
        .build();
    let tracer = provider.tracer("langdb-ai-gateway");
    opentelemetry::global::set_tracer_provider(provider);

    let otel_layer = events::layer("langdb::user_tracing", LevelFilter::INFO, tracer);
    Registry::default()
        .with(builder)
        .with(otel_layer)
        .try_init()
        .expect("initialized subscriber successfully");
}

pub fn init_tui_tracing(sender: Sender<String>) {
    // Set default log level if not set
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    let sender_clone = sender.clone();
    let make_writer = move || LogWriter {
        sender: sender_clone.clone(),
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::new(
                "langdb_core=off,actix_web::middleware::logger=info,error",
            )
            .add_directive("actix_web::middleware::logger=info".parse().unwrap())
            .add_directive("langdb_gateway=off".parse().unwrap())
            .add_directive("langdb_core=off".parse().unwrap())
            .add_directive("actix_server=off".parse().unwrap()),
        )
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(make_writer)
        .event_format(
            tracing_subscriber::fmt::format()
                .compact()
                .without_time()
                .with_target(false)
                .with_level(false),
        )
        .init();
}

struct LogWriter {
    sender: Sender<String>,
}

impl std::io::Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(log) = String::from_utf8(buf.to_vec()) {
            let sender = self.sender.clone();
            tokio::spawn(async move {
                sender.send(log).await.ok();
            });
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
