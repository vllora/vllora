use crate::knowledge_embeddings::embed_phrase;
use actix_multipart::Multipart;
use actix_web::{error, web, HttpResponse, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::knowledge_source::NewKnowledgeSource;
use vllora_core::metadata::models::knowledge_source_part::NewKnowledgeSourcePart;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::knowledge_source::KnowledgeSourceService;
use vllora_core::types::metadata::project::Project;

fn map_db_error(err: DatabaseError) -> actix_web::Error {
    match err {
        DatabaseError::QueryError(diesel::result::Error::NotFound) => {
            error::ErrorNotFound("Knowledge source not found")
        }
        other => error::ErrorInternalServerError(other),
    }
}

#[derive(Debug, Deserialize)]
pub struct SearchKnowledgePartsRequest {
    pub phrase: String,
    pub top_k: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct SearchKnowledgePartsResponse {
    pub matches: Vec<vllora_core::metadata::services::knowledge_source::KnowledgeSourcePartMatch>,
}

pub async fn create_knowledge_source(
    workflow_id: web::Path<String>,
    mut payload: Multipart,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let mut name: Option<String> = None;
    let mut reference_id: Option<String> = None;
    let mut description: Option<String> = None;
    let mut metadata: Option<serde_json::Value> = None;
    let mut file_name: Option<String> = None;
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut parts: Vec<NewKnowledgeSourcePart> = Vec::new();
    // OTel trace pipeline (Track A): when a source is backed by an already-
    // persisted `trace_bundles` row, the client sends `kind=otel-trace` +
    // `trace_bundle_id=<uuid>` instead of a file upload.
    let mut kind: Option<String> = None;
    let mut trace_bundle_id: Option<String> = None;

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
            "name" | "reference_id" | "description" | "metadata" | "parts" | "kind"
            | "trace_bundle_id" => {
                let mut bytes = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk = chunk.map_err(error::ErrorBadRequest)?;
                    bytes.extend_from_slice(&chunk);
                }
                let text = String::from_utf8(bytes).map_err(error::ErrorBadRequest)?;
                match field_name.as_str() {
                    "name" => name = Some(text),
                    "reference_id" => reference_id = Some(text),
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
                    "kind" => kind = Some(text),
                    "trace_bundle_id" => trace_bundle_id = Some(text),
                    _ => {}
                }
            }
            _ => {
                while let Some(chunk) = field.next().await {
                    let _ = chunk.map_err(error::ErrorBadRequest)?;
                }
            }
        }
    }

    let name = name
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| error::ErrorBadRequest("name is required"))?;
    let reference_id = reference_id
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let kind = kind.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let trace_bundle_id = trace_bundle_id
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let is_otel_trace = kind.as_deref() == Some("otel-trace") || trace_bundle_id.is_some();

    if is_otel_trace {
        let trace_bundle_id = trace_bundle_id.ok_or_else(|| {
            error::ErrorBadRequest("trace_bundle_id is required when kind=otel-trace")
        })?;
        // OTel-backed sources don't have a file upload; the blob lives in trace_bundles.
        let source_id = Uuid::new_v4().to_string();
        let ks = service
            .create_typed(NewKnowledgeSource {
                id: Some(source_id),
                reference_id,
                workflow_id,
                name,
                description,
                metadata,
                trace_bundle_id: Some(trace_bundle_id),
                part: parts,
            })
            .map_err(map_db_error)?;
        return Ok(HttpResponse::Created().json(serde_json::json!({
            "knowledge_source": ks,
        })));
    }

    let file_bytes = file_bytes
        .filter(|v| !v.is_empty())
        .ok_or_else(|| error::ErrorBadRequest("file is required"))?;
    let source_id = Uuid::new_v4().to_string();
    let safe_file_name = file_name.unwrap_or_else(|| "document.bin".to_string());
    let base_dir = std::env::var("KNOWLEDGE_STORAGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".knowledge_store"));
    let file_path = base_dir
        .join(&workflow_id)
        .join(&source_id)
        .join(safe_file_name);
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
            reference_id,
            workflow_id,
            name,
            description,
            metadata,
            trace_bundle_id: None,
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
    let (workflow_id, identifier) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let ks = service
        .get_typed_by_identifier_and_workflow_id(&workflow_id, &identifier)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(ks))
}

