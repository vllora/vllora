use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::HttpMessage;
use langdb_core::telemetry::AdditionalContext;
use langdb_core::types::threads::CompletionsThreadId;
use std::collections::HashMap;
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use uuid::Uuid;

pub struct ThreadId;

impl Default for ThreadId {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadId {
    pub fn new() -> Self {
        ThreadId
    }
}

impl<S, B> Transform<S, ServiceRequest> for ThreadId
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type InitError = ();
    type Transform = ThreadIdMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ThreadIdMiddleware { service }))
    }
}

pub struct ThreadIdMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for ThreadIdMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        if req.path().contains("chat/completion") || req.path().contains("responses") {
            let thread_id = match req.headers().get("X-Thread-Id") {
                Some(value) => match value.to_str() {
                    Ok(v) => match uuid::Uuid::parse_str(v) {
                        Ok(v) => v.to_string(),
                        Err(_) => Uuid::new_v4().to_string(),
                    },
                    Err(_) => Uuid::new_v4().to_string(),
                },
                None => Uuid::new_v4().to_string(),
            };

            req.extensions_mut()
                .insert(CompletionsThreadId::new(thread_id.clone()));
            let mut extensions_mut = req.extensions_mut();
            let additional_context = extensions_mut.get_mut::<AdditionalContext>();
            if let Some(additional_context) = additional_context {
                additional_context
                    .0
                    .insert("langdb.thread_id".to_string(), thread_id);
            } else {
                let mut additional_context = HashMap::new();
                additional_context.insert("langdb.thread_id".to_string(), thread_id);
                req.extensions_mut()
                    .insert(AdditionalContext::new(additional_context));
            }
        }

        let fut = self.service.call(req);
        Box::pin(async move {
            let res = fut.await?;
            Ok(res)
        })
    }
}
