use std::sync::Arc;

use clap::Parser;
use config::ConfigError;
use serde::Deserialize;
use serde::Serialize;
use std::time::Duration;
use thiserror::Error;
use vllora_core::error::GatewayError;
use vllora_core::events::broadcast_channel_manager::BroadcastChannelManager;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::telemetry::RunSpanBuffer;

mod agents;
mod callback_handler;
mod cli;
mod config;
mod cost;
mod guardrails;
mod handlers;
mod http;
mod metrics_writer;
mod middleware;
mod ports;
mod run;
mod seed;
mod session;
mod threads;
mod tracing;
mod usage;

#[derive(Error, Debug)]
pub enum CliError {
    #[error(transparent)]
    GatewayError(#[from] Box<GatewayError>),
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
    DatabaseError(#[from] DatabaseError),
    #[error(transparent)]
    ModelsLoadError(#[from] run::models::ModelsLoadError),
    #[error(transparent)]
    ProvidersLoadError(#[from] run::providers::ProvidersLoadError),
    #[error(transparent)]
    AgentError(#[from] agents::AgentError),
    #[error("Error: {0}")]
    CustomError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    api_key: String,
}

pub const LOGO: &str = r#"
        _     _                    
 __   _| |   | |    ___  _ __ __ _ 
 \ \ / / |   | |   / _ \| '__/ _` |
  \ V /| |___| |__| (_) | | | (_| |
   \_/ |_____|_____\___/|_|  \__,_|
"#;

// Embed models data JSON for fast startup
const MODELS_DATA_JSON: &str = include_str!("../models_data.json");

#[actix_web::main]
async fn main() -> Result<(), CliError> {
    dotenv::dotenv().ok();
    std::env::set_var("RUST_BACKTRACE", "1");

    let cli = cli::Cli::parse();

    let db_pool = get_db_pool()?;

    if let Some(cli::Commands::Traces(traces_cmd)) = cli.command {
        cli::commands::traces::handle_traces(db_pool, traces_cmd).await?;
        return Ok(());
    }

    println!("{LOGO}");
    let project_trace_senders = Arc::new(BroadcastChannelManager::new(Default::default()));

    let project_trace_senders_cleanup = Arc::clone(&project_trace_senders);
    vllora_core::events::broadcast_channel_manager::start_cleanup_task(
        (*project_trace_senders_cleanup).clone(),
    );

    let run_span_buffer = Arc::new(RunSpanBuffer::new(Duration::from_secs(20)));

    tracing::init_tracing(
        project_trace_senders.inner().clone(),
        run_span_buffer.clone(),
        Some(db_pool.clone()),
    );

    vllora_core::metadata::utils::init_db(&db_pool);
    let session = session::fetch_session_id(db_pool.clone()).await;

    // Ping session once in background (non-blocking)
    session::check_version(session.id.clone());

    // Seed the database with a default project if none exist
    seed::seed_database(&db_pool)?;

    match cli.command {
        Some(cli::Commands::Sync { models, providers }) => {
            cli::commands::sync::handle_sync(db_pool, models, providers).await
        }
        Some(cli::Commands::List) => cli::commands::list::handle_list(db_pool).await,
        Some(cli::Commands::Traces(_traces_cmd)) => {
            unreachable!()
        }
        Some(cli::Commands::GenerateModelsJson { output }) => {
            cli::commands::generate_models_json::handle_generate_models_json(output).await
        }
        Some(cli::Commands::Serve(subcommand_args)) => {
            cli::commands::serve::handle_serve(
                db_pool,
                subcommand_args,
                cli.config,
                project_trace_senders,
                run_span_buffer,
                session,
            )
            .await
        }
        None => {
            // No command specified, use flattened serve args
            cli::commands::serve::handle_serve(
                db_pool,
                cli.serve_args,
                cli.config,
                project_trace_senders,
                run_span_buffer,
                session,
            )
            .await
        }
    }
}

fn get_db_pool() -> Result<vllora_core::metadata::pool::DbPool, CliError> {
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    let vllora_dir = format!("{home_dir}/.vllora");
    std::fs::create_dir_all(&vllora_dir).unwrap_or_default();
    let vllora_db_file = format!("{vllora_dir}/vllora.db");
    let db_pool = vllora_core::metadata::pool::establish_connection(vllora_db_file, 10);

    vllora_core::metadata::utils::init_db(&db_pool);

    Ok(db_pool)
}
