use actix_multipart::Multipart;
use actix_web::{error, web, HttpResponse, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::knowledge::DbNewKnowledge;
use vllora_core::metadata::models::workflow::{DbNewWorkflow, DbUpdateWorkflow};
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::knowledge::KnowledgeService;
use vllora_core::metadata::services::workflow::WorkflowService;

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub objective: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowRequest {
    pub name: Option<String>,
    pub objective: Option<String>,
}

fn map_db_error(err: DatabaseError) -> actix_web::Error {
    match err {
        DatabaseError::QueryError(diesel::result::Error::NotFound) => {
            error::ErrorNotFound("Workflow not found")
        }
        other => error::ErrorInternalServerError(other),
    }
}

pub async fn list_workflows(db_pool: web::Data<DbPool>) -> Result<HttpResponse> {
    let service = WorkflowService::new(db_pool.get_ref().clone());
    let workflows = service.list().map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(workflows))
}

pub async fn create_workflow(
    body: web::Json<CreateWorkflowRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let payload = body.into_inner();
    let service = WorkflowService::new(db_pool.get_ref().clone());

    let workflow = service
        .create(DbNewWorkflow::new(payload.name, payload.objective))
        .map_err(map_db_error)?;

    Ok(HttpResponse::Created().json(workflow))
}

pub async fn update_workflow(
    workflow_id: web::Path<String>,
    body: web::Json<UpdateWorkflowRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowService::new(db_pool.get_ref().clone());
    let update = DbUpdateWorkflow::new()
        .with_name(payload.name)
        .with_objective(payload.objective);

    let workflow = service.update(&workflow_id, update).map_err(map_db_error)?;

    Ok(HttpResponse::Ok().json(workflow))
}

pub async fn soft_delete_workflow(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowService::new(db_pool.get_ref().clone());

    service.soft_delete(&workflow_id).map_err(map_db_error)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "id": workflow_id,
        "deleted": true
    })))
}

fn placeholder(endpoint: &str, method: &str, workspace_id: &str) -> HttpResponse {
    HttpResponse::NotImplemented().json(serde_json::json!({
        "message": "Placeholder endpoint not implemented yet",
        "endpoint": endpoint,
        "method": method,
        "workspace_id": workspace_id
    }))
}

#[derive(Debug, Default)]
struct CreateKnowledgePayload {
    name: Option<String>,
    metadata: Option<String>,
    description: Option<String>,
    file_name: Option<String>,
    file_bytes: Option<Vec<u8>>,
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    if cleaned.is_empty() {
        "document.bin".to_string()
    } else {
        cleaned
    }
}

fn knowledge_storage_base_dir() -> PathBuf {
    std::env::var("KNOWLEDGE_STORAGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".knowledge_store"))
}

async fn parse_create_knowledge_payload(
    mut payload: Multipart,
) -> Result<CreateKnowledgePayload, actix_web::Error> {
    let mut out = CreateKnowledgePayload::default();

    while let Some(field) = payload.next().await {
        let mut field = field.map_err(error::ErrorBadRequest)?;
        let field_name = field.name().to_string();

        match field_name.as_str() {
            "file" => {
                let cd = field.content_disposition();
                if let Some(filename) = cd.get_filename() {
                    out.file_name = Some(sanitize_filename(filename));
                }
                let mut bytes = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk = chunk.map_err(error::ErrorBadRequest)?;
                    bytes.extend_from_slice(&chunk);
                }
                out.file_bytes = Some(bytes);
            }
            "name" | "metadata" | "description" => {
                let mut bytes = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk = chunk.map_err(error::ErrorBadRequest)?;
                    bytes.extend_from_slice(&chunk);
                }
                let value = String::from_utf8(bytes).map_err(error::ErrorBadRequest)?;
                match field_name.as_str() {
                    "name" => out.name = Some(value),
                    "metadata" => out.metadata = Some(value),
                    "description" => out.description = Some(value),
                    _ => {}
                }
            }
            _ => while let Some(chunk) = field.next().await {
                let _ = chunk.map_err(error::ErrorBadRequest)?;
            },
        }
    }

    Ok(out)
}

