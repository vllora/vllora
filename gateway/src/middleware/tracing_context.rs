use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    HttpMessage,
};
use futures::future::LocalBoxFuture;
use langdb_core::telemetry::AdditionalContext;
use langdb_core::telemetry::HeaderExtractor;
use langdb_core::types::metadata::project::Project;
use opentelemetry::{
    baggage::BaggageExt, propagation::TextMapPropagator, trace::FutureExt, Context, KeyValue,
};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use std::future::Ready;

pub struct TracingContext;
impl<S, B> Transform<S, ServiceRequest> for TracingContext
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Transform = TracingContextMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(TracingContextMiddleware { service }))
    }
}

pub struct TracingContextMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for TracingContextMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let propagator = TraceContextPropagator::new();
        let context =
            propagator.extract_with_context(&Context::new(), &HeaderExtractor(req.headers()));

        let mut project_slug = None;

        if let Some(project) = &req.extensions().get::<Project>().cloned() {
            project_slug = Some(project.slug.clone());
        }

        let mut key_values = vec![
            KeyValue::new("langdb.tenant", "default".to_string()),
            KeyValue::new("langdb.project_id", project_slug.unwrap_or_default()),
        ];

        let label = req
            .headers()
            .get("x-label")
            .and_then(|v| v.to_str().ok().map(|v| v.to_string()));

        if let Some(label) = label.as_ref() {
            key_values.push(KeyValue::new("langdb.label", label.clone()));
        }

        let additional_context = req.extensions().get::<AdditionalContext>().cloned();
        if let Some(additional_context) = additional_context.as_ref() {
            for (key, value) in additional_context.0.iter() {
                key_values.push(KeyValue::new(key.clone(), value.clone()));
            }
        }

        let context = context.with_baggage(key_values);

        let fut = self.service.call(req).with_context(context);
        Box::pin(fut)
    }
}
