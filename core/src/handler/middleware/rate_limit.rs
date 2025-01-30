use crate::usage::{increment_and_get_value, LimitPeriod};
use actix_web::dev::forward_ready;
use actix_web::web::Data;
use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::rc::Rc;
use tokio::sync::Mutex;

pub const API_CALLS: &str = "api_calls";

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RateLimiting {
    pub hourly: Option<u64>,
    pub daily: Option<u64>,
    pub monthly: Option<u64>,
}
pub struct RateLimitMiddleware;

impl<S, B> Transform<S, ServiceRequest> for RateLimitMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = RateLimitMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RateLimitMiddlewareService {
            service: service.into(),
        }))
    }
}

pub struct RateLimitMiddlewareService<S> {
    service: Rc<S>,
}

type LocalBoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;

impl<S, B> Service<ServiceRequest> for RateLimitMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        let service = Rc::clone(&self.service);

        Box::pin(async move {
            let rate_limit_config = req.app_data::<Option<RateLimiting>>().cloned();
            if let Some(Some(rate_limit)) = rate_limit_config {
                let client = req
                    .extract::<Data<Mutex<ConnectionManager>>>()
                    .await
                    .unwrap()
                    .clone()
                    .into_inner();
                let mut client = async { client.lock().await.clone() }.await;

                if let Some(hourly) = rate_limit.hourly {
                    check_limit(&mut client, LimitPeriod::Hour, hourly).await?;
                }
                if let Some(daily) = rate_limit.daily {
                    check_limit(&mut client, LimitPeriod::Day, daily).await?;
                }
                if let Some(monthly) = rate_limit.monthly {
                    check_limit(&mut client, LimitPeriod::Month, monthly).await?;
                }
            }

            service.call(req).await
        })
    }
}

async fn check_limit(
    client: &mut ConnectionManager,
    period: LimitPeriod,
    limit: u64,
) -> Result<(), Error> {
    match increment_and_get_value(client, period, "default", API_CALLS, 1).await {
        Ok(current_calls) if current_calls > limit => Err(actix_web::error::ErrorTooManyRequests(
            "API call limit exceeded",
        )),
        Ok(_) => Ok(()),
        Err(e) => Err(actix_web::error::ErrorInternalServerError(e.to_string())),
    }
}
