//! HTTP handlers for OTel GenAI trace bundles.
//!
//! Trace bundles are the storage side of the OTel finetune pipeline (Track A).
//! A bundle is the raw set of semconv spans that back a `knowledge_sources`
//! row of kind `otel-trace`. See
//! `vllora/ui/docs/workflow-skill-first-approach/trace-pipeline-implementation-plan.md`.

use actix_web::{error, web, HttpResponse, Result};
use serde::Deserialize;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::trace_bundle::NewTraceBundle;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::trace_bundle::TraceBundleService;

fn map_db_error(err: DatabaseError) -> actix_web::Error {
    match err {
        DatabaseError::QueryError(diesel::result::Error::NotFound) => {
            error::ErrorNotFound("Trace bundle not found")
        }
        other => error::ErrorInternalServerError(other),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateTraceBundleRequest {
    pub name: String,
    pub semconv_spans: serde_json::Value,
}

/// `POST /finetune/workflows/{workflow_id}/trace-bundles`
///
/// Accepts `{name, semconv_spans: [...]}`. Computes rollup metadata from the
/// blob (span_count, distinct tool_names, distinct model_names), persists the
/// row, and returns the full bundle including the parsed semconv blob.
pub async fn create_trace_bundle(
    workflow_id: web::Path<String>,
    body: web::Json<CreateTraceBundleRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let req = body.into_inner();

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(error::ErrorBadRequest("name is required"));
    }
    if !req.semconv_spans.is_array() {
        return Err(error::ErrorBadRequest(
            "semconv_spans must be a JSON array of spans",
        ));
    }

    let service = TraceBundleService::new(db_pool.get_ref().clone());
    let bundle = service
        .create(NewTraceBundle {
            workflow_id,
            name,
            semconv_spans: req.semconv_spans,
        })
        .map_err(map_db_error)?;

    Ok(HttpResponse::Created().json(bundle))
}

/// `GET /finetune/workflows/{workflow_id}/trace-bundles/{bundle_id}`
///
/// Returns the full bundle row including the parsed `semconv_spans` blob.
pub async fn get_trace_bundle(
    path: web::Path<(String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (_workflow_id, bundle_id) = path.into_inner();
    let service = TraceBundleService::new(db_pool.get_ref().clone());
    let bundle = service.get(&bundle_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(bundle))
}
