use actix_web::body::EitherBody;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{web, Error, HttpMessage, HttpResponse};
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::rc::Rc;
use tracing::error;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::project::ProjectServiceImpl;

pub struct LucyProjectMiddleware;

impl<S, B> Transform<S, ServiceRequest> for LucyProjectMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = LucyProjectMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(LucyProjectMiddlewareService {
            service: service.into(),
        }))
    }
}

pub struct LucyProjectMiddlewareService<S> {
    service: Rc<S>,
}

type LocalBoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;

impl<S, B> Service<ServiceRequest> for LucyProjectMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let srv = self.service.clone();

        Box::pin(async move {
            let database_pool: Option<&web::Data<DbPool>> = req.app_data();

            let Some(database_pool) = database_pool else {
                error!("Database pool is not found");
                return Ok(req.into_response(
                    HttpResponse::InternalServerError()
                        .finish()
                        .map_into_right_body(),
                ));
            };

            let project_service = ProjectServiceImpl::new(database_pool.get_ref().clone());

            if let Some(project) = project_service.get_by_slug("lucy")? {
                tracing::debug!("Lucy project resolved: {}", project.name);
                // Store full DbProject for handlers
                req.extensions_mut().insert(project.clone());
                // Store lightweight GatewayTenant for telemetry (core crate)
                req.extensions_mut()
                    .insert(vllora_core::types::GatewayTenant {
                        name: "lucy".to_string(),
                        project_slug: "lucy".to_string(),
                    });
            }

            let fut = srv.call(req);
            Ok(fut.await?.map_into_left_body())
        })
    }
}