/// Serve the original uploaded file for a knowledge source.
/// Scans the storage directory for the first file matching the source ID.
pub async fn download_knowledge_source_file(
    path: web::Path<(String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, identifier) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    // Verify source exists and get its ID
    let ks = service
        .get_by_identifier_and_workflow_id(&workflow_id, &identifier)
        .map_err(map_db_error)?;

    let base_dir = std::env::var("KNOWLEDGE_STORAGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".knowledge_store"));
    let source_dir = base_dir.join(&workflow_id).join(&ks.id);

    // Find the first file in the source directory
    let mut entries = fs::read_dir(&source_dir)
        .await
        .map_err(|_| error::ErrorNotFound("Original file not found"))?;

    let file_entry = entries
        .next_entry()
        .await
        .map_err(error::ErrorInternalServerError)?
        .ok_or_else(|| error::ErrorNotFound("Original file not found"))?;

    let file_path = file_entry.path();
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document.bin")
        .to_string();

    let bytes = fs::read(&file_path)
        .await
        .map_err(|_| error::ErrorNotFound("Original file not found"))?;

    // Infer content type from extension
    let content_type = match file_path.extension().and_then(|e| e.to_str()) {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    };

    Ok(HttpResponse::Ok()
        .content_type(content_type)
        .append_header((
            "Content-Disposition",
            format!("inline; filename=\"{}\"", file_name),
        ))
        .body(bytes))
}

#[derive(Debug, Deserialize)]
pub struct ListKnowledgeSourcesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list_knowledge_sources(
    workflow_id: web::Path<String>,
    query: web::Query<ListKnowledgeSourcesQuery>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let query = query.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let sources = service
        .list_typed_paged(&workflow_id, query.limit, query.offset)
        .map_err(map_db_error)?;
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

pub async fn add_knowledge_source_parts(
    path: web::Path<(String, String)>,
    body: web::Json<Vec<NewKnowledgeSourcePart>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, source_identifier) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    service
        .get_by_identifier_and_workflow_id(&workflow_id, &source_identifier)
        .map_err(map_db_error)?;

    let parts = service
        .add_parts_by_identifier_and_workflow_id(
            &workflow_id,
            &source_identifier,
            body.into_inner(),
        )
        .map_err(map_db_error)?;

    Ok(HttpResponse::Created().json(serde_json::json!({ "parts": parts })))
}

pub async fn list_knowledge_source_parts(
    path: web::Path<(String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, source_identifier) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    service
        .get_by_identifier_and_workflow_id(&workflow_id, &source_identifier)
        .map_err(map_db_error)?;

    let parts = service
        .list_parts_by_identifier_and_workflow_id(&workflow_id, &source_identifier)
        .map_err(map_db_error)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "parts": parts })))
}

pub async fn delete_knowledge_source_part(
    path: web::Path<(String, String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, source_identifier, part_identifier) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    service
        .get_by_identifier_and_workflow_id(&workflow_id, &source_identifier)
        .map_err(map_db_error)?;

    service
        .delete_part(&workflow_id, &source_identifier, &part_identifier)
        .map_err(map_db_error)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "deleted": true,
        "part_identifier": part_identifier
    })))
}

pub async fn update_parts_metadata(
    path: web::Path<(String, String)>,
    body: web::Json<Vec<vllora_core::metadata::services::knowledge_source::PartMetadataUpdate>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, source_identifier) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    service
        .get_by_identifier_and_workflow_id(&workflow_id, &source_identifier)
        .map_err(map_db_error)?;

    let updated = service
        .update_parts_extraction_metadata(&workflow_id, &source_identifier, body.into_inner())
        .map_err(map_db_error)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "updated": updated })))
}

