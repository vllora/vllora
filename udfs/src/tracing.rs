use tracing_appender::rolling;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

pub fn get_default_filter(filter: &str) -> String {
    format!("error,{filter}")
}

pub fn init_tracing(override_env: Option<&str>) -> crate::Result<()> {
    let enable_file_logging = std::env::var("UDF_FILE_LOGGING").is_ok();
    let log_writer = move || -> Box<dyn std::io::Write + Send> {
        if enable_file_logging {
            Box::new(rolling::minutely("./logs", "logs"))
        } else {
            Box::new(std::io::stderr())
        }
    };

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
            .or_else(|_| EnvFilter::try_new(get_default_filter("udfs=error"))),
    }
    .unwrap();

    let subscriber = tracing_subscriber::Registry::default().with(
        tracing_subscriber::fmt::Layer::default()
            .event_format(f)
            .with_writer(log_writer)
            .with_filter(stdout_filter),
    );

    subscriber.init();
    Ok(())
}
