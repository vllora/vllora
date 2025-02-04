use actix_cors::Cors;
use actix_web::middleware::Logger;
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
use crate::config::Config;
use crate::cost::GatewayCostCalculator;
use crate::limit::GatewayLimitChecker;
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

    pub async fn start(
        self,
        models: Vec<ModelDefinition>,
        storage: Option<Arc<Mutex<InMemoryStorage>>>,
    ) -> Result<impl Future<Output = Result<(), ServerError>>, ServerError> {
        let trace_senders = Arc::new(TraceMap::new());
        let trace_senders_inner = Arc::clone(&trace_senders);

        let cost_calculator = GatewayCostCalculator::new(models.clone());
        let callback = if let Some(storage) = &storage {
            init_callback_handler(storage.clone(), cost_calculator.clone())
        } else {
            CallbackHandlerFn(None)
        };

        let server = HttpServer::new(move || {
            let limit_checker = if let Some(storage) = storage.clone() {
                match &self.config.cost_control {
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

            let cors = Self::get_cors(CorsOptions::Permissive);
            Self::create_app_entry(
                cors,
                storage.clone(),
                trace_senders_inner.clone(),
                models.clone(),
                callback.clone(),
                cost_calculator.clone(),
                limit_checker.clone(),
                self.config.rate_limit.clone(),
                self.config.providers.clone(),
            )
        })
        .bind((self.config.http.host.as_str(), self.config.http.port))?
        .run()
        .map_err(ServerError::Actix);

        let writer = match self.config.clickhouse {
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

        app.wrap(Logger::default())
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
