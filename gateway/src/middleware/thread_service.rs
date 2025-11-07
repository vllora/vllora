use crate::threads::ThreadImpl;
use actix_web::body::EitherBody;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{web, Error, HttpMessage};
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::rc::Rc;
use vllora_core::metadata::pool::DbPool;
use vllora_core::types::metadata::project::Project;
use vllora_core::types::threads::ThreadServiceWrapper;

pub struct ThreadsServiceMiddleware;

impl ThreadsServiceMiddleware {
    pub fn new() -> Self {
        Self
    }
}

impl<S, B> Transform<S, ServiceRequest> for ThreadsServiceMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = ThreadsMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ThreadsMiddlewareService {
            service: service.into(),
        }))
    }
}

pub struct ThreadsMiddlewareService<S> {
    service: Rc<S>,
}

type LocalBoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;

impl<S, B> Service<ServiceRequest> for ThreadsMiddlewareService<S>
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
            let project: Option<Project> = req.extensions().get::<Project>().cloned();

            tracing::info!("Database pool: {:?}, project: {:?}", database_pool, project);
            if let (Some(database_pool), Some(project)) = (database_pool, project) {
                let thread_impl = ThreadImpl::new(database_pool.get_ref().clone(), project.clone());
                req.extensions_mut()
                    .insert(Rc::new(thread_impl) as Rc<dyn ThreadServiceWrapper>);
                tracing::info!("Thread service wrapper inserted into extensions");
            } else {
                tracing::error!("Database pool or project is not found");
            }

            let fut = srv.call(req);
            Ok(fut.await?.map_into_left_body())
        })
    }
}
