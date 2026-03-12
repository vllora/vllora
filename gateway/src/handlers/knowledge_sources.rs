use actix_multipart::Multipart;
use actix_web::{error, web, HttpResponse, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::knowledge_source::NewKnowledgeSource;
use vllora_core::metadata::models::knowledge_source_part::NewKnowledgeSourcePart;
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
pub struct UpdateStatusRequest {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChunksRequest {
    pub extracted_content: serde_json::Value,
}

pub async fn create_knowledge_source(
    workflow_id: web::Path<String>,
    mut payload: Multipart,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut metadata: Option<serde_json::Value> = None;
    let mut file_name: Option<String> = None;
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut parts: Vec<NewKnowledgeSourcePart> = Vec::new();

    while let Some(field) = payload.next().await {
        let mut field = field.map_err(error::ErrorBadRequest)?;
        let field_name = field.name().to_string();
        match field_name.as_str() {
            "file" => {
                if let Some(filename) = field.content_disposition().get_filename() {
                    file_name = Some(filename.to_string());
                }
                let mut bytes = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk = chunk.map_err(error::ErrorBadRequest)?;
                    bytes.extend_from_slice(&chunk);
                }
                file_bytes = Some(bytes);
            }
            "name" | "description" | "metadata" | "parts" => {
                let mut bytes = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk = chunk.map_err(error::ErrorBadRequest)?;
                    bytes.extend_from_slice(&chunk);
                }
                let text = String::from_utf8(bytes).map_err(error::ErrorBadRequest)?;
                match field_name.as_str() {
                    "name" => name = Some(text),
                    "description" => description = Some(text),
                    "metadata" => {
                        let parsed = serde_json::from_str::<serde_json::Value>(&text)
                            .map_err(error::ErrorBadRequest)?;
                        metadata = Some(parsed);
                    }
                    "parts" => {
                        parts = serde_json::from_str::<Vec<NewKnowledgeSourcePart>>(&text)
                            .map_err(error::ErrorBadRequest)?;
                    }
                    _ => {}
                }
            }
            _ => while let Some(chunk) = field.next().await {
                let _ = chunk.map_err(error::ErrorBadRequest)?;
            },
        }
    }

    let name = name
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| error::ErrorBadRequest("name is required"))?;
    let file_bytes = file_bytes
        .filter(|v| !v.is_empty())
        .ok_or_else(|| error::ErrorBadRequest("file is required"))?;
    let source_id = Uuid::new_v4().to_string();
    let safe_file_name = file_name.unwrap_or_else(|| "document.bin".to_string());
    let base_dir = std::env::var("KNOWLEDGE_STORAGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".knowledge_store"));
    let file_path = base_dir.join(&workflow_id).join(&source_id).join(safe_file_name);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(error::ErrorInternalServerError)?;
    }
    fs::write(&file_path, file_bytes)
        .await
        .map_err(error::ErrorInternalServerError)?;

    let ks = service
        .create_typed(NewKnowledgeSource {
            id: Some(source_id),
            workflow_id,
            name,
            description,
            metadata,
            part: parts,
        })
        .map_err(|e| {
            let _ = std::fs::remove_file(&file_path);
            map_db_error(e)
        })?;
    Ok(HttpResponse::Created().json(serde_json::json!({
        "knowledge_source": ks,
        "document_path": file_path.to_string_lossy().to_string()
    })))
}

pub async fn get_knowledge_source(
    path: web::Path<(String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (_workflow_id, ks_id) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let ks = service.get_typed(&ks_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(ks))
}

pub async fn list_knowledge_sources(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let sources = service.list_typed(&workflow_id).map_err(map_db_error)?;
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
    _body: web::Json<UpdateStatusRequest>,
    _db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (_workflow_id, ks_id) = path.into_inner();
    Ok(HttpResponse::NotImplemented().json(serde_json::json!({
        "message": "Legacy status updates removed from knowledge_sources",
        "knowledge_source_id": ks_id
    })))
}

pub async fn update_knowledge_source_chunks(
    path: web::Path<(String, String)>,
    _body: web::Json<UpdateChunksRequest>,
    _db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (_workflow_id, ks_id) = path.into_inner();
    Ok(HttpResponse::NotImplemented().json(serde_json::json!({
        "message": "Legacy extracted_content updates removed from knowledge_sources; use /knowledge/{ks_id}/parts APIs",
        "knowledge_source_id": ks_id
    })))
}

pub async fn add_knowledge_source_parts(
    path: web::Path<(String, String)>,
    body: web::Json<Vec<NewKnowledgeSourcePart>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, ks_id) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let source = service.get(&ks_id).map_err(map_db_error)?;
    if source.workflow_id != workflow_id {
        return Err(error::ErrorNotFound("Knowledge source not found"));
    }

    let parts = service
        .add_parts(&ks_id, body.into_inner())
        .map_err(map_db_error)?;

    Ok(HttpResponse::Created().json(serde_json::json!({ "parts": parts })))
}

pub async fn list_knowledge_source_parts(
    path: web::Path<(String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, ks_id) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let source = service.get(&ks_id).map_err(map_db_error)?;
    if source.workflow_id != workflow_id {
        return Err(error::ErrorNotFound("Knowledge source not found"));
    }

    let parts = service.list_parts(&ks_id).map_err(map_db_error)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "parts": parts })))
}

pub async fn delete_knowledge_source_part(
    path: web::Path<(String, String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, ks_id, part_id) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let source = service.get(&ks_id).map_err(map_db_error)?;
    if source.workflow_id != workflow_id {
        return Err(error::ErrorNotFound("Knowledge source not found"));
    }

    service
        .delete_part(&ks_id, &part_id)
        .map_err(map_db_error)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "deleted": true,
        "part_id": part_id
    })))
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