pub async fn create_workflow_knowledge(
    workspace_id: web::Path<String>,
    payload: Multipart,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workspace_id.into_inner();
    let parsed = parse_create_knowledge_payload(payload).await?;

    let name = parsed
        .name
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| error::ErrorBadRequest("name is required"))?;
    let file_bytes = parsed
        .file_bytes
        .filter(|v| !v.is_empty())
        .ok_or_else(|| error::ErrorBadRequest("file is required"))?;
    let file_name = parsed.file_name.unwrap_or_else(|| "document.bin".to_string());

    let knowledge_input = DbNewKnowledge::new(
        workflow_id.clone(),
        name,
        parsed.metadata,
        parsed.description,
    );
    let knowledge_id = knowledge_input
        .id
        .clone()
        .ok_or_else(|| error::ErrorInternalServerError("knowledge id generation failed"))?;

    let file_path = knowledge_storage_base_dir()
        .join(&workflow_id)
        .join(&knowledge_id)
        .join(file_name);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(error::ErrorInternalServerError)?;
    }
    fs::write(&file_path, file_bytes)
        .await
        .map_err(error::ErrorInternalServerError)?;

    let service = KnowledgeService::new(db_pool.get_ref().clone());
    let knowledge = match service.create(knowledge_input) {
        Ok(k) => k,
        Err(e) => {
            let _ = fs::remove_file(&file_path).await;
            return Err(map_db_error(e));
        }
    };

    let relative_path = file_path
        .strip_prefix(Path::new("."))
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string();

    Ok(HttpResponse::Created().json(serde_json::json!({
        "knowledge": knowledge,
        "document_path": relative_path
    })))
}

pub async fn list_workflow_knowledges(
    workspace_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workspace_id.into_inner();
    let service = KnowledgeService::new(db_pool.get_ref().clone());
    let records = service
        .list_by_workflow_id(&workflow_id)
        .map_err(map_db_error)?;

    let items: Vec<serde_json::Value> = records
        .into_iter()
        .map(|knowledge| {
            let document_dir = knowledge_storage_base_dir()
                .join(&workflow_id)
                .join(&knowledge.id)
                .to_string_lossy()
                .to_string();
            serde_json::json!({
                "knowledge": knowledge,
                "document_dir": document_dir
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "workflow_id": workflow_id,
        "items": items
    })))
}

pub async fn chunk_workflow_knowledge(workspace_id: web::Path<String>) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/knowledge/chunk",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn delete_workflow_knowledge_chunk(
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (workspace_id, chunk_id) = path.into_inner();
    Ok(HttpResponse::NotImplemented().json(serde_json::json!({
        "message": "Placeholder endpoint not implemented yet",
        "endpoint": "/finetune/workflows/{workspace_id}/knowledge/chunk/{chunk_id}",
        "method": "DELETE",
        "workspace_id": workspace_id,
        "chunk_id": chunk_id
    })))
}

pub async fn create_workflow_knowledge_trace(
    workspace_id: web::Path<String>,
) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/knowledge/trace",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn delete_workflow_knowledge_trace(
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (workspace_id, trace_id) = path.into_inner();
    Ok(HttpResponse::NotImplemented().json(serde_json::json!({
        "message": "Placeholder endpoint not implemented yet",
        "endpoint": "/finetune/workflows/{workspace_id}/knowledge/trace/{trace_id}",
        "method": "DELETE",
        "workspace_id": workspace_id,
        "trace_id": trace_id
    })))
}

pub async fn create_workflow_topics(workspace_id: web::Path<String>) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/topics",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn delete_workflow_topics(workspace_id: web::Path<String>) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/topics",
        "DELETE",
        &workspace_id.into_inner(),
    ))
}

pub async fn generate_workflow_topics(workspace_id: web::Path<String>) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/topics/generate",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn generate_workflow_dataset(workspace_id: web::Path<String>) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/dataset/generate",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn get_workflow_dataset_generate_status(
    workspace_id: web::Path<String>,
) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/dataset/generate/status",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn run_workflow_evaluator(workspace_id: web::Path<String>) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/evaluator/run",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn get_workflow_evaluator_run_status(
    workspace_id: web::Path<String>,
) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/evaluator/run/status",
        "GET",
        &workspace_id.into_inner(),
    ))
}
