use crate::callback_handler::init_callback_handler;
use crate::config::Config;
use crate::cost::GatewayCostCalculator;
use crate::eval_state_tracker::EvalJobStateTracker;
use crate::finetune_state_tracker::FinetuneJobStateTracker;
use crate::guardrails::GuardrailsService;
use crate::handlers::{
    agents, debug, eval_jobs, finetune, knowledge_sources, models, projects, session, threads,
    workflow_logs, workflow_records, workflow_topics, workflows,
};
use crate::knowledge_embeddings::start_embedding_backfill_job;
use crate::metrics_writer::SqliteMetricsWriterAdapter;
use crate::middleware::lucy_project::LucyProjectMiddleware;
use crate::middleware::project::ProjectMiddleware;
use crate::middleware::thread_service::ThreadsServiceMiddleware;
use crate::middleware::trace_logger::TraceLogger;
use crate::middleware::tracing_context::TracingContext;
use actix_cors::Cors;
use actix_web::web::JsonConfig;
use actix_web::Scope as ActixScope;
use actix_web::{
    body::MessageBody,
    dev::{ServiceFactory, ServiceRequest, ServiceResponse},
    web::{self, Data},
    App, HttpServer,
};
use futures::{future::try_join, Future, TryFutureExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::signal;
use tokio::sync::Mutex;
use vllora_core::credentials::KeyStorage;
use vllora_core::credentials::ProviderKeyResolver;
use vllora_core::events::broadcast_channel_manager::BroadcastChannelManager;
use vllora_core::events::callback_handler::GatewayCallbackHandlerFn;
use vllora_core::events::ui_broadcaster::EventsSendersContainer;
use vllora_core::events::ui_broadcaster::EventsUIBroadcaster;
use vllora_core::executor::chat_completion::breakpoint::BreakpointManager;
use vllora_core::handler::chat::create_chat_completion;
use vllora_core::handler::embedding::embeddings_handler;
use vllora_core::handler::group;
use vllora_core::handler::image::create_image;
use vllora_core::handler::labels;
use vllora_core::handler::mcp_configs;
use vllora_core::handler::middleware::actix_otel::CloudApiInvokeMiddleware;
use vllora_core::handler::middleware::actix_otel::RunSpanMiddleware;
use vllora_core::handler::middleware::rate_limit::RateLimitMiddleware;
use vllora_core::handler::middleware::run_id::RunId;
use vllora_core::handler::middleware::thread_id::ThreadId;
use vllora_core::handler::responses;
use vllora_core::handler::runs;
use vllora_core::handler::spans;
use vllora_core::handler::traces;
use vllora_core::handler::CallbackHandlerFn;
use vllora_core::mcp::server::LocalSessionManager;
use vllora_core::metadata::models::session::DbSession;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::project_trace::ProjectTraceTenantResolver;
use vllora_core::metadata::services::group::GroupServiceImpl;
use vllora_core::metadata::services::model::ModelServiceImpl;
use vllora_core::metadata::services::project::ProjectServiceImpl;
use vllora_core::metadata::services::provider::ProvidersServiceImpl;
use vllora_core::metadata::services::run::RunServiceImpl;
use vllora_core::metadata::services::trace::TraceServiceImpl as MetadataTraceServiceImpl;
use vllora_core::metadata::DatabaseService;
use vllora_core::telemetry::database::SqliteTraceWriterTransport;
use vllora_core::telemetry::metrics_database::SqliteMetricsWriterTransport;
use vllora_core::telemetry::RunSpanBuffer;
use vllora_core::types::guardrails::service::GuardrailsEvaluator;
use vllora_core::types::guardrails::Guard;
use vllora_core::types::metadata::services::model::ModelService;
use vllora_core::usage::InMemoryStorage;
use vllora_llm::types::gateway::CostCalculator;
use vllora_telemetry::MetricsServiceImpl;
use vllora_telemetry::MetricsServiceServer;
use vllora_telemetry::SpanWriterTransport;
use vllora_telemetry::TraceServiceImpl as TelemetryTraceServiceImpl;
use vllora_telemetry::TraceServiceServer;

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "serde")]
pub enum CorsOptions {
    Permissive,
    Custom(Vec<String>, usize),
}

