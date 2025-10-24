use std::sync::Arc;

use ::tracing::info;
use axum::routing::get;
use clap::Parser;
use config::{Config, ConfigError};
use http::ApiServer;
use langdb_core::metadata::error::DatabaseError;
use langdb_core::metadata::services::model::ModelServiceImpl;
use langdb_core::{error::GatewayError, usage::InMemoryStorage};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod callback_handler;
mod cli;
mod config;
mod cost;
mod guardrails;
mod handlers;
mod http;
mod limit;
mod middleware;
mod run;
mod seed;
mod session;
mod tracing;
mod usage;
use langdb_core::events::broadcast_channel_manager::BroadcastChannelManager;
use static_serve::embed_asset;
use static_serve::embed_assets;
use tokio::sync::Mutex;

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
▗▄▄▄▖▗▖   ▗▖    ▗▄▖ ▗▄▄▖  ▗▄▖ 
▐▌   ▐▌   ▐▌   ▐▌ ▐▌▐▌ ▐▌▐▌ ▐▌
▐▛▀▀▘▐▌   ▐▌   ▐▌ ▐▌▐▛▀▚▖▐▛▀▜▌
▐▙▄▄▖▐▙▄▄▖▐▙▄▄▖▝▚▄▞▘▐▌ ▐▌▐▌ ▐▌
"#;

embed_assets!("dist", compress = true);

#[actix_web::main]
async fn main() -> Result<(), CliError> {
    dotenv::dotenv().ok();
    println!("{LOGO}");
    std::env::set_var("RUST_BACKTRACE", "1");

    let cli = cli::Cli::parse();

    let db_pool = get_db_pool()?;

    langdb_core::metadata::utils::init_db(&db_pool);
    let session = session::fetch_session_id(db_pool.clone()).await;

    // Ping session once in background (non-blocking)
    session::ping_session(session.id.clone());

    let project_trace_senders = Arc::new(BroadcastChannelManager::new(Default::default()));

    let project_trace_senders_cleanup = Arc::clone(&project_trace_senders);
    langdb_core::events::broadcast_channel_manager::start_cleanup_task(
        (*project_trace_senders_cleanup).clone(),
    );

    tracing::init_tracing(project_trace_senders.inner().clone());
    // Seed the database with a default project if none exist
    seed::seed_database(&db_pool)?;

    match cli
        .command
        .unwrap_or(cli::Commands::Serve(cli::ServeArgs::default()))
    {
        cli::Commands::Login => session::login().await,
        cli::Commands::Sync => {
            tracing::init_tracing(project_trace_senders.inner().clone());
            info!("Syncing models from API to database...");
            let models = run::models::fetch_and_store_models(db_pool.clone()).await?;
            info!("Successfully synced {} models to database", models.len());
            Ok(())
        }
        cli::Commands::SyncProviders => {
            tracing::init_tracing(project_trace_senders.inner().clone());
            info!("Syncing providers from API to database...");
            run::providers::sync_providers(db_pool.clone()).await?;
            info!("Successfully synced providers to database");
            Ok(())
        }
        cli::Commands::List => {
            tracing::init_tracing(project_trace_senders.inner().clone());
            // Query models from database
            use langdb_core::metadata::services::model::ModelService;
            let model_service = ModelServiceImpl::new(db_pool.clone());
            let db_models = model_service.list(None)?;

            info!("Found {} models in database\n", db_models.len());

            // Convert DbModel to ModelMetadata and display as table
            let models: Vec<langdb_core::models::ModelMetadata> =
                db_models.into_iter().map(|m| m.into()).collect();

            run::table::pretty_print_models(models);
            Ok(())
        }
        cli::Commands::Serve(serve_args) => {
            // Check if models table is empty and sync if needed
            seed::seed_models(&db_pool).await?;

            // Check if providers table is empty and sync if needed
            seed::seed_providers(&db_pool).await?;

            let config = Config::load(&cli.config)?;
            let config = config.apply_cli_overrides(&cli::Commands::Serve(serve_args));

            let backend_port = config.http.port;
            let ui_port = config.ui.port;

            let api_server = ApiServer::new(config, db_pool.clone());
            let server_handle = tokio::spawn(async move {
                let storage = Arc::new(Mutex::new(InMemoryStorage::new()));
                match api_server
                    .start(
                        Some(storage),
                        project_trace_senders.clone(),
                        session.clone(),
                    )
                    .await
                {
                    Ok(server) => server.await,
                    Err(e) => Err(e),
                }
            });

            let frontend_handle = tokio::spawn(async move {
                // Handler for serving VITE_BACKEND_PORT environment variable as plain text or JSON
                let vite_backend_port_handler = move || async move {
                    axum::Json(
                        serde_json::json!({ "VITE_BACKEND_PORT": backend_port, "version": env!("CARGO_PKG_VERSION") }),
                    )
                };

                let index = embed_asset!("dist/index.html");
                let router = static_router()
                    .route("/api/env", get(vite_backend_port_handler))
                    .fallback(index);

                let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", ui_port)).await;
                match listener {
                    Ok(listener) => {
                        axum::serve(listener, router.into_make_service())
                            .await
                            .unwrap();
                    }
                    Err(e) => {
                        eprintln!("Failed to bind frontend server to port 8084: {e}");
                    }
                }
            });

            tokio::select! {
                r = server_handle => {
                    if let Err(e) = r {
                        eprintln!("Counter loop error: {e}");
                    }
                }
                r = frontend_handle => {
                    if let Err(e) = r {
                        eprintln!("Server error: {e}");
                    }
                }
            }
            Ok(())
        }
    }
}

fn get_db_pool() -> Result<langdb_core::metadata::pool::DbPool, CliError> {
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    let ellora_dir = format!("{home_dir}/.ellora");
    std::fs::create_dir_all(&ellora_dir).unwrap_or_default();
    let ellora_db_file = format!("{ellora_dir}/ellora.sqlite");
    let db_pool = langdb_core::metadata::pool::establish_connection(ellora_db_file, 10);

    langdb_core::metadata::utils::init_db(&db_pool);

    Ok(db_pool)
}
