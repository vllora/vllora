use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};

pub struct TraceLogger;

impl<S, B> Transform<S, ServiceRequest> for TraceLogger
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = TraceLoggerMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(TraceLoggerMiddleware { service }))
    }
}

pub struct TraceLoggerMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for TraceLoggerMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let start_time = std::time::Instant::now();
        let method = req.method().to_string();
        let uri = req.uri().to_string();

        let fut = self.service.call(req);

        Box::pin(async move {
            let res = fut.await?;
            let elapsed = start_time.elapsed();
            let status = res.status().as_u16();

            if status >= 400 {
                tracing::error!(
                    "{} {} {}ms",
                    format!("{} {} HTTP/1.1", method, uri),
                    status,
                    elapsed.as_millis(),
                );
            } else {
                tracing::info!(
                    "{} {} {}ms",
                    format!("{} {} HTTP/1.1", method, uri),
                    status,
                    elapsed.as_millis(),
                );
            }

            Ok(res)
        })
    }
}
