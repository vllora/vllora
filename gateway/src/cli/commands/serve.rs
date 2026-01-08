use crate::agents;
use crate::cli::{Commands, ServeArgs};
use crate::config::Config;
use crate::distri;
use crate::http::ApiServer;
use crate::ports::{resolve_ports, Service, ServicePort};
use crate::seed;
use crate::CliError;
use axum::routing::get;
use static_serve::embed_asset;
use static_serve::embed_assets;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use vllora_core::events::broadcast_channel_manager::BroadcastChannelManager;
use vllora_core::metadata::models::session::DbSession;
use vllora_core::metadata::pool::DbPool;
use vllora_core::telemetry::RunSpanBuffer;
use vllora_core::usage::InMemoryStorage;

embed_assets!("dist", compress = true);

pub async fn handle_serve(
    db_pool: DbPool,
    serve_args: ServeArgs,
    config_path: String,
    project_trace_senders: Arc<BroadcastChannelManager>,
    run_span_buffer: Arc<RunSpanBuffer>,
    session: DbSession,
) -> Result<(), CliError> {
    // Check if models table is empty and sync if needed
    seed::seed_models(&db_pool).await?;

    // Check if providers table is empty and sync if needed
    seed::seed_providers(&db_pool).await?;

    let config = Config::load(&config_path)?;
    let mut config = config.apply_cli_overrides(&Commands::Serve(serve_args.clone()));

    let services = resolve_ports(&config).await?;

    let services_with_new_ports = services
        .iter()
        .filter(|service| service.suggested_port.is_some())
        .collect::<Vec<&ServicePort>>();

    if !services_with_new_ports.is_empty() {
        println!("\n✅ Configured ports are in use. New ports have been assigned for the following services:");
        for service in &services_with_new_ports {
            println!(
                "   {}: {} -> {}",
                service.service,
                service.initial_port,
                service.suggested_port.unwrap()
            );
        }

        print!("\n⚠️  Would you like to accept these port changes? (Y/n): ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                let trimmed = input.trim().to_lowercase();
                // Default to "yes" if empty (just pressing Enter)
                if !trimmed.is_empty() && trimmed != "y" && trimmed != "yes" {
                    eprintln!("❌ Port changes rejected. Exiting.");
                    return Err(CliError::IoError(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "User rejected port changes",
                    )));
                }

                // Apply the port changes to config
                for service in &services_with_new_ports {
                    match service.service {
                        Service::Backend => {
                            config.http.port = service
                                .suggested_port
                                .expect("Suggested port should be present");
                        }
                        Service::UI => {
                            config.ui.port = service
                                .suggested_port
                                .expect("Suggested port should be present");
                        }
                        Service::Otel => {
                            config.otel.port = service
                                .suggested_port
                                .expect("Suggested port should be present");
                        }
                        Service::Distri => {
                            config.distri.port = service
                                .suggested_port
                                .expect("Suggested port should be present");
                        }
                    }
                }

                println!("✅ Port changes accepted.\n");
            }
            Err(_) => {
                eprintln!("❌ Failed to read user input. Exiting.");
                return Err(CliError::IoError(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Failed to read user input",
                )));
            }
        }
    }

    // Start Distri server if not already running
    let distri_api_url =
        std::env::var("DISTRI_URL").unwrap_or_else(|_| "http://localhost:8081".to_string());

    if !distri::is_distri_running(&distri_api_url).await {
        println!("📥 Downloading and starting Distri server...");
        match distri::start_distri_server(config.distri.port).await {
            Ok(mut child) => {
                println!("✅ Distri server started successfully");

                // Spawn a task to monitor the Distri process
                tokio::spawn(async move {
                    let status = child.wait().await;
                    if let Ok(status) = status {
                        if status.code().unwrap_or(0) > 2 {
                            eprintln!("⚠️  Distri server process exited with status: {:?}", status);
                        }
                    }
                });
            }
            Err(e) => {
                eprintln!("⚠️  Warning: Failed to start Distri server: {}", e);
                eprintln!("   Agents may not be available. You can start Distri manually.");
            }
        }
    } else {
        println!("✅ Distri server is already running");
    }

    // Extract ports from config after potential changes
    let backend_port = config.http.port;
    let ui_port = config.ui.port;
    let otel_port = config.otel.port;
    let open_ui_on_startup = config.ui.open_on_startup;
    let distri_port = config.distri.port;

    // Register agents with Distri server in background (non-blocking)
    println!("📋 Registering agents with Distri server in background...");
    tokio::spawn(async move {
        // Wait a bit for Distri to be fully ready
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        match agents::register_agents(Some(format!("http://localhost:{}", distri_port))).await {
            Ok(_) => {
                // Success message is logged inside register_agents
            }
            Err(e) => {
                eprintln!("⚠️  Warning: Failed to register agents: {}", e);
                eprintln!("   Agents may not be available.");
            }
        }
    });

    let api_server = ApiServer::new(config, db_pool.clone());
    let server_handle = tokio::spawn(async move {
        let storage = Arc::new(Mutex::new(InMemoryStorage::new()));
        match api_server
            .start(
                Some(storage),
                project_trace_senders.clone(),
                run_span_buffer.clone(),
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
            axum::Json(serde_json::json!({
                "VITE_BACKEND_PORT": backend_port,
                "VITE_OTEL_PORT": otel_port,
                "VERSION": env!("CARGO_PKG_VERSION"),
                "VITE_DISTRI_PORT": distri_port
            }))
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

                if let Err(e) = axum::serve(listener, router.into_make_service()).await {
                    eprintln!("Failed to bind frontend server to port {}: {e}", ui_port);
                }
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
