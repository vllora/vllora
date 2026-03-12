use actix_web::{error, web, HttpResponse, Result};
use serde::Deserialize;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::workflow_topic::{DbNewWorkflowTopic, TopicUpdateInput};
use vllora_core::metadata::models::workflow_topic_source::{TopicSourceCreateInput, TopicSourceUpdateInput};
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::workflow_topic::WorkflowTopicService;
use uuid::Uuid;

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
    pub id: Option<String>,
    pub reference_id: Option<String>,
    pub name: String,
    pub parent_id: Option<String>,
    pub system_prompt: Option<String>,
}

impl TopicInput {
    fn into_db_topic(self, workflow_id: &str) -> DbNewWorkflowTopic {
        DbNewWorkflowTopic {
            id: self.id,
            reference_id: self.reference_id,
            workflow_id: workflow_id.to_string(),
            name: self.name,
            parent_id: self.parent_id,
            system_prompt: self.system_prompt,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateTopicsRequest {
    pub topics: Vec<TopicInput>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTopicsRequest {
    pub topics: Vec<TopicUpdateInput>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteTopicsRequest {
    pub identifiers: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTopicRelationsRequest {
    pub relations: Vec<TopicSourceCreateInput>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTopicRelationsRequest {
    pub relations: Vec<TopicSourceUpdateInput>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteTopicRelationsRequest {
    pub identifiers: Vec<String>,
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
        .map(|mut t| {
            if t.id.is_none() {
                t.id = Some(Uuid::new_v4().to_string());
            }
            t
        })
        .map(|t| t.into_db_topic(&workflow_id))
        .collect();

    let count = service
        .create(&workflow_id, db_topics)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Created().json(serde_json::json!({ "created": count })))
}

pub async fn update_topics(
    workflow_id: web::Path<String>,
    body: web::Json<UpdateTopicsRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowTopicService::new(db_pool.get_ref().clone());

    let count = service
        .update_many(&workflow_id, payload.topics)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "updated": count })))
}

pub async fn delete_topics(
    workflow_id: web::Path<String>,
    body: web::Json<DeleteTopicsRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowTopicService::new(db_pool.get_ref().clone());

    let count = service
        .delete_many(&workflow_id, payload.identifiers)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "deleted": count })))
}

pub async fn list_topic_source_relations(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowTopicService::new(db_pool.get_ref().clone());
    let relations = service.list_relations(&workflow_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "relations": relations })))
}

pub async fn create_topic_source_relations(
    workflow_id: web::Path<String>,
    body: web::Json<CreateTopicRelationsRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowTopicService::new(db_pool.get_ref().clone());

    let count = service
        .create_relations(&workflow_id, payload.relations)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Created().json(serde_json::json!({ "created": count })))
}

pub async fn update_topic_source_relations(
    workflow_id: web::Path<String>,
    body: web::Json<UpdateTopicRelationsRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowTopicService::new(db_pool.get_ref().clone());

    let count = service
        .update_relations(&workflow_id, payload.relations)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "updated": count })))
}

pub async fn delete_topic_source_relations(
    workflow_id: web::Path<String>,
    body: web::Json<DeleteTopicRelationsRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowTopicService::new(db_pool.get_ref().clone());

    let count = service
        .delete_relations(&workflow_id, payload.identifiers)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "deleted": count })))
}
