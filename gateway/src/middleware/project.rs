use actix_web::body::EitherBody;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{web, Error, HttpMessage, HttpResponse};
use langdb_metadata::pool::DbPool;
use langdb_metadata::services::project::{ProjectService, ProjectServiceImpl};
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use tracing::error;
use uuid::Uuid;

pub const PROJECT_HEADER: &str = "X-Project-Id";

pub struct ProjectMiddleware;

impl ProjectMiddleware {
    pub fn new() -> Self {
        Self
    }
}

impl<S, B> Transform<S, ServiceRequest> for ProjectMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = ProjectMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ProjectMiddlewareService {
            service: service.into(),
        }))
    }
}

pub struct ProjectMiddlewareService<S> {
    service: Rc<S>,
}

type LocalBoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;

impl<S, B> Service<ServiceRequest> for ProjectMiddlewareService<S>
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
        let headers = req.headers().clone();
        let project_id_in_header = headers
            .get(PROJECT_HEADER)
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        let srv = self.service.clone();

        Box::pin(async move {
            let database_pool: Option<&web::Data<Arc<DbPool>>> = req.app_data();

            let Some(database_pool) = database_pool else {
                error!("Database pool is not found");
                return Ok(req.into_response(
                    HttpResponse::InternalServerError()
                        .finish()
                        .map_into_right_body(),
                ));
            };

            let project_service = ProjectServiceImpl::new(database_pool.get_ref().clone());

            let project = match &project_id_in_header {
                Some(project_id) => {
                    let Ok(project_uuid) = project_id.parse::<Uuid>() else {
                        error!("Project ID '{project_id}' is not a valid UUID");
                        return Ok(req.into_response(
                            HttpResponse::BadRequest()
                                .json(serde_json::json!({
                                    "error": "Invalid project ID",
                                    "message": "The provided project ID is not a valid UUID"
                                }))
                                .map_into_right_body(),
                        ));
                    };

                    project_service
                        .get_by_id(project_uuid, Uuid::nil())
                        .map(Some)
                }
                None => {
                    // No project header, try to get the default project
                    match project_service.list(Uuid::nil()) {
                        Ok(mut projects) => {
                            // Find the first default project, or fall back to the first project
                            if let Some(pos) = projects.iter().position(|p| p.is_default) {
                                Ok(Some(projects.remove(pos)))
                            } else if let Some(first_project) = projects.into_iter().next() {
                                Ok(Some(first_project))
                            } else {
                                error!("No projects found in database");
                                Ok(None)
                            }
                        }
                        Err(e) => Err(e),
                    }
                }
            };

            match project {
                Ok(Some(p)) => {
                    tracing::debug!("Project resolved: {}", p.name);
                    // Store full DbProject for handlers
                    req.extensions_mut().insert(p.clone());
                    // Store lightweight GatewayProject for telemetry (core crate)
                    req.extensions_mut().insert(langdb_core::types::GatewayProject {
                        id: p.id.to_string(),
                    });
                }
                Ok(None) => {
                    error!("No project found");
                    return Ok(req.into_response(
                        HttpResponse::BadRequest()
                            .json(serde_json::json!({
                                "error": "Project not found",
                                "message": "No project found in database"
                            }))
                            .map_into_right_body(),
                    ));
                }
                Err(e) => {
                    if let Some(project_id) = project_id_in_header {
                        error!("Error fetching project '{project_id}': {:?}", e);
                    } else {
                        error!("Error fetching default project: {:?}", e);
                    }

                    return Ok(req.into_response(
                        HttpResponse::BadRequest()
                            .json(serde_json::json!({
                                "error": "Project not found",
                                "message": "The specified project ID does not exist or you don't have access to it"
                            }))
                            .map_into_right_body(),
                    ));
                }
            }

            let fut = srv.call(req);
            Ok(fut.await?.map_into_left_body())
        })
    }
}
