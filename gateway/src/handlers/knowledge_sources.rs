use actix_web::{error, web, HttpResponse, Result};
use serde::Deserialize;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::knowledge_source::KnowledgeSourceService;

fn map_db_error(err: DatabaseError) -> actix_web::Error {
    match err {
        DatabaseError::QueryError(diesel::result::Error::NotFound) => {
            error::ErrorNotFound("Knowledge source not found")
        }
        other => error::ErrorInternalServerError(other),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateKnowledgeSourceRequest {
    pub name: String,
    #[serde(rename = "type")]
    pub source_type: String,
    pub content: Option<String>,
    pub extracted_content: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChunksRequest {
    pub extracted_content: serde_json::Value,
}

pub async fn create_knowledge_source(
    workflow_id: web::Path<String>,
    body: web::Json<CreateKnowledgeSourceRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let extracted_str = payload.extracted_content.map(|v| v.to_string());

    let ks = service
        .create(
            &workflow_id,
            &payload.name,
            &payload.source_type,
            payload.content.as_deref(),
            extracted_str.as_deref(),
        )
        .map_err(map_db_error)?;
    Ok(HttpResponse::Created().json(ks))
}

pub async fn get_knowledge_source(
    path: web::Path<(String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (_workflow_id, ks_id) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let ks = service.get(&ks_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(ks))
}

pub async fn list_knowledge_sources(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let sources = service.list(&workflow_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "knowledge_sources": sources })))
}

pub async fn count_knowledge_sources(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let count = service.count(&workflow_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "count": count })))
}

pub async fn update_knowledge_source_status(
    path: web::Path<(String, String)>,
    body: web::Json<UpdateStatusRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (_workflow_id, ks_id) = path.into_inner();
    let payload = body.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    service
        .update_status(&ks_id, &payload.status)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "updated": true })))
}

pub async fn update_knowledge_source_chunks(
    path: web::Path<(String, String)>,
    body: web::Json<UpdateChunksRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (_workflow_id, ks_id) = path.into_inner();
    let payload = body.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let content_str = payload.extracted_content.to_string();
    service
        .update_extracted_content(&ks_id, &content_str)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "updated": true })))
}

pub async fn soft_delete_knowledge_source(
    path: web::Path<(String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (_workflow_id, ks_id) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    service.soft_delete(&ks_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "deleted": true })))
}

pub async fn soft_delete_all_knowledge_sources(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let count = service
        .soft_delete_all(&workflow_id)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "deleted": count })))
}
