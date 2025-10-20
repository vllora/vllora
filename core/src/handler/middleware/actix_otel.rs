use crate::telemetry::events::JsonValue;
use actix_web::dev::forward_ready;
use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};
use opentelemetry::trace::TraceContextExt;
use std::collections::HashMap;
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::rc::Rc;
use tracing::{field, Span};
use tracing_futures::Instrument;
use valuable::Valuable;

use actix_web::{HttpMessage, HttpRequest};

use opentelemetry_semantic_conventions::trace::HTTP_RESPONSE_STATUS_CODE;

pub(crate) fn get_client_ip(req: &HttpRequest) -> String {
    let header = req
        .headers()
        .get("x-client-ip")
        .or_else(|| req.headers().get("X-Forwarded-For"));
    if let Some(header) = header {
        if let Ok(header_str) = header.to_str() {
            if let Some(first_ip) = header_str.split(',').next() {
                let ip = first_ip.trim().to_string();
                if let Some(ip) = ip.split(":").next() {
                    return ip.to_string();
                }
            }
        }
    }

    // Fall back to connection info
    req.connection_info()
        .realip_remote_addr()
        .unwrap_or("unknown")
        .to_string()
}

pub struct ActixOtelMiddleware;

impl<S, B> Transform<S, ServiceRequest> for ActixOtelMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = ActixOtelMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ActixOtelMiddlewareService {
            service: service.into(),
        }))
    }
}
pub struct ActixOtelMiddlewareService<S> {
    service: Rc<S>,
}

const IGNORED_HEADERS: [&str; 5] = [
    "authorization",
    "cookie",
    "x-amzn-trace-id",
    "x-amz-cf-id",
    "via",
];

type LocalBoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;

impl<S, B> Service<ServiceRequest> for ActixOtelMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = Rc::clone(&self.service);

        Box::pin(async move {
            let context = req.extensions().get::<opentelemetry::Context>().cloned();

            let execution_fn = async move || {
                let span = tracing::info_span!(
                    target: "langdb::user_tracing::cloud_api",
                    "cloud_api_invoke",
                    http.request.method = req.method().to_string(),
                    http.request.path = req.path().to_string(),
                    http.request.header = JsonValue(
                        &serde_json::to_value(
                            req.headers()
                                .iter()
                                .filter(|(k, _)| !IGNORED_HEADERS.contains(&k.as_str()))
                                .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or_default())).collect::<HashMap<_, _>>()
                        ).unwrap_or_default()
                    ).as_value(),
                    http.response.status_code = field::Empty,
                    status = field::Empty,
                    ip = get_client_ip(req.request())
                );

                async move {
                    // Proceed with the request if within limits
                    match service.call(req).await {
                        Ok(ok_res) => {
                            let span = Span::current();
                            span.record(HTTP_RESPONSE_STATUS_CODE, ok_res.status().as_u16() as i64);
                            span.record("status", ok_res.status().as_u16() as i64);
                            if ok_res.status().is_server_error() {
                                span.record(
                                    "status",
                                    ok_res
                                        .status()
                                        .canonical_reason()
                                        .map(ToString::to_string)
                                        .unwrap_or_default(),
                                );
                            };
                            Ok(ok_res)
                        }
                        Err(err) => {
                            let span = Span::current();
                            span.record("status", format!("err {err:?}"));
                            Err(err)
                        }
                    }
                }
                .instrument(span.clone())
                .await
            };

            let is_remote_context = if let Some(context) = context {
                context.span().span_context().is_remote()
            } else {
                false
            };

            if is_remote_context {
                // If the context is remote, we don't need to start a new span for the run
                execution_fn().await
            } else {
                // If the context is local, we need to start a new span for the run
                let span = tracing::info_span!(
                    target: "langdb::user_tracing::run",
                    "run",
                )
                .clone();
                execution_fn().instrument(span).await
            }
        })
    }
}
