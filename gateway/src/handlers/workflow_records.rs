use actix_web::{error, web, HttpResponse, Result};
use serde::Deserialize;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::workflow_record::DbNewWorkflowRecord;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::workflow_record::{
    WorkflowRecordScoreService, WorkflowRecordService,
};
use vllora_core::types::handlers::pagination::{PaginatedResult, Pagination};

fn map_db_error(err: DatabaseError) -> actix_web::Error {
    match err {
        DatabaseError::QueryError(diesel::result::Error::NotFound) => {
            error::ErrorNotFound("Record not found")
        }
        other => error::ErrorInternalServerError(other),
    }
}

#[derive(Debug, Deserialize)]
pub struct RecordInput {
    pub id: String,
    pub data: serde_json::Value,
    #[serde(alias = "topic")]
    pub topic_id: Option<String>,
    pub span_id: Option<String>,
    #[serde(default)]
    pub is_generated: bool,
    pub source_record_id: Option<String>,
    pub metadata: Option<String>,
}

impl RecordInput {
    fn into_db_record(self, workflow_id: &str) -> DbNewWorkflowRecord {
        DbNewWorkflowRecord {
            id: self.id,
            workflow_id: workflow_id.to_string(),
            data: self.data.to_string(),
            topic_id: self.topic_id,
            span_id: self.span_id,
            is_generated: if self.is_generated { 1 } else { 0 },
            source_record_id: self.source_record_id,
            metadata: self.metadata,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AddRecordsRequest {
    pub records: Vec<RecordInput>,
}

#[derive(Debug, Deserialize)]
pub struct ReplaceRecordsRequest {
    pub records: Vec<RecordInput>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTopicRequest {
    #[serde(alias = "topic")]
    pub topic_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TopicUpdate {
    pub record_id: String,
    #[serde(alias = "topic")]
    pub topic_id: String,
}

#[derive(Debug, Deserialize)]
pub struct BatchUpdateTopicsRequest {
    pub updates: Vec<TopicUpdate>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDataRequest {
    pub data: String,
}

#[derive(Debug, Deserialize)]
pub struct RenameTopicRequest {
    #[serde(alias = "old_name")]
    pub old_topic_id: String,
    #[serde(alias = "new_name")]
    pub new_topic_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ListRecordsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub topic_id: Option<String>,
}

/// List records — supports optional pagination via `?limit=N&offset=M`.
/// Optional `?topic_id=UUID` filter narrows to a single topic.
/// Without query params: returns `{ "records": [...] }` (backward-compatible).
/// With query params: returns `{ "data": [...], "pagination": { offset, limit, total } }`.
pub async fn list_records(
    workflow_id: web::Path<String>,
    query: web::Query<ListRecordsQuery>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());

    if let Some(limit) = query.limit {
        let offset = query.offset.unwrap_or(0);
        let (records, total) = if let Some(ref topic_id) = query.topic_id {
            service
                .list_paginated_by_topic(&workflow_id, topic_id, limit, offset)
                .map_err(map_db_error)?
        } else {
            service
                .list_paginated(&workflow_id, limit, offset)
                .map_err(map_db_error)?
        };
        Ok(HttpResponse::Ok().json(PaginatedResult::new(
            records,
            Pagination {
                offset,
                limit,
                total,
            },
        )))
    } else {
        let records = service.list(&workflow_id).map_err(map_db_error)?;
        Ok(HttpResponse::Ok().json(serde_json::json!({ "records": records })))
    }
}

pub async fn records_summary(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());
    let summary = service.summary(&workflow_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(summary))
}

#[derive(Debug, Deserialize)]
pub struct SpanExistsQuery {
    pub span_id: String,
}

pub async fn span_exists(
    workflow_id: web::Path<String>,
    query: web::Query<SpanExistsQuery>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());
    let exists = service
        .span_exists(&workflow_id, &query.span_id)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "exists": exists })))
}

pub async fn count_records(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());
    let count = service.count(&workflow_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "count": count })))
}

pub async fn counts_by_topic(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());
    let counts = service
        .counts_by_topic(&workflow_id)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "counts": counts })))
}

pub async fn add_records(
    workflow_id: web::Path<String>,
    body: web::Json<AddRecordsRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());

    let db_records: Vec<DbNewWorkflowRecord> = payload
        .records
        .into_iter()
        .map(|r| r.into_db_record(&workflow_id))
        .collect();

    let count = service
        .add(&workflow_id, db_records)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Created().json(serde_json::json!({ "added": count })))
}

pub async fn replace_records(
    workflow_id: web::Path<String>,
    body: web::Json<ReplaceRecordsRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());

    let db_records: Vec<DbNewWorkflowRecord> = payload
        .records
        .into_iter()
        .map(|r| r.into_db_record(&workflow_id))
        .collect();

    let count = service
        .replace_all(&workflow_id, db_records)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "replaced": count })))
}

pub async fn update_record_topic(
    path: web::Path<(String, String)>,
    body: web::Json<UpdateTopicRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, record_id) = path.into_inner();
    let payload = body.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());

    service
        .update_topic(&workflow_id, &record_id, payload.topic_id.as_deref())
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "updated": true })))
}

pub async fn batch_update_topics(
    workflow_id: web::Path<String>,
    body: web::Json<BatchUpdateTopicsRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());

    let updates: Vec<(&str, &str)> = payload
        .updates
        .iter()
        .map(|u| (u.record_id.as_str(), u.topic_id.as_str()))
        .collect();

    service
        .batch_update_topics(&workflow_id, &updates)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "updated": true })))
}

pub async fn update_record_data(
    path: web::Path<(String, String)>,
    body: web::Json<UpdateDataRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, record_id) = path.into_inner();
    let payload = body.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());

    service
        .update_data(&workflow_id, &record_id, &payload.data)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "updated": true })))
}

pub async fn rename_topic(
    workflow_id: web::Path<String>,
    body: web::Json<RenameTopicRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());

    let count = service
        .rename_topic(&workflow_id, &payload.old_topic_id, &payload.new_topic_id)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "renamed": count })))
}

pub async fn clear_topic(
    path: web::Path<(String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, topic_id) = path.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());

    let count = service
        .clear_topic(&workflow_id, &topic_id)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "cleared": count })))
}

pub async fn clear_all_topics(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());

    let count = service
        .clear_all_topics(&workflow_id)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "cleared": count })))
}

pub async fn delete_record(
    path: web::Path<(String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (workflow_id, record_id) = path.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());

    service
        .delete(&workflow_id, &record_id)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "deleted": true })))
}

pub async fn delete_all_records(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowRecordService::new(db_pool.get_ref().clone());

    let count = service.delete_all(&workflow_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "deleted": count })))
}

pub async fn list_record_scores(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowRecordScoreService::new(db_pool.get_ref().clone());
    let scores = service
        .list_by_workflow(&workflow_id)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "scores": scores })))
}
