use actix_web::{error, web, HttpResponse, Result};
use serde::Deserialize;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::workflow_topic::DbNewWorkflowTopic;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::workflow_topic::WorkflowTopicService;

fn map_db_error(err: DatabaseError) -> actix_web::Error {
    match err {
        DatabaseError::QueryError(diesel::result::Error::NotFound) => {
            error::ErrorNotFound("Topic not found")
        }
        other => error::ErrorInternalServerError(other),
    }
}

#[derive(Debug, Deserialize)]
pub struct TopicInput {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub selected: bool,
    pub source_chunk_refs: Option<serde_json::Value>,
}

impl TopicInput {
    fn into_db_topic(self, workflow_id: &str) -> DbNewWorkflowTopic {
        DbNewWorkflowTopic {
            id: self.id,
            workflow_id: workflow_id.to_string(),
            name: self.name,
            parent_id: self.parent_id,
            selected: if self.selected { 1 } else { 0 },
            source_chunk_refs: self.source_chunk_refs.map(|v| v.to_string()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateTopicsRequest {
    pub topics: Vec<TopicInput>,
}

pub async fn list_topics(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowTopicService::new(db_pool.get_ref().clone());
    let topics = service.list(&workflow_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "topics": topics })))
}

pub async fn create_topics(
    workflow_id: web::Path<String>,
    body: web::Json<CreateTopicsRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowTopicService::new(db_pool.get_ref().clone());

    let db_topics: Vec<DbNewWorkflowTopic> = payload
        .topics
        .into_iter()
        .map(|t| t.into_db_topic(&workflow_id))
        .collect();

    let count = service
        .create(&workflow_id, db_topics)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Created().json(serde_json::json!({ "created": count })))
}

pub async fn delete_all_topics(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowTopicService::new(db_pool.get_ref().clone());

    let count = service.delete_all(&workflow_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "deleted": count })))
}
