use actix_cors::Cors;
use actix_web::Scope as ActixScope;
use actix_web::{
    body::MessageBody,
    dev::{ServiceFactory, ServiceRequest, ServiceResponse},
    web::{self, Data},
    App, HttpServer,
};
use futures::{future::try_join, Future, TryFutureExt};
use langdb_core::database::clickhouse::ClickhouseHttp;
use langdb_core::database::DatabaseTransportClone;
use langdb_core::handler::chat::create_chat_completion;
use langdb_core::handler::embedding::embeddings_handler;
use langdb_core::handler::image::create_image;
use langdb_core::handler::middleware::rate_limit::{RateLimitMiddleware, RateLimiting};
use langdb_core::handler::models::list_gateway_models;
use langdb_core::handler::{AvailableModels, CallbackHandlerFn, LimitCheckWrapper};
use langdb_core::models::ModelDefinition;
use langdb_core::otel::{TraceMap, TraceServiceImpl, TraceServiceServer};
use langdb_core::types::gateway::CostCalculator;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::signal;
use tokio::sync::Mutex;

use crate::callback_handler::init_callback_handler;
use crate::config::{load_langdb_proxy_config, Config};
use crate::cost::GatewayCostCalculator;
use crate::limit::GatewayLimitChecker;
use crate::middleware::trace_logger::TraceLogger;
use crate::otel::DummyTraceWritterTransport;
use langdb_core::executor::ProvidersConfig;
use langdb_core::otel::database::DatabaseSpanWritter;
use langdb_core::otel::SpanWriterTransport;
use langdb_core::usage::InMemoryStorage;

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
}

impl ApiServer {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn print_useful_info(&self) {
        // Print friendly startup message
        println!("\n🚀 AI Gateway starting up:");
        println!(
            "   🌐 HTTP server ready at: \x1b[36mhttp://{}:{}\x1b[0m",
            self.config.http.host, self.config.http.port
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
        models: Vec<ModelDefinition>,
        storage: Option<Arc<Mutex<InMemoryStorage>>>,
    ) -> Result<impl Future<Output = Result<(), ServerError>>, ServerError> {
        let trace_senders = Arc::new(TraceMap::new());
        let trace_senders_inner = Arc::clone(&trace_senders);
        let server_config = self.clone();

        let cost_calculator = GatewayCostCalculator::new(models.clone());
        let callback = if let Some(storage) = &storage {
            init_callback_handler(storage.clone(), cost_calculator.clone())
        } else {
            CallbackHandlerFn(None)
        };

        let server = HttpServer::new(move || {
            let limit_checker = if let Some(storage) = storage.clone() {
                match &server_config.config.cost_control {
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
                trace_senders_inner.clone(),
                models.clone(),
                callback.clone(),
                cost_calculator.clone(),
                limit_checker.clone(),
                server_config.config.rate_limit.clone(),
                providers_config,
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
            None => Box::new(DummyTraceWritterTransport {}) as Box<dyn SpanWriterTransport>,
        };

        let trace_service =
            TraceServiceServer::new(TraceServiceImpl::new(Arc::new(TraceMap::new()), writer));
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
        trace_senders: Arc<TraceMap>,
        models: Vec<ModelDefinition>,
        callback: CallbackHandlerFn,
        cost_calculator: GatewayCostCalculator,
        limit_checker: Option<LimitCheckWrapper>,
        rate_limit: Option<RateLimiting>,
        providers: Option<ProvidersConfig>,
    ) -> App<
        impl ServiceFactory<
            ServiceRequest,
            Response = ServiceResponse<impl MessageBody>,
            Config = (),
            InitError = (),
            Error = actix_web::Error,
        >,
    > {
        let app = App::new();

        let mut service = Self::attach_gateway_routes(web::scope("/v1"));
        if let Some(in_memory_storage) = in_memory_storage {
            service = service.app_data(in_memory_storage);
        }

        if let Some(providers) = &providers {
            service = service.app_data(providers.clone());
        }

        app.wrap(TraceLogger)
            .service(
                service
                    .app_data(limit_checker)
                    .app_data(Data::new(callback))
                    .app_data(web::Data::from(trace_senders.clone()))
                    .app_data(Data::new(AvailableModels(models)))
                    .app_data(Data::new(
                        Box::new(cost_calculator) as Box<dyn CostCalculator>
                    ))
                    .app_data(rate_limit)
                    .wrap(RateLimitMiddleware),
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
            .route("/models", web::get().to(list_gateway_models))
            .route("/embeddings", web::post().to(embeddings_handler))
            .route("/images/generations", web::post().to(create_image))
    }
}
