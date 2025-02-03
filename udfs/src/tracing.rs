use tracing_appender::rolling;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

pub fn get_default_filter(filter: &str) -> String {
    format!("error,{filter}")
}

pub fn init_tracing(override_env: Option<&str>) -> crate::Result<()> {
    let debug_file = rolling::minutely("./logs", "logs");

    let f = tracing_subscriber::fmt::format::Format::default()
        .with_file(true)
        .with_line_number(true)
        .with_level(true)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_ansi(false)
        .compact();

    let stdout_filter = match override_env {
        Some(env) => EnvFilter::try_new(env),
        None => EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(get_default_filter("udfs=debug"))),
    }
    .unwrap();

    let subscriber = tracing_subscriber::Registry::default().with(
        tracing_subscriber::fmt::Layer::default()
            .event_format(f)
            .with_writer(debug_file)
            .with_filter(stdout_filter),
    );

    subscriber.init();
    Ok(())
}
