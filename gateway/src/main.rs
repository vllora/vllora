use clap::Parser;
use config::Config;
use langdb_core::error::GatewayError;
use rest::ApiServer;
use run::models::load_models;
use thiserror::Error;

mod callback_handler;
mod cli;
mod config;
mod cost;
mod limit;
mod otel;
mod rest;
mod run;
mod tracing;
mod usage;

#[derive(Error, Debug)]
pub enum CliError {
    #[error(transparent)]
    GatewayError(#[from] GatewayError),
    #[error(transparent)]
    ConfigError(#[from] Box<dyn std::error::Error>),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    YamlError(#[from] serde_yaml::Error),
    #[error(transparent)]
    JsonError(#[from] serde_json::Error),
    #[error(transparent)]
    ServerError(#[from] rest::ServerError),
}

#[actix_web::main]
async fn main() -> Result<(), CliError> {
    tracing::init_tracing();
    std::env::set_var("RUST_BACKTRACE", "1");

    let cli = cli::Cli::parse();
    let config = Config::load(&cli.config);

    match cli.command.unwrap_or(cli::Commands::Serve) {
        cli::Commands::Update { force } => {
            println!("Updating models{}...", if force { " (forced)" } else { "" });
            let models = load_models(true).await?;
            println!("{} Models updated successfully!", models.len());
            Ok(())
        }
        cli::Commands::List => {
            println!("Available models:");
            let models = load_models(false).await?;
            // TODO: Implement better model listing logic
            run::table::pretty_print_models(models);
            Ok(())
        }
        cli::Commands::Serve => {
            let models = load_models(false).await?;
            let api_server = ApiServer::new(config);
            let api_result = api_server.start(models).await?.await;
            if let Err(e) = api_result {
                eprintln!("API Server Error: {:?}", e);
            }
            Ok(())
        }
    }
}
