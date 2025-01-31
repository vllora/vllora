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
use langdb_core::usage::connection_manager::redis_connection_manager;
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
use langdb_core::otel::database::DatabaseSpanWritter;
use langdb_core::otel::SpanWriterTransport;

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
    #[error("Failed to connect to redis: {0}")]
    FailedToConnectToRedis(String),
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
    ) -> Result<impl Future<Output = Result<(), ServerError>>, ServerError> {
        let trace_senders = Arc::new(TraceMap::new());
        let trace_senders_inner = Arc::clone(&trace_senders);

        let cost_calculator = GatewayCostCalculator::new(models.clone());
        let (callback, redis_manager) = if let Some(redis_config) = self.config.redis {
            let redis_manager = redis_connection_manager(redis_config.url)
                .await
                .map_err(|e| ServerError::FailedToConnectToRedis(e.to_string()))?;
            let callback = init_callback_handler(redis_manager.clone(), cost_calculator.clone());

            (callback, Some(redis_manager))
        } else {
            (CallbackHandlerFn(None), None)
        };

        let server = HttpServer::new(move || {
            let limit_checker = if let Some(manager) = redis_manager.clone() {
                match &self.config.cost_control {
                    Some(cc) => {
                        let checker =
                            GatewayLimitChecker::new(Arc::new(Mutex::new(manager)), cc.clone());
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
                redis_manager.clone(),
                trace_senders_inner.clone(),
                models.clone(),
                callback.clone(),
                cost_calculator.clone(),
                limit_checker.clone(),
                self.config.rate_limit.clone(),
            )
        })
        .bind((self.config.rest.host.as_str(), self.config.rest.port))?
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
            .serve_with_shutdown("[::]:4317".parse().unwrap(), async {
                signal::ctrl_c().await.expect("failed to listen for ctrl+c");
            });

        let tonic_fut = tonic_server.map_err(ServerError::Tonic);
        Ok(try_join(server, tonic_fut).map_ok(|_| ()))
    }

    #[allow(clippy::too_many_arguments)]
    fn create_app_entry(
        cors: Cors,
        redis_manager: Option<langdb_core::redis::aio::ConnectionManager>,
        trace_senders: Arc<TraceMap>,
        models: Vec<ModelDefinition>,
        callback: CallbackHandlerFn,
        cost_calculator: GatewayCostCalculator,
        limit_checker: Option<LimitCheckWrapper>,
        rate_limit: Option<RateLimiting>,
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
        if let Some(redis_manager) = redis_manager {
            service = service.app_data(Data::new(Mutex::new(redis_manager)));
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
