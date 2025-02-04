use clap::Parser;
use config::{Config, ConfigError};
use http::ApiServer;
use langdb_core::{error::GatewayError, model::image_generation::openai::ApiError};
use run::models::{load_models, ModelsLoadError};
use thiserror::Error;

mod callback_handler;
mod cli;
mod config;
mod cost;
mod http;
mod limit;
mod otel;
mod run;
mod tracing;
mod tui;
mod usage;
use ::tracing::info;
use tui::Tui;

#[derive(Error, Debug)]
pub enum CliError {
    #[error(transparent)]
    GatewayError(#[from] GatewayError),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    YamlError(#[from] serde_yaml::Error),
    #[error(transparent)]
    JsonError(#[from] serde_json::Error),
    #[error(transparent)]
    ServerError(#[from] http::ServerError),
    #[error(transparent)]
    ConfigError(#[from] ConfigError),
    #[error(transparent)]
    ModelsError(#[from] ModelsLoadError),
}

#[actix_web::main]
async fn main() -> Result<(), CliError> {
    dotenv::dotenv().ok();
    std::env::set_var("RUST_BACKTRACE", "1");

    let cli = cli::Cli::parse();
    let config = Config::load(&cli.config)?;

    match cli
        .command
        .unwrap_or(cli::Commands::Serve(cli::ServeArgs::default()))
    {
        cli::Commands::Update { force } => {
            tracing::init_tracing();
            println!("Updating models{}...", if force { " (forced)" } else { "" });
            let models = load_models(true).await?;
            println!("{} Models updated successfully!", models.len());
            Ok(())
        }
        cli::Commands::List => {
            tracing::init_tracing();
            println!("Available models:");
            let models = load_models(false).await?;
            run::table::pretty_print_models(models);
            Ok(())
        }
        cli::Commands::Serve(serve_args) => {
            let (log_sender, log_receiver) = tokio::sync::mpsc::channel(100);
            tracing::init_tui_tracing(log_sender);

            let models = load_models(false).await?;
            let config = config.apply_cli_overrides(&cli::Commands::Serve(serve_args));

            let api_server = ApiServer::new(config);
            info!("Starting server...");

            let mut server_handle = tokio::spawn(async move {
                match api_server.start(models).await {
                    Ok(server) => server.await,
                    Err(e) => Err(e),
                }
            });

            let mut tui_handle = tokio::spawn(async move {
                let mut tui = Tui::new(log_receiver)?;
                tui.run()?;
                Ok::<(), CliError>(())
            });

            tokio::select! {
                tui_result = &mut tui_handle => {
                    if let Ok(result) = tui_result {
                        result?;
                    }
                    server_handle.abort();
                }
                server_result = &mut server_handle => {
                    if let Ok(result) = server_result {
                        result?;
                    }
                    tui_handle.abort();
                }
            }

            Ok(())
        }
    }
}
