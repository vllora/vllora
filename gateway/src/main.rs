use std::sync::Arc;

use ::tracing::info;
use axum::routing::get;
use clap::Parser;
use config::{Config, ConfigError};
use http::ApiServer;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::services::model::ModelServiceImpl;
use vllora_core::{error::GatewayError, usage::InMemoryStorage};

mod callback_handler;
mod cli;
mod config;
mod cost;
mod guardrails;
mod handlers;
mod http;
mod middleware;
mod run;
mod seed;
mod session;
mod threads;
mod tracing;
mod usage;
use static_serve::embed_asset;
use static_serve::embed_assets;
use tokio::sync::Mutex;
use vllora_core::events::broadcast_channel_manager::BroadcastChannelManager;

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
        _     _                    
 __   _| |   | |    ___  _ __ __ _ 
 \ \ / / |   | |   / _ \| '__/ _` |
  \ V /| |___| |__| (_) | | | (_| |
   \_/ |_____|_____\___/|_|  \__,_|
"#;

embed_assets!("dist", compress = true);

// Embed models data JSON for fast startup
const MODELS_DATA_JSON: &str = include_str!("../models_data.json");

#[actix_web::main]
async fn main() -> Result<(), CliError> {
    dotenv::dotenv().ok();
    println!("{LOGO}");
    std::env::set_var("RUST_BACKTRACE", "1");

    let cli = cli::Cli::parse();

    let db_pool = get_db_pool()?;

    vllora_core::metadata::utils::init_db(&db_pool);
    let session = session::fetch_session_id(db_pool.clone()).await;

    // Ping session once in background (non-blocking)
    session::check_version(session.id.clone());

    let project_trace_senders = Arc::new(BroadcastChannelManager::new(Default::default()));

    let project_trace_senders_cleanup = Arc::clone(&project_trace_senders);
    vllora_core::events::broadcast_channel_manager::start_cleanup_task(
        (*project_trace_senders_cleanup).clone(),
    );

    tracing::init_tracing(project_trace_senders.inner().clone());
    // Seed the database with a default project if none exist
    seed::seed_database(&db_pool)?;

    match cli
        .command
        .unwrap_or(cli::Commands::Serve(cli::ServeArgs::default()))
    {
        cli::Commands::Sync { models, providers } => {
            // If no specific flags are provided, sync both
            let sync_models = models || !providers;
            let sync_providers = providers || !models;

            if sync_models {
                info!("Syncing models from API to database...");
                let models = run::models::fetch_and_store_models(db_pool.clone()).await?;
                info!("Successfully synced {} models to database", models.len());
            }

            if sync_providers {
                info!("Syncing providers from API to database...");
                run::providers::sync_providers(db_pool.clone()).await?;
                info!("Successfully synced providers to database");
            }

            Ok(())
        }
        cli::Commands::List => {
            // Query models from database
            use vllora_core::metadata::services::model::ModelService;
            let model_service = ModelServiceImpl::new(db_pool.clone());
            let db_models = model_service.list(None)?;

            info!("Found {} models in database\n", db_models.len());

            // Convert DbModel to ModelMetadata and display as table
            let models: Vec<vllora_core::models::ModelMetadata> =
                db_models.into_iter().map(|m| m.into()).collect();

            run::table::pretty_print_models(models);
            Ok(())
        }
        cli::Commands::GenerateModelsJson { output } => {
            info!("Generating models JSON file: {}", output);
            let output_path = std::path::Path::new(&output);
            let models = run::models::fetch_and_save_models_json(output_path).await?;
            info!(
                "Successfully generated {} models to {}",
                models.len(),
                output
            );
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
            let open_ui_on_startup = config.ui.open_on_startup;

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
                        serde_json::json!({ "VITE_BACKEND_PORT": backend_port, "VERSION": env!("CARGO_PKG_VERSION") }),
                    )
                };

                let index = embed_asset!("dist/index.html");
                let router = static_router()
                    .route("/api/env", get(vite_backend_port_handler))
                    .fallback(index);

                let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", ui_port)).await;
                match listener {
                    Ok(listener) => {
                        if open_ui_on_startup {
                            // Open UI in browser after server starts
                            let ui_url = format!("http://localhost:{}", ui_port);
                            // Try to open in browser, but don't fail if it doesn't work
                            if let Err(e) = open::that(&ui_url) {
                                println!("⚠ Could not open browser automatically: {}", e);
                                println!("   Please open {} manually", ui_url);
                            } else {
                                println!("🚀 Opening UI in your default browser...");
                            }
                        }

                        axum::serve(listener, router.into_make_service())
                            .await
                            .unwrap();
                    }
                    Err(e) => {
                        eprintln!("Failed to bind frontend server to port {}: {e}", ui_port);
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

fn get_db_pool() -> Result<vllora_core::metadata::pool::DbPool, CliError> {
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    let vllora_dir = format!("{home_dir}/.vllora");
    std::fs::create_dir_all(&vllora_dir).unwrap_or_default();
    let vllora_db_file = format!("{vllora_dir}/vllora.db");
    let db_pool = vllora_core::metadata::pool::establish_connection(vllora_db_file, 10);

    vllora_core::metadata::utils::init_db(&db_pool);

    Ok(db_pool)
}
