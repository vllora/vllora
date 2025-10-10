use crate::callback_handler::init_callback_handler;
use crate::config::{load_langdb_proxy_config, Config};
use crate::cost::GatewayCostCalculator;
use crate::guardrails::GuardrailsService;
use crate::handlers::threads;
use crate::handlers::{models, projects, providers, runs, session, traces};
use crate::limit::GatewayLimitChecker;
use crate::middleware::project::ProjectMiddleware;
use crate::middleware::run_id::RunId;
use crate::middleware::thread_id::ThreadId;
use crate::middleware::trace_logger::TraceLogger;
use crate::middleware::tracing_context::TracingContext;
use actix_cors::Cors;
use actix_web::Scope as ActixScope;
use actix_web::{
    body::MessageBody,
    dev::{ServiceFactory, ServiceRequest, ServiceResponse},
    web::{self, Data},
    App, HttpServer,
};
use futures::{future::try_join, Future, TryFutureExt};
use langdb_core::credentials::KeyStorage;
use langdb_core::credentials::ProviderKeyResolver;
use langdb_core::database::clickhouse::ClickhouseHttp;
use langdb_core::database::DatabaseTransportClone;
use langdb_core::events::broadcast_channel_manager::BroadcastChannelManager;
use langdb_core::events::callback_handler::GatewayCallbackHandlerFn;
use langdb_core::events::ui_broadcaster::EventsSendersContainer;
use langdb_core::events::ui_broadcaster::EventsUIBroadcaster;
use langdb_core::executor::ProvidersConfig;
use langdb_core::handler::chat::create_chat_completion;
use langdb_core::handler::embedding::embeddings_handler;
use langdb_core::handler::image::create_image;
use langdb_core::handler::middleware::actix_otel::ActixOtelMiddleware;
use langdb_core::handler::middleware::rate_limit::{RateLimitMiddleware, RateLimiting};
use langdb_core::handler::{CallbackHandlerFn, LimitCheckWrapper};
use langdb_core::history::thread_entity::ThreadEntityImpl;
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::project_trace::ProjectTraceTenantResolver;
use langdb_core::metadata::services::model::ModelService;
use langdb_core::metadata::services::model::ModelServiceImpl;
use langdb_core::metadata::services::project::ProjectServiceImpl;
use langdb_core::telemetry::database::DatabaseSpanWritter;
use langdb_core::telemetry::database::SqliteTraceWriterTransport;
use langdb_core::telemetry::SpanWriterTransport;
use langdb_core::telemetry::{TraceServiceImpl, TraceServiceServer};
use langdb_core::types::gateway::CostCalculator;
use langdb_core::types::guardrails::service::GuardrailsEvaluator;
use langdb_core::types::guardrails::Guard;
use langdb_core::types::threads::ThreadEntity;
use langdb_core::usage::InMemoryStorage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::signal;
use tokio::sync::Mutex;

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
        println!("\n🚀 AI Gateway starting up:");
        println!(
            "   🌐 HTTP server ready at: \x1b[36mhttp://{}:{}\x1b[0m",
            self.config.http.host, self.config.http.port
        );
        println!(
            "   🚀 UI server ready at: \x1b[36mhttp://{}:{}\x1b[0m",
            self.config.http.host, 8084
        );

        // Add documentation and community links
        println!("\n📚 Where the cool kids hang out:");
        println!(
            "   🔍 Read the docs (if you're into that): \x1b[36mhttps://docs.langdb.ai\x1b[0m"
        );
        println!("   ⭐ Drop us a star: \x1b[36mhttps://github.com/langdb/ai-gateway\x1b[0m");
        println!(
            "   🎮 Join our Slack (we have memes): \x1b[36mhttps://join.slack.com/t/langdbcommunity/shared_invite/zt-2haf5kj6a-d7NX6TFJUPX45w~Ag4dzlg\x1b[0m"
        );
        println!("   🐦 Latest updates on X: \x1b[36mhttps://x.com/LangdbAi\x1b[0m");

        println!("\n⚡Quick Start ⚡");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
        println!(
            "\x1b[33mcurl -X POST \x1b[36mhttp://{}:{}/v1/chat/completions\x1b[33m \\\x1b[0m",
            self.config.http.host, self.config.http.port
        );
        println!("\x1b[33m  -H \x1b[32m\"Content-Type: application/json\"\x1b[33m \\\x1b[0m");
        println!("\x1b[33m  -d\x1b[0m \x1b[32m'{{");
        println!("    \"model\": \"gpt-4o-mini\",");
        println!("    \"messages\": [{{\"role\": \"user\", \"content\": \"Hello LangDB!\"}}]");
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
        let events_senders_container = Arc::new(EventsSendersContainer::new(events_senders));

        let project_traces_senders = project_trace_senders.clone();
        let server = HttpServer::new(move || {
            let limit_checker = if let Some(storage) = storage.clone() {
                match &server_config_for_closure.config.cost_control {
                    Some(cc) => {
                        let checker = GatewayLimitChecker::new(storage, cc.clone());
                        Some(LimitCheckWrapper {
                            checkers: vec![Arc::new(Mutex::new(checker))],
                        })
                    }
                    None => None,
                }
            } else {
                None
            };

            let providers_config = load_langdb_proxy_config(server_config.config.providers.clone());

            let cors = Self::get_cors(CorsOptions::Permissive);
            Self::create_app_entry(
                cors,
                storage.clone(),
                server_config.config.guards.clone(),
                callback.clone(),
                cost_calculator.clone(),
                limit_checker.clone(),
                server_config.config.rate_limit.clone(),
                providers_config,
                server_config_for_closure.db_pool.clone(),
                events_senders_container.clone(),
                project_traces_senders.clone(),
            )
        })
        .bind((self.config.http.host.as_str(), self.config.http.port))?
        .run()
        .map_err(ServerError::Actix);

        let writer = match server_config.config.clickhouse {
            Some(c) => {
                let client = ClickhouseHttp::root().with_url(&c.url).clone_box();
                Box::new(DatabaseSpanWritter::new(client)) as Box<dyn SpanWriterTransport>
            }
            None => Box::new(SqliteTraceWriterTransport::new(Arc::new(
                server_config.db_pool.clone(),
            ))) as Box<dyn SpanWriterTransport>,
        };

        let project_service = ProjectServiceImpl::new(server_config.db_pool.clone());
        let trace_service = TraceServiceServer::new(TraceServiceImpl::new(
            writer,
            Box::new(ProjectTraceTenantResolver::new(project_service)),
        ));
        let tonic_server = tonic::transport::Server::builder()
            .add_service(trace_service)
            .serve_with_shutdown("[::]:4317".parse()?, async {
                signal::ctrl_c().await.expect("failed to listen for ctrl+c");
            });

        let tonic_fut = tonic_server.map_err(ServerError::Tonic);

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
        limit_checker: Option<LimitCheckWrapper>,
        rate_limit: Option<RateLimiting>,
        providers: Option<ProvidersConfig>,
        db_pool: DbPool,
        events_senders_container: Arc<EventsSendersContainer>,
        project_trace_senders: Arc<BroadcastChannelManager>,
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

        if let Some(providers) = &providers {
            service = service.app_data(providers.clone());
        }

        let guardrails_service = Box::new(GuardrailsService::new(guards.unwrap_or_default()))
            as Box<dyn GuardrailsEvaluator>;

        let broadcaster = EventsUIBroadcaster::new(events_senders_container.clone());
        let thread_entity =
            Box::new(ThreadEntityImpl::new(db_pool.clone())) as Box<dyn ThreadEntity>;

        let callback_handler = GatewayCallbackHandlerFn::new(vec![], Some(broadcaster.clone()));
        let key_storage =
            Box::new(ProviderKeyResolver::new(db_pool.clone())) as Box<dyn KeyStorage>;

        let model_service =
            Box::new(ModelServiceImpl::new(db_pool.clone())) as Box<dyn ModelService>;

        app.wrap(TraceLogger)
            .wrap(ActixOtelMiddleware)
            .wrap(TracingContext)
            .wrap(ThreadId)
            .wrap(RunId)
            .wrap(ProjectMiddleware::new())
            .app_data(Data::new(broadcaster))
            .app_data(web::Data::from(project_trace_senders))
            .app_data(Data::new(thread_entity))
            .app_data(Data::new(callback_handler))
            .app_data(Data::new(key_storage))
            .service(
                service
                    .app_data(Data::new(model_service))
                    .app_data(limit_checker)
                    .app_data(Data::new(callback))
                    .app_data(Data::new(
                        Box::new(cost_calculator) as Box<dyn CostCalculator>
                    ))
                    .app_data(rate_limit)
                    .app_data(Data::new(guardrails_service))
                    .wrap(RateLimitMiddleware),
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
                web::scope("/providers")
                    .route("", web::get().to(providers::list_providers))
                    .route(
                        "/{provider_name}",
                        web::put().to(providers::update_provider),
                    )
                    .route(
                        "/{provider_name}",
                        web::delete().to(providers::delete_provider),
                    ),
            )
            .service(
                web::scope("/threads")
                    .route("", web::get().to(threads::list_threads))
                    .route("", web::post().to(threads::list_threads))
                    .route("/{id}", web::put().to(threads::update_thread))
                    .route(
                        "/{id}/messages",
                        web::get().to(threads::get_thread_messages),
                    )
                    .route(
                        "/{id}/messages/{message_id}",
                        web::get().to(threads::get_thread_message),
                    ),
            )
            .route(
                "/events",
                web::get().to(langdb_core::handler::events::stream_events),
            )
            .service(web::scope("/traces").route("", web::get().to(traces::list_traces)))
            .service(
                web::scope("/runs")
                    .route("", web::get().to(runs::list_runs))
                    .route("/{run_id}", web::get().to(traces::get_spans_by_run)),
            )
            .service(
                web::scope("/session")
                    .route("/start", web::post().to(session::start_session))
                    .route("/fetch_key/{session_id}", web::get().to(session::fetch_key)),
            )
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
    }
}
