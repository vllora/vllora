use std::sync::{Arc, RwLock};

use clap::Parser;
use config::{Config, ConfigError};
use http::ApiServer;
use langdb_core::{error::GatewayError, usage::InMemoryStorage};
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
use tokio::sync::Mutex;
use tui::{Counters, Tui};

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

pub const LOGO: &str = r#"

  ██       █████  ███    ██  ██████  ██████  ██████  
  ██      ██   ██ ████   ██ ██       ██   ██ ██   ██ 
  ██      ███████ ██ ██  ██ ██   ███ ██   ██ ██████  
  ██      ██   ██ ██  ██ ██ ██    ██ ██   ██ ██   ██ 
  ███████ ██   ██ ██   ████  ██████  ██████  ██████
"#;

#[actix_web::main]
async fn main() -> Result<(), CliError> {
    dotenv::dotenv().ok();
    println!("{LOGO}");
    std::env::set_var("RUST_BACKTRACE", "1");

    let cli = cli::Cli::parse();

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
            if serve_args.interactive {
                let (log_sender, log_receiver) = tokio::sync::mpsc::channel(100);
                tracing::init_tui_tracing(log_sender);

                let config = Config::load(&cli.config)?;
                let models = load_models(false).await?;
                let config = config.apply_cli_overrides(&cli::Commands::Serve(serve_args));

                let api_server = ApiServer::new(config);
                info!("Starting server...");

                let storage = Arc::new(Mutex::new(InMemoryStorage::new()));
                let storage_clone = storage.clone();
                let counters = Arc::new(RwLock::new(Counters::default()));
                let counters_clone = counters.clone();
                let mut counters_handle =
                    tokio::spawn(async move { Tui::spawn_counter_loop(storage, counters).await });

                let mut server_handle = tokio::spawn(async move {
                    match api_server.start(models, Some(storage_clone)).await {
                        Ok(server) => server.await,
                        Err(e) => Err(e),
                    }
                });

                let mut tui_handle: tokio::task::JoinHandle<Result<(), CliError>> =
                    tokio::spawn(async move {
                        let mut tui = Tui::new(log_receiver)?;
                        tui.run(counters_clone)?;
                        Ok::<(), CliError>(())
                    });

                tokio::select! {
                    counter_result = &mut counters_handle => {
                        if let Err(e) = counter_result {
                            eprintln!("{e}");
                        }
                        counters_handle.abort();
                    }
                    tui_result = &mut tui_handle => {
                        if let Ok(Err(e)) = tui_result {
                            eprintln!("{e}");
                        }
                        server_handle.abort();
                    }

                    server_result = &mut server_handle => {
                        if let Ok(Err(e)) = server_result {
                            eprintln!("{e}");
                        }
                        tui_handle.abort();
                    }
                }
            } else {
                tracing::init_tracing();

                let config = Config::load(&cli.config)?;
                let config = config.apply_cli_overrides(&cli::Commands::Serve(serve_args));
                let api_server = ApiServer::new(config);
                let models = load_models(false).await?;
                let server_handle = tokio::spawn(async move {
                    let storage = Arc::new(Mutex::new(InMemoryStorage::new()));
                    match api_server.start(models, Some(storage)).await {
                        Ok(server) => server.await,
                        Err(e) => Err(e),
                    }
                });

                match server_handle.await {
                    Ok(result) => {
                        if let Err(e) = result {
                            eprintln!("{e}");
                        }
                    }
                    Err(e) => eprintln!("{e}"),
                }
            }
            Ok(())
        }
    }
}