#[derive(Error, Debug)]
pub enum ServerError {
    #[error(transparent)]
    Actix(#[from] std::io::Error),
    #[error(transparent)]
    Tonic(#[from] tonic::transport::Error),
    #[error(transparent)]
    AddrParseError(#[from] std::net::AddrParseError),
}

#[derive(Clone, Debug)]
pub struct ApiServer {
    config: Config,
    db_pool: DbPool,
}

impl ApiServer {
    pub fn new(config: Config, db_pool: DbPool) -> Self {
        Self { config, db_pool }
    }

    pub fn print_useful_info(&self) {
        // Print friendly startup message
        println!("\n🌐 AI Gateway starting up:");
        println!(
            "   🚀 HTTP server ready at: \x1b[36mhttp://{}:{}\x1b[0m",
            self.config.http.host, self.config.http.port
        );
        println!("\n🌐 Starting UI server...");
        println!(
            "   🚀 UI server ready at: \x1b[36mhttp://{}:{}\x1b[0m",
            self.config.http.host, self.config.ui.port
        );
        println!("\n🌐 Starting MCP server...");
        println!(
            "   🚀 MCP server ready at: \x1b[36mhttp://{}:{}/mcp\x1b[0m",
            self.config.http.host, self.config.http.port
        );

        println!("\n🌐 Starting OTEL gRPC collector...");
        println!(
            "   🚀 OTEL gRPC collector ready at: \x1b[36mhttp://{}:{}\x1b[0m",
            self.config.http.host, self.config.otel.port
        );

        println!("\n🌐 Starting Distri server...");
        println!(
            "   🚀 Distri server ready at: \x1b[36mhttp://{}:{}\x1b[0m",
            self.config.http.host, self.config.distri.port
        );

        // Add documentation and community links
        println!("\n📚 Where the cool kids hang out:");
        println!(
            "   🔍 Read the docs (if you're into that): \x1b[36mhttps://vllora.dev/docs\x1b[0m"
        );
        println!("   ⭐ Drop us a star: \x1b[36mhttps://github.com/vllora/vllora\x1b[0m");
        println!(
            "   🎮 Join our Slack (we have memes): \x1b[36mhttps://join.slack.com/t/vllora/shared_invite/zt-2haf5kj6a-d7NX6TFJUPX45w~Ag4dzlg\x1b[0m"
        );

        println!("\n⚡Quick Start ⚡");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
        println!(
            "\x1b[33mcurl -X POST \x1b[36mhttp://{}:{}/v1/chat/completions\x1b[33m \\\x1b[0m",
            self.config.http.host, self.config.http.port
        );
        println!("\x1b[33m  -H \x1b[32m\"Content-Type: application/json\"\x1b[33m \\\x1b[0m");
        println!("\x1b[33m  -d\x1b[0m \x1b[32m'{{");
        println!("    \"model\": \"gpt-4o-mini\",");
        println!("    \"messages\": [{{\"role\": \"user\", \"content\": \"Hello vLLora!\"}}]");
        println!("  }}'\x1b[0m");
        println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        println!("\n💫 Join the fun:");
        println!("   🌟 Star the repo (we'll notice!)");
        println!("   💬 Share your builds on Slack");
        println!("   🔥 Keep up with our shenanigans on X");
        println!();
    }
    pub async fn start(
        self,
        storage: Option<Arc<Mutex<InMemoryStorage>>>,
        project_trace_senders: Arc<BroadcastChannelManager>,
        run_span_buffer: Arc<RunSpanBuffer>,
        session: DbSession,
    ) -> Result<impl Future<Output = Result<(), ServerError>>, ServerError> {
        let cost_calculator = GatewayCostCalculator::new();
        let callback = if let Some(storage) = &storage {
            init_callback_handler(storage.clone(), cost_calculator.clone())
        } else {
            CallbackHandlerFn(None)
        };

        let server_config = self.clone();
        let server_config_for_closure = server_config.clone();

        let events_senders = Arc::new(Mutex::new(HashMap::new()));
        let project_traces_senders = project_trace_senders.clone();
        let session_manager = Arc::new(LocalSessionManager::default());
        let breakpoint_manager = Arc::new(BreakpointManager::new());
        let events_senders_container = Arc::new(
            EventsSendersContainer::new(events_senders)
                .with_breakpoint_manager(breakpoint_manager.clone()),
        );

        let breakpoint_manager_for_closure = breakpoint_manager.clone();

        let config = self.config.clone();
        let server = HttpServer::new(move || {
            let cors = Self::get_cors(CorsOptions::Permissive);
            Self::create_app_entry(
                cors,
                storage.clone(),
                server_config.config.guards.clone(),
                callback.clone(),
                cost_calculator.clone(),
                server_config_for_closure.db_pool.clone(),
                events_senders_container.clone(),
                project_traces_senders.clone(),
                run_span_buffer.clone(),
                session.clone(),
                session_manager.clone(),
                breakpoint_manager_for_closure.clone(),
                config.clone(),
            )
        })
        .bind((self.config.http.host.as_str(), self.config.http.port))?
        .run()
        .map_err(ServerError::Actix);

        let writer = Box::new(SqliteTraceWriterTransport::new(
            server_config.db_pool.clone(),
        )) as Box<dyn SpanWriterTransport>;

        let project_service = ProjectServiceImpl::new(server_config.db_pool.clone());
        let trace_service = TraceServiceServer::new(TelemetryTraceServiceImpl::new(
            writer,
            Box::new(ProjectTraceTenantResolver::new(project_service)),
            project_trace_senders.inner().clone(),
        ));

        // Create metrics service
        let sqlite_metrics_writer = Arc::new(SqliteMetricsWriterTransport::new(
            server_config.db_pool.clone(),
        ));
        let metrics_writer_adapter =
            Arc::new(SqliteMetricsWriterAdapter::new(sqlite_metrics_writer));
        let metrics_project_service = ProjectServiceImpl::new(server_config.db_pool.clone());
        let metrics_service = MetricsServiceServer::new(MetricsServiceImpl::new(
            metrics_writer_adapter,
            Box::new(ProjectTraceTenantResolver::new(metrics_project_service)),
        ));

        let tonic_server = tonic::transport::Server::builder()
            .add_service(trace_service)
            .add_service(metrics_service)
            .serve_with_shutdown(
                format!("{}:{}", self.config.otel.host, self.config.otel.port).parse()?,
                async {
                    signal::ctrl_c().await.expect("failed to listen for ctrl+c");
                },
            );

        let tonic_fut = tonic_server.map_err(ServerError::Tonic);

        // Initialize and start finetune job state tracker
        let key_storage_for_tracker = Arc::new(Box::new(ProviderKeyResolver::new(
            server_config.db_pool.clone(),
        )) as Box<dyn KeyStorage>);
        let state_tracker =
            FinetuneJobStateTracker::new(server_config.db_pool.clone(), key_storage_for_tracker);
        let _state_tracker_handle = state_tracker.start();

        // Initialize and start eval job state tracker
        let key_storage_for_eval_tracker = Arc::new(Box::new(ProviderKeyResolver::new(
            server_config.db_pool.clone(),
        )) as Box<dyn KeyStorage>);
        let eval_state_tracker =
            EvalJobStateTracker::new(server_config.db_pool.clone(), key_storage_for_eval_tracker);
        let _eval_state_tracker_handle = eval_state_tracker.start();

        // Start background embedding backfill/resume loop for knowledge source parts.
        let _embedding_backfill_handle =
            start_embedding_backfill_job(server_config.db_pool.clone());

        // Print useful info after servers are bound and ready
        self.print_useful_info();

        Ok(try_join(server, tonic_fut).map_ok(|_| ()))
    }

    #[allow(clippy::too_many_arguments)]
    fn create_app_entry(
        cors: Cors,
        in_memory_storage: Option<Arc<Mutex<InMemoryStorage>>>,
        guards: Option<HashMap<String, Guard>>,
        callback: CallbackHandlerFn,
        cost_calculator: GatewayCostCalculator,
        db_pool: DbPool,
        events_senders_container: Arc<EventsSendersContainer>,
        project_trace_senders: Arc<BroadcastChannelManager>,
        run_span_buffer: Arc<RunSpanBuffer>,
        session: DbSession,
        session_manager: Arc<LocalSessionManager>,
        breakpoint_manager: Arc<BreakpointManager>,
        config: Config,
    ) -> App<
        impl ServiceFactory<
            ServiceRequest,
            Response = ServiceResponse<impl MessageBody>,
            Config = (),
            InitError = (),
            Error = actix_web::Error,
        >,
    > {
        let app = App::new().app_data(web::Data::new(db_pool.clone()));

        let mut service = Self::attach_gateway_routes(web::scope("/v1"));
        if let Some(in_memory_storage) = in_memory_storage {
            service = service.app_data(in_memory_storage);
        }

        let lucy_service = Self::attach_gateway_routes(web::scope("/lucy/v1"));

        let guardrails_service =
            Arc::new(Box::new(GuardrailsService::new(guards.unwrap_or_default()))
                as Box<dyn GuardrailsEvaluator>);

        let broadcaster = EventsUIBroadcaster::new(events_senders_container.clone());

        let callback_handler = GatewayCallbackHandlerFn::new(vec![], Some(broadcaster.clone()));
        let key_storage =
            Box::new(ProviderKeyResolver::new(db_pool.clone())) as Box<dyn KeyStorage>;

        let model_service =
            Box::new(ModelServiceImpl::new(db_pool.clone())) as Box<dyn ModelService>;

        let database_service = DatabaseService::new(db_pool.clone());

        let mcp_scope =
            vllora_core::mcp::server::service::attach_vllora_mcp::<MetadataTraceServiceImpl>(
                web::scope("/mcp"),
                session_manager.clone(),
                &database_service,
            );

        let json_config = JsonConfig::default().limit(50 * 1024 * 1024); // 50MB — supports base64-encoded file content blocks in chat completions

        app.wrap(TraceLogger)
            .wrap(ThreadId)
            .wrap(RunId)
            .wrap(ThreadsServiceMiddleware::new())
            .wrap(ProjectMiddleware::new())
            .app_data(json_config)
            .app_data(Data::new(broadcaster))
            .app_data(web::Data::from(project_trace_senders))
            .app_data(web::Data::from(run_span_buffer))
            .app_data(Data::new(callback_handler))
            .app_data(Data::new(key_storage))
            .app_data(Data::new(session))
            .app_data(Data::new(database_service))
            .app_data(Data::from(breakpoint_manager.clone()))
            .app_data(Data::new(model_service))
            .app_data(Data::new(config))
            .service(
                service
                    .app_data(Data::new(callback.clone()))
                    .app_data(Data::new(
                        Box::new(cost_calculator.clone()) as Box<dyn CostCalculator>
                    ))
                    .app_data(Data::from(guardrails_service.clone()))
                    .wrap(CloudApiInvokeMiddleware)
                    .wrap(RunSpanMiddleware)
                    .wrap(TracingContext)
                    .wrap(RateLimitMiddleware),
            )
            .service(
                lucy_service
                    .app_data(Data::new(callback))
                    .app_data(Data::new(
                        Box::new(cost_calculator.clone()) as Box<dyn CostCalculator>
                    ))
                    .app_data(Data::from(guardrails_service.clone()))
                    .wrap(CloudApiInvokeMiddleware)
                    .wrap(RunSpanMiddleware)
                    .wrap(TracingContext)
                    .wrap(RateLimitMiddleware)
                    .wrap(LucyProjectMiddleware),
            )
            .service(
                web::scope("/projects")
                    .route("", web::get().to(projects::list_projects))
                    .route("", web::post().to(projects::create_project))
                    .route("/{id}", web::get().to(projects::get_project))
                    .route("/{id}", web::delete().to(projects::delete_project))
                    .route("/{id}", web::put().to(projects::update_project))
                    .route(
                        "/{id}/default",
                        web::post().to(projects::set_default_project),
                    ),
            )
            .service(
                web::scope("/finetune")
                    .service(
                        web::scope("/workflows")
                            .route("", web::get().to(workflows::list_workflows))
                            .route("", web::post().to(workflows::create_workflow))
                            .service(
                                web::scope("/{workflow_id}")
                                    .route("", web::get().to(workflows::get_workflow))
                                    .route("", web::put().to(workflows::update_workflow))
                                    .route("", web::delete().to(workflows::soft_delete_workflow))
                                    // Records CRUD
                                    .service(
                                        web::scope("/records")
                                            .route("", web::get().to(workflow_records::list_records))
                                            .route("/count", web::get().to(workflow_records::count_records))
                                            .route("", web::post().to(workflow_records::add_records))
                                            .route("", web::put().to(workflow_records::replace_records))
                                            .route("", web::delete().to(workflow_records::delete_all_records))
                                            .route("/topics", web::patch().to(workflow_records::batch_update_topics))
                                            .route("/topics", web::delete().to(workflow_records::clear_all_topics))
                                            .route("/rename-topic", web::patch().to(workflow_records::rename_topic))
                                            .route("/topics/{topic_id}", web::delete().to(workflow_records::clear_topic))
                                            .route("/{record_id}", web::patch().to(workflow_records::update_record_topic))
                                            .route("/{record_id}", web::delete().to(workflow_records::delete_record))
                                            .route("/{record_id}/data", web::patch().to(workflow_records::update_record_data))
                                            .route("/scores", web::get().to(workflow_records::list_record_scores)),
                                    )
                                    .service(
                                        web::scope("/logs")
                                            .route(
                                                "",
                                                web::get().to(workflow_logs::list_workflow_logs),
                                            )
                                            .route(
                                                "/bulk",
                                                web::post()
                                                    .to(workflow_logs::create_workflow_logs_bulk),
                                            ),
                                    )
                                    // Pipeline Journal
                                    .service(
                                        web::scope("/journal")
                                            .route(
                                                "",
                                                web::get().to(workflows::get_pipeline_journal),
                                            )
                                            .route(
                                                "/entries",
                                                web::post()
                                                    .to(workflows::append_journal_entries),
                                            ),
                                    )
                                    // Topics CRUD
                                    .route("/topics", web::get().to(workflow_topics::list_topics))
                                    .route("/topics", web::post().to(workflow_topics::create_topics))
                                    .route("/topics", web::put().to(workflow_topics::update_topics))
                                    .route("/topics", web::delete().to(workflow_topics::delete_topics))
                                    .route("/topics/relations", web::get().to(workflow_topics::list_topic_source_relations))
                                    .route("/topics/relations", web::post().to(workflow_topics::create_topic_source_relations))
                                    .route("/topics/relations", web::put().to(workflow_topics::update_topic_source_relations))
                                    .route("/topics/relations", web::delete().to(workflow_topics::delete_topic_source_relations))
                                    .route(
                                        "/topics/generate",
                                        web::post().to(workflows::generate_workflow_topics),
                                    )
                                    // Knowledge Sources CRUD
                                    .service(
                                        web::scope("/knowledge")
                                            .route("", web::get().to(knowledge_sources::list_knowledge_sources))
                                            .route("", web::post().to(knowledge_sources::create_knowledge_source))
                                            .route("", web::put().to(knowledge_sources::upsert_knowledge_source))
                                            .route("", web::delete().to(knowledge_sources::soft_delete_all_knowledge_sources))
                                            .route("/count", web::get().to(knowledge_sources::count_knowledge_sources))
                                            .route("/search", web::post().to(knowledge_sources::search_knowledge_source_parts))
                                            .route("/chunk", web::post().to(workflows::chunk_workflow_knowledge))
                                            .route("/trace", web::post().to(workflows::create_workflow_knowledge_trace))
                                            .route("/trace/{trace_id}", web::delete().to(workflows::delete_workflow_knowledge_trace))
                                            .route("/{ks_id}", web::get().to(knowledge_sources::get_knowledge_source))
                                            .route("/{ks_id}", web::delete().to(knowledge_sources::soft_delete_knowledge_source))
                                            .route("/{ks_id}/file", web::get().to(knowledge_sources::download_knowledge_source_file))
                                            .route("/{ks_id}/parts", web::post().to(knowledge_sources::add_knowledge_source_parts))
                                            .route("/{ks_id}/parts", web::get().to(knowledge_sources::list_knowledge_source_parts))
                                            .route("/{ks_id}/parts", web::patch().to(knowledge_sources::update_parts_metadata))
                                            .route("/{ks_id}/parts/{part_id}", web::delete().to(knowledge_sources::delete_knowledge_source_part)),
                                    )
                                    // Workflow-scoped evaluation metadata (local DB)
                                    .service(
                                        web::scope("/evaluations")
                                            .route("", web::get().to(eval_jobs::list_eval_jobs))
                                            .route(
                                                "/metrics",
                                                web::get()
                                                    .to(finetune::get_workflow_evaluation_metrics),
                                            )
                                            .route(
                                                "",
                                                web::delete().to(eval_jobs::delete_workflow_eval_jobs),
                                            )
                                            .route("/{job_id}", web::get().to(eval_jobs::get_eval_job)),
                                    )
                                    // Dataset (cloud JSONL) - keep existing placeholders
                                    .route(
                                        "/dataset/generate",
                                        web::post().to(workflows::generate_workflow_dataset),
                                    )
                                    .route(
                                        "/dataset/generate/status",
                                        web::post()
                                            .to(workflows::get_workflow_dataset_generate_status),
                                    )
                                    .route(
                                        "/evaluator/run",
                                        web::post().to(workflows::run_workflow_evaluator),
                                    )
                                    .route(
                                        "/evaluator/run/status",
                                        web::get().to(workflows::get_workflow_evaluator_run_status),
                                    )
                                    .service(
                                        web::scope("/evaluator")
                                            .route(
                                                "",
                                                web::patch()
                                                    .to(finetune::update_workflow_evaluator),
                                            )
                                            .route(
                                                "/dry-run",
                                                web::post()
                                                    .to(finetune::dry_run_workflow_evaluator),
                                            )
                                            .route(
                                                "/versions",
                                                web::get()
                                                    .to(finetune::get_workflow_evaluator_versions),
                                            ),
                                    )
                                    .service(
                                        web::scope("/jobs")
                                            .route(
                                                "/estimate",
                                                web::post().to(finetune::estimate_job),
                                            )
                                            .route(
                                                "",
                                                web::post().to(finetune::create_job),
                                            )
                                            .route(
                                                "",
                                                web::get().to(finetune::list_finetune_jobs),
                                            )
                                            .service(
                                                web::scope("/{job_id}")
                                                    .route(
                                                        "/status",
                                                        web::get().to(
                                                            finetune::get_job_status,
                                                        ),
                                                    )
                                                    .route(
                                                        "/metrics",
                                                        web::get().to(
                                                            finetune::get_finetune_job_metrics,
                                                        ),
                                                    )
                                                    .route(
                                                        "/models",
                                                        web::get()
                                                            .to(finetune::get_finetune_job_models),
                                                    )
                                                    .route(
                                                        "/cancel",
                                                        web::post()
                                                            .to(finetune::cancel_finetune_job),
                                                    )
                                                    .route(
                                                        "/resume",
                                                        web::post()
                                                            .to(finetune::resume_finetune_job),
                                                    )
                                                    .route(
                                                        "/weights/url",
                                                        web::get()
                                                            .to(finetune::get_weights_download_url),
                                                    ),
                                            ),
                                    )
                                    // Cloud analytics & finetune evaluations (read-only proxies)
                                    .route(
                                        "/analytics",
                                        web::get().to(finetune::get_dataset_analytics),
                                    )
                                    .route(
                                        "/finetune-evaluations",
                                        web::get().to(finetune::get_finetune_evaluations),
                                    )
                                    .route(
                                        "/finetune-evaluations/metrics",
                                        web::get().to(finetune::get_finetune_evaluations_metrics),
                                    )
                                    .route(
                                        "/evaluations/metrics",
                                        web::get().to(finetune::get_workflow_evaluation_metrics),
                                    ),
                            ),
                    )
                    // NOTE: POST /finetune/datasets (upload) was removed — gateway auto-uploads
                    // via ensure_dataset_uploaded() inside create_evaluation / create_finetune_job.
                    .service(
                        web::scope("/analytics")
                            .route(
                                "/dry-run",
                                web::post().to(finetune::dry_run_dataset_analytics),
                            ),
                    )
                    .service(
                        web::scope("/evaluations")
                            .route("", web::post().to(finetune::create_evaluation))
                            .route(
                                "/{evaluation_run_id}",
                                web::get().to(finetune::get_evaluation_result),
                            ),
                    )
                    .service(
                        web::scope("/deployments")
                            .route("", web::post().to(finetune::deploy_model))
                            .route(
                                "/{deployment_id}",
                                web::delete().to(finetune::delete_deployment),
                            ),
                    )
                    .service(
                        web::scope("/topic-hierarchy")
                            .route(
                                "/generate",
                                web::post().to(finetune::generate_topic_hierarchy),
                            )
                            .route("/adjust", web::post().to(finetune::adjust_topic_hierarchy)),
                    )
                    .app_data(Data::from(guardrails_service.clone()))
                    .app_data(Data::new(
                        Box::new(cost_calculator.clone()) as Box<dyn CostCalculator>
                    )),
            )
            .service(
                web::scope("/providers")
                    .route(
                        "",
                        web::get().to(vllora_core::handler::providers::list_providers::<
                            ProvidersServiceImpl,
                        >),
                    )
                    .route(
                        "",
                        web::post().to(
                            vllora_core::handler::providers::create_provider_definition::<
                                ProvidersServiceImpl,
                            >,
                        ),
                    )
                    .route(
                        "/{provider_name}",
                        web::put().to(vllora_core::handler::providers::update_provider_key::<
                            ProvidersServiceImpl,
                        >),
                    )
                    .route(
                        "/{provider_name}",
                        web::delete().to(vllora_core::handler::providers::delete_provider),
                    )
                    .route(
                        "/definitions/{id}",
                        web::get().to(vllora_core::handler::providers::get_provider_definition::<
                            ProvidersServiceImpl,
                        >),
                    )
                    .route(
                        "/definitions/{id}",
                        web::put().to(
                            vllora_core::handler::providers::update_provider_definition::<
                                ProvidersServiceImpl,
                            >,
                        ),
                    )
                    .route(
                        "/definitions/{id}",
                        web::delete().to(
                            vllora_core::handler::providers::delete_provider_definition::<
                                ProvidersServiceImpl,
                            >,
                        ),
                    ),
            )
            .service(
                web::scope("/threads")
                    .route(
                        "",
                        web::get().to(vllora_core::handler::threads::list_threads),
                    )
                    .route(
                        "",
                        web::post().to(vllora_core::handler::threads::list_threads),
                    )
                    .route("/{id}", web::get().to(threads::get_thread))
                    .route("/{id}", web::put().to(threads::update_thread)),
            )
            .service(
                web::scope("/events")
                    .route(
                        "",
                        web::get().to(vllora_core::handler::events::stream_events),
                    )
                    .route(
                        "",
                        web::post().to(vllora_core::handler::events::send_events),
                    ),
            )
            .service(web::scope("/spans").route(
                "",
                web::get().to(spans::list_spans::<MetadataTraceServiceImpl>),
            ))
            .service(web::scope("/labels").route("", web::get().to(labels::list_labels)))
            .service(
                web::scope("/mcp-configs")
                    .route("", web::get().to(mcp_configs::list_mcp_configs))
                    .route("", web::post().to(mcp_configs::upsert_mcp_config))
                    .route("/tools", web::post().to(mcp_configs::get_mcp_config_tools))
                    .route("/{id}", web::get().to(mcp_configs::get_mcp_config))
                    .route(
                        "/{id}/tools",
                        web::get().to(mcp_configs::update_mcp_config_tools),
                    )
                    .route("/{id}", web::delete().to(mcp_configs::delete_mcp_config))
                    .route("/{id}", web::put().to(mcp_configs::update_mcp_config)),
            )
            .service(web::scope("/traces").route(
                "",
                web::get().to(traces::list_traces::<MetadataTraceServiceImpl>),
            ))
            .service(
                web::scope("/runs")
                    .route("", web::get().to(runs::list_root_runs::<RunServiceImpl>))
                    .route(
                        "/{run_id}",
                        web::get().to(traces::get_spans_by_run::<MetadataTraceServiceImpl>),
                    )
                    .route(
                        "/{run_id}/details",
                        web::get().to(runs::run_by_id::<RunServiceImpl>),
                    ),
            )
            .service(
                web::scope("/group")
                    .route(
                        "",
                        web::get().to(group::list_root_group::<GroupServiceImpl>),
                    )
                    .route(
                        "/spans",
                        web::get().to(group::get_group_spans::<MetadataTraceServiceImpl>),
                    ) // Unified endpoint for all group types
                    .route(
                        "/batch-spans",
                        web::post().to(group::get_batch_group_spans::<MetadataTraceServiceImpl>),
                    ), // Batch endpoint for multiple groups
            )
            .service(
                web::scope("/session")
                    .route("/track", web::post().to(session::track_session))
                    .route("/start", web::post().to(session::start_session))
                    .route("/fetch_key/{session_id}", web::get().to(session::fetch_key)),
            )
            .service(
                web::scope("/agents")
                    .route("/register", web::post().to(agents::register_agents))
                    .route("/config", web::get().to(agents::get_lucy_config)),
            )
            .service(
                web::scope("/debug")
                    .route("/continue", web::post().to(debug::continue_breakpoint))
                    .route(
                        "/continue/all",
                        web::post().to(debug::continue_all_breakpoints),
                    )
                    .route("/breakpoints", web::get().to(debug::list_breakpoints))
                    .route(
                        "/global_breakpoint",
                        web::post().to(debug::set_global_breakpoint),
                    ),
            )
            .service(
                web::scope("/models")
                    .route(
                        "",
                        web::post()
                            .to(vllora_core::handler::models::create_model::<ModelServiceImpl>),
                    )
                    .route(
                        "/custom/{name}",
                        web::delete().to(
                            vllora_core::handler::models::delete_custom_model_by_name::<
                                ModelServiceImpl,
                            >,
                        ),
                    )
                    .route(
                        "/{id}",
                        web::get().to(vllora_core::handler::models::get_model::<ModelServiceImpl>),
                    )
                    .route(
                        "/{id}",
                        web::put()
                            .to(vllora_core::handler::models::update_model::<ModelServiceImpl>),
                    )
                    .route(
                        "/{id}",
                        web::delete()
                            .to(vllora_core::handler::models::delete_model::<ModelServiceImpl>),
                    ),
            )
            .service(mcp_scope)
            .wrap(cors)
    }

    fn get_cors(cors: CorsOptions) -> Cors {
        match cors {
            CorsOptions::Permissive => Cors::permissive(),
            CorsOptions::Custom(origins, max_age) => origins
                .into_iter()
                .fold(Cors::default(), |cors, origin| cors.allowed_origin(&origin))
                .max_age(max_age),
        }
    }

    fn attach_gateway_routes(scope: ActixScope) -> ActixScope {
        scope
            .route("/chat/completions", web::post().to(create_chat_completion))
            .route(
                "/models",
                web::get().to(crate::handlers::list_models_from_db),
            )
            .route("/pricing", web::get().to(models::list_gateway_pricing))
            .route("/embeddings", web::post().to(embeddings_handler))
            .route("/images/generations", web::post().to(create_image))
            .route("/responses", web::post().to(responses::create))
    }
}