pub async fn upsert_knowledge_source(
    workflow_id: web::Path<String>,
    mut payload: Multipart,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    let mut name: Option<String> = None;
    let mut reference_id: Option<String> = None;
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
            "name" | "reference_id" | "description" | "metadata" | "parts" => {
                let mut bytes = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk = chunk.map_err(error::ErrorBadRequest)?;
                    bytes.extend_from_slice(&chunk);
                }
                let text = String::from_utf8(bytes).map_err(error::ErrorBadRequest)?;
                match field_name.as_str() {
                    "name" => name = Some(text),
                    "reference_id" => reference_id = Some(text),
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
            _ => {
                while let Some(chunk) = field.next().await {
                    let _ = chunk.map_err(error::ErrorBadRequest)?;
                }
            }
        }
    }

    let name = name
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| error::ErrorBadRequest("name is required"))?;
    let reference_id = reference_id
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let file_bytes = file_bytes
        .filter(|v| !v.is_empty())
        .ok_or_else(|| error::ErrorBadRequest("file is required"))?;

    // Check if a knowledge source with this name already exists in the workflow
    let existing = service
        .find_by_name_and_workflow_id(&workflow_id, &name)
        .map_err(map_db_error)?;

    let replaced_id = if let Some(existing_ks) = existing {
        let old_id = existing_ks.id.clone();
        service.soft_delete(&existing_ks.id).map_err(map_db_error)?;
        Some(old_id)
    } else {
        None
    };

    // Create the new knowledge source
    let source_id = Uuid::new_v4().to_string();
    let safe_file_name = file_name.unwrap_or_else(|| "document.bin".to_string());
    let base_dir = std::env::var("KNOWLEDGE_STORAGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".knowledge_store"));
    let file_path = base_dir
        .join(&workflow_id)
        .join(&source_id)
        .join(safe_file_name);
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
            reference_id,
            workflow_id,
            name,
            description,
            metadata,
            trace_bundle_id: None,
            part: parts,
        })
        .map_err(|e| {
            let _ = std::fs::remove_file(&file_path);
            map_db_error(e)
        })?;

    let mut status = if replaced_id.is_some() {
        HttpResponse::Ok()
    } else {
        HttpResponse::Created()
    };

    Ok(status.json(serde_json::json!({
        "knowledge_source": ks,
        "document_path": file_path.to_string_lossy().to_string(),
        "replaced": replaced_id.is_some(),
        "replaced_id": replaced_id
    })))
}

pub async fn soft_delete_knowledge_source(
    path: web::Path<(String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, identifier) = path.into_inner();
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());

    service
        .soft_delete_by_identifier_and_workflow_id(&workflow_id, &identifier)
        .map_err(map_db_error)?;
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

pub async fn search_knowledge_source_parts(
    workflow_id: web::Path<String>,
    body: web::Json<SearchKnowledgePartsRequest>,
    db_pool: web::Data<DbPool>,
    project: web::ReqData<Project>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let req = body.into_inner();
    let top_k = req.top_k.unwrap_or(5).min(100);
    let phrase = req.phrase.trim().to_string();

    if phrase.is_empty() {
        return Err(error::ErrorBadRequest("phrase is required"));
    }

    let query_embedding = embed_phrase(db_pool.get_ref().clone(), &phrase, &project.slug)
        .await
        .map_err(error::ErrorInternalServerError)?;
    let service = KnowledgeSourceService::new(db_pool.get_ref().clone());
    let mut matches = service
        .search_parts_by_similarity(&workflow_id, &query_embedding, top_k)
        .map_err(map_db_error)?;
    for m in &mut matches {
        m.part.embeddings = None;
    }

    Ok(HttpResponse::Ok().json(SearchKnowledgePartsResponse { matches }))
}
