use actix_web::{web, HttpRequest, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vllora_core::metadata::models::project::NewProjectDTO;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::project::ProjectServiceImpl;
use vllora_core::types::metadata::project::Project;
use vllora_core::types::metadata::services::project::ProjectService;

use vllora_core::ok_json;

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
    pub settings: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub settings: Option<serde_json::Value>,
    pub is_default: Option<bool>,
}

#[derive(Serialize)]
pub struct GetProjectResponse {
    pub project: Project,
}

pub async fn list_projects(_req: HttpRequest, db_pool: web::Data<DbPool>) -> Result<HttpResponse> {
    let project_service = ProjectServiceImpl::new(db_pool.get_ref().clone());

    // Use a dummy owner_id for now (you might want to get this from auth context)
    let owner_id = Uuid::nil();

    ok_json!(project_service.list(owner_id))
}

pub async fn create_project(
    req: web::Json<CreateProjectRequest>,
    db_pool: web::Data<DbPool>,
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

    Ok(project_service
        .create(new_project, owner_id)
        .map(|project| HttpResponse::Created().json(project))?)
}

pub async fn get_project(
    project_id: web::Path<Uuid>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let project_id = project_id.into_inner();

    let project_service = ProjectServiceImpl::new(db_pool.get_ref().clone());

    // Use a dummy owner_id for now (you might want to get this from auth context)
    let owner_id = Uuid::nil();

    ok_json!(project_service
        .get_by_id(project_id, owner_id)
        .map(|project| GetProjectResponse { project }))
}

pub async fn delete_project(
    project_id: web::Path<Uuid>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let project_id = project_id.into_inner();

    let project_service = ProjectServiceImpl::new(db_pool.get_ref().clone());

    // Use a dummy owner_id for now (you might want to get this from auth context)
    let owner_id = Uuid::nil();

    Ok(project_service
        .delete(project_id, owner_id)
        .map(|_result| {
            HttpResponse::Ok().json(serde_json::json!({
                "message": "Project deleted successfully"
            }))
        })?)
}

pub async fn update_project(
    project_id: web::Path<Uuid>,
    req: web::Json<UpdateProjectRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let project_id = project_id.into_inner();
    let project_service = ProjectServiceImpl::new(db_pool.get_ref().clone());

    // Use a dummy owner_id for now (you might want to get this from auth context)
    let owner_id = Uuid::nil();

    // Convert UpdateProjectRequest to UpdateProjectDTO
    let update_data = vllora_core::metadata::models::project::UpdateProjectDTO {
        name: req.name.clone(),
        description: req.description.clone(),
        settings: req.settings.clone(),
        is_default: req.is_default,
    };

    ok_json!(project_service.update(project_id, owner_id, update_data))
}

pub async fn set_default_project(
    project_id: web::Path<Uuid>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let project_id = project_id.into_inner();

    let project_service = ProjectServiceImpl::new(db_pool.get_ref().clone());

    // Use a dummy owner_id for now (you might want to get this from auth context)
    let owner_id = Uuid::nil();

    ok_json!(project_service.set_default(project_id, owner_id))
}
