use actix_web::{web, HttpRequest, HttpResponse, Result};
use langdb_core::types::metadata::project::Project;
use langdb_metadata::services::project::{ProjectService, ProjectServiceImpl};
use langdb_metadata::models::project::NewProjectDTO;
use langdb_metadata::pool::DbPool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
    pub settings: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct CreateProjectResponse {
    pub project: Project,
}

#[derive(Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub settings: Option<serde_json::Value>,
    pub is_default: Option<bool>,
}

#[derive(Serialize)]
pub struct UpdateProjectResponse {
    pub project: Project,
}

#[derive(Serialize)]
pub struct ListProjectsResponse {
    pub projects: Vec<Project>,
}

#[derive(Serialize)]
pub struct GetProjectResponse {
    pub project: Project,
}

pub async fn list_projects(
    _req: HttpRequest,
    db_pool: web::Data<Arc<DbPool>>,
) -> Result<HttpResponse> {
    let project_service = ProjectServiceImpl::new(db_pool.get_ref().clone());
    
    // Use a dummy owner_id for now (you might want to get this from auth context)
    let owner_id = Uuid::nil();
    
    match project_service.list(owner_id) {
        Ok(projects) => {
            let response = ListProjectsResponse { projects };
            Ok(HttpResponse::Ok().json(response))
        }
        Err(e) => {
            tracing::error!("Failed to list projects: {:?}", e);
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({
                    "error": "Failed to list projects",
                    "message": e.to_string()
                })))
        }
    }
}

pub async fn create_project(
    req: web::Json<CreateProjectRequest>,
    db_pool: web::Data<Arc<DbPool>>,
) -> Result<HttpResponse> {
    let project_service = ProjectServiceImpl::new(db_pool.get_ref().clone());
    
    // Use a dummy owner_id for now (you might want to get this from auth context)
    let owner_id = Uuid::nil();
    
    let new_project = NewProjectDTO {
        name: req.name.clone(),
        description: req.description.clone(),
        settings: req.settings.clone(),
        private_model_prices: None,
        usage_limit: None,
    };
    
    match project_service.create(new_project, owner_id) {
        Ok(project) => {
            let response = CreateProjectResponse { project };
            Ok(HttpResponse::Created().json(response))
        }
        Err(e) => {
            tracing::error!("Failed to create project: {:?}", e);
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({
                    "error": "Failed to create project",
                    "message": e.to_string()
                })))
        }
    }
}

pub async fn get_project(
    path: web::Path<String>,
    db_pool: web::Data<Arc<DbPool>>,
) -> Result<HttpResponse> {
    let project_id = match path.parse::<Uuid>() {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::BadRequest()
                .json(serde_json::json!({
                    "error": "Invalid project ID",
                    "message": "Project ID must be a valid UUID"
                })));
        }
    };
    
    let project_service = ProjectServiceImpl::new(db_pool.get_ref().clone());
    
    // Use a dummy owner_id for now (you might want to get this from auth context)
    let owner_id = Uuid::nil();
    
    match project_service.get_by_id(project_id, owner_id) {
        Ok(project) => {
            let response = GetProjectResponse { project };
            Ok(HttpResponse::Ok().json(response))
        }
        Err(e) => {
            tracing::error!("Failed to get project {}: {:?}", project_id, e);
            Ok(HttpResponse::NotFound()
                .json(serde_json::json!({
                    "error": "Project not found",
                    "message": format!("Project with ID {} not found", project_id)
                })))
        }
    }
}

pub async fn delete_project(
    path: web::Path<String>,
    db_pool: web::Data<Arc<DbPool>>,
) -> Result<HttpResponse> {
    let project_id = match path.parse::<Uuid>() {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::BadRequest()
                .json(serde_json::json!({
                    "error": "Invalid project ID",
                    "message": "Project ID must be a valid UUID"
                })));
        }
    };
    
    let project_service = ProjectServiceImpl::new(db_pool.get_ref().clone());
    
    // Use a dummy owner_id for now (you might want to get this from auth context)
    let owner_id = Uuid::nil();
    
    match project_service.delete(project_id, owner_id) {
        Ok(_) => {
            tracing::info!("Successfully deleted project: {}", project_id);
            Ok(HttpResponse::Ok()
                .json(serde_json::json!({
                    "message": "Project deleted successfully"
                })))
        }
        Err(e) => {
            tracing::error!("Failed to delete project {}: {:?}", project_id, e);
            Ok(HttpResponse::NotFound()
                .json(serde_json::json!({
                    "error": "Project not found",
                    "message": format!("Project with ID {} not found or already deleted", project_id)
                })))
        }
    }
}

pub async fn update_project(
    path: web::Path<String>,
    _req: web::Json<UpdateProjectRequest>,
    _db_pool: web::Data<Arc<DbPool>>,
) -> Result<HttpResponse> {
    let project_id = match path.parse::<Uuid>() {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::BadRequest()
                .json(serde_json::json!({
                    "error": "Invalid project ID",
                    "message": "Project ID must be a valid UUID"
                })));
        }
    };
    
    // For now, we don't have an update method in ProjectService
    // This would need to be implemented in the service layer
    tracing::warn!("Update project endpoint called but not implemented yet for project: {}", project_id);
    
    Ok(HttpResponse::NotImplemented()
        .json(serde_json::json!({
            "error": "Not implemented",
            "message": "Update project functionality is not yet implemented"
        })))
}

