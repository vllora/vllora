use crate::events::ui_broadcaster::EventsUIBroadcaster;
use crate::types::metadata::project::Project;
use crate::types::threads::{CompletionsRunId, CompletionsThreadId};
use actix_web::body::{BodySize, BoxBody, MessageBody};
use actix_web::dev::forward_ready;
use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};
use bytes::Bytes;
use opentelemetry::trace::TraceContextExt;
use pin_project_lite::pin_project;
use std::collections::HashMap;
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use tracing::{field, Span};
use tracing_futures::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use valuable::Valuable;
use vllora_llm::types::events::{CustomEventType, Event, EventRunContext};
use vllora_telemetry::create_run_span;
use vllora_telemetry::events::JsonValue;

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
    B: MessageBody + 'static,
    B::Error: std::fmt::Display,
{
    type Response = ServiceResponse<BoxBody>;
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
    B: MessageBody + 'static,
    B::Error: std::fmt::Display,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = Rc::clone(&self.service);

        Box::pin(async move {
            let run_id = req.extensions().get::<CompletionsRunId>().cloned();
            let thread_id = req.extensions().get::<CompletionsThreadId>().cloned();
            let broadcaster: Option<web::Data<EventsUIBroadcaster>> = req.app_data().cloned();
            let project = req.extensions().get::<Project>().cloned();
            let request_context = req.extensions().get::<opentelemetry::Context>().cloned();

            let is_remote_context = if let Some(context) = request_context.as_ref() {
                context.span().span_context().is_remote()
            } else {
                false
            };

            let mut run_span = None;

            if !is_remote_context {
                // If the context is local, we need to start a new span for the run
                let span = create_run_span!({});

                run_span = Some(span.clone());

                let run_parent_span_id = match request_context {
                    Some(context) => {
                        let context_span = context.span();
                        let span_context = context_span.span_context();
                        if span_context.is_valid() {
                            Some(span_context.span_id())
                        } else {
                            None
                        }
                    }
                    None => None,
                };

                if let (Some(run_id), Some(thread_id)) = (run_id.as_ref(), thread_id.as_ref()) {
                    let parent_span_id = Some(span.context().span().span_context().span_id());
                    let event = Event::RunStarted {
                        run_context: EventRunContext {
                            run_id: Some(run_id.value()),
                            thread_id: Some(thread_id.value()),
                            span_id: parent_span_id,
                            parent_span_id: run_parent_span_id,
                        },
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                    };

                    if let (Some(broadcaster), Some(project)) =
                        (broadcaster.as_ref(), project.as_ref())
                    {
                        broadcaster
                            .send_events(&project.slug.to_string(), &[event])
                            .await;
                    }
                }
            }

            let run_span_for_future = run_span.clone();
            let mut run_span_for_body = run_span;

            let run_span_id = run_span_for_body
                .as_ref()
                .map(|span| span.context().span().span_context().span_id());
            let run_span_for_parent = run_span_for_future.clone();
            async move {
                // Enter the run span to keep it active during request handling
                // It will also be entered during body streaming in SpanInstrumentedBody
                let _run_span_guard = run_span_for_future.as_ref().map(|s| s.enter());
                
                // Create the span with the run span as its explicit parent
                // This ensures the run span doesn't end before this span is created
                let span: Span = if let Some(ref parent_span) = run_span_for_parent {
                    tracing::info_span!(
                        target: "vllora::user_tracing::cloud_api",
                        parent: parent_span.clone(),
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
                        HTTP_RESPONSE_STATUS_CODE = field::Empty,
                        status = field::Empty,
                        error = field::Empty,
                        ip = get_client_ip(req.request())
                    )
                } else {
                    tracing::info_span!(
                        target: "vllora::user_tracing::cloud_api",
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
                        HTTP_RESPONSE_STATUS_CODE = field::Empty,
                        status = field::Empty,
                        error = field::Empty,
                        ip = get_client_ip(req.request())
                    )
                };


                if let (Some(run_id), Some(thread_id)) = (run_id, thread_id) {
                    let event = Event::Custom {
                        run_context: EventRunContext {
                            run_id: Some(run_id.value()),
                            thread_id: Some(thread_id.value()),
                            span_id: Some(span.context().span().span_context().span_id()),
                            parent_span_id: run_span_id,
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

                // Proceed with the request if within limits
                match service.call(req).instrument(span.clone()).await {
                    Ok(ok_res) => {
                        span.record(HTTP_RESPONSE_STATUS_CODE, ok_res.status().as_u16() as i64);
                        span.record("status", ok_res.status().as_u16() as i64);
                        if let Some(error) = ok_res.response().error() {
                            span.record("error", error.to_string());
                        }

                        let span_for_body = span.clone();
                        let instrumented = ok_res.map_body(move |_, body| {
                            let run_span = run_span_for_body.take();
                            BoxBody::new(SpanInstrumentedBody::new(
                                body,
                                span_for_body.clone(),
                                run_span,
                            ))
                        });

                        Ok(instrumented)
                    }
                    Err(err) => {
                        span.record("error", err.to_string());
                        span.record(HTTP_RESPONSE_STATUS_CODE, 500);
                        Err(err)
                    }
                }
            }.await
        })
    }
}

pin_project! {
    struct SpanInstrumentedBody<B> {
        #[pin]
        inner: B,
        span: Span,
        run_span: Option<Span>,
        finished: bool,
    }
}

impl<B> SpanInstrumentedBody<B> {
    fn new(inner: B, span: Span, run_span: Option<Span>) -> Self {
        Self {
            inner,
            span,
            run_span,
            finished: false,
        }
    }
}

impl<B> MessageBody for SpanInstrumentedBody<B>
where
    B: MessageBody,
    B::Error: std::fmt::Display,
{
    type Error = B::Error;

    fn size(&self) -> BodySize {
        self.inner.size()
    }

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, Self::Error>>> {
        let this = self.project();
        let _entered = this.span.enter();
        // Keep the run span active during body streaming
        let _run_span_entered = this.run_span.as_ref().map(|s| s.enter());

        match this.inner.poll_next(cx) {
            Poll::Ready(None) => {
                if !*this.finished {
                    *this.finished = true;
                }
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(err))) => {
                this.span.record("error", err.to_string());
                if !*this.finished {
                    this.span.record("status", 400_i64);
                    *this.finished = true;
                }
                Poll::Ready(Some(Err(err)))
            }
            other => other,
        }
    }
}
