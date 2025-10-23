use crate::events::ui_broadcaster::EventsUIBroadcaster;
use crate::events::{CustomEventType, Event, EventRunContext};
use crate::telemetry::events::JsonValue;
use crate::types::metadata::project::Project;
use crate::types::threads::{CompletionsRunId, CompletionsThreadId};
use actix_web::dev::forward_ready;
use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};
use opentelemetry::trace::TraceContextExt;
use opentelemetry::SpanId;
use std::collections::HashMap;
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::rc::Rc;
use tracing::{field, Span};
use tracing_futures::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use valuable::Valuable;

use actix_web::{web, HttpMessage, HttpRequest};

use opentelemetry_semantic_conventions::trace::HTTP_RESPONSE_STATUS_CODE;

pub fn get_client_ip(req: &HttpRequest) -> String {
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
            let extensions = req.extensions();
            let run_id = extensions.get::<CompletionsRunId>().cloned();
            let thread_id = extensions.get::<CompletionsThreadId>().cloned();
            let broadcaster: Option<web::Data<EventsUIBroadcaster>> = req.app_data().cloned();
            let project = extensions.get::<Project>().cloned();
            let context = extensions.get::<opentelemetry::Context>().cloned();

            drop(extensions);

            let run_id_clone = run_id.clone();
            let thread_id_clone = thread_id.clone();
            let broadcaster_clone = broadcaster.clone();
            let project_clone = project.clone();
            let context_clone = context.clone();

            let execution_fn = async move |parent_span_id: Option<SpanId>| {
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

                let parent_span_id = match context {
                    Some(context) => {
                        let context_span = context.span();
                        let span_context = context_span.span_context();
                        if span_context.is_valid() {
                            Some(span_context.span_id())
                        } else {
                            parent_span_id
                        }
                    }
                    None => parent_span_id,
                };

                if let (Some(run_id), Some(thread_id)) = (run_id, thread_id) {
                    let event = Event::Custom {
                        run_context: EventRunContext {
                            run_id: Some(run_id.value()),
                            thread_id: Some(thread_id.value()),
                            span_id: Some(span.context().span().span_context().span_id()),
                            parent_span_id,
                        },
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                        custom_event: CustomEventType::SpanStart {
                            operation_name: "cloud_api_invoke".to_string(),
                            attributes: serde_json::json!({}),
                        },
                    };

                    if let (Some(broadcaster), Some(project)) = (broadcaster, project) {
                        broadcaster
                            .send_events(&project.slug.to_string(), &[event])
                            .await;
                    }
                }

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

            let is_remote_context = if let Some(context) = context_clone {
                context.span().span_context().is_remote()
            } else {
                false
            };

            if is_remote_context {
                // If the context is remote, we don't need to start a new span for the run
                execution_fn(None).await
            } else {
                // If the context is local, we need to start a new span for the run
                let span = tracing::info_span!(
                    target: "langdb::user_tracing::run",
                    "run",
                )
                .clone();

                let mut span_id = None;
                if let (Some(run_id), Some(thread_id)) = (run_id_clone, thread_id_clone) {
                    span_id = Some(span.context().span().span_context().span_id());
                    let event = Event::RunStarted {
                        run_context: EventRunContext {
                            run_id: Some(run_id.value()),
                            thread_id: Some(thread_id.value()),
                            span_id,
                            parent_span_id: None,
                        },
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                    };

                    if let (Some(broadcaster), Some(project)) = (broadcaster_clone, project_clone) {
                        broadcaster
                            .send_events(&project.slug.to_string(), &[event])
                            .await;
                    }
                }

                execution_fn(span_id).instrument(span).await
            }
        })
    }
}
