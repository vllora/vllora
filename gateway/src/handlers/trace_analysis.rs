//! HTTP handlers for trace analysis results (trace-informed curriculum).
//!
//! Stores and retrieves the 4 artifacts produced by `trace_analyze.py`
//! for a given workflow. One row per workflow (upsert semantics).
//! Returns 404 for workflows that don't have trace analysis (PDF-only mode).

use actix_web::{error, web, HttpResponse, Result};
use vllora_core::metadata::models::trace_analysis::NewTraceAnalysis;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::trace_analysis::TraceAnalysisService;

/// `GET /finetune/workflows/{workflow_id}/trace-analysis`
///
/// Returns the trace analysis artifacts for a workflow, or 404 if none exists.
pub async fn get_trace_analysis(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = TraceAnalysisService::new(db_pool.get_ref().clone());

    match service.get_by_workflow(&workflow_id) {
        Ok(Some(analysis)) => Ok(HttpResponse::Ok().json(analysis)),
        Ok(None) => Err(error::ErrorNotFound(
            "No trace analysis for this workflow",
        )),
        Err(e) => Err(error::ErrorInternalServerError(e)),
    }
}

/// `PUT /finetune/workflows/{workflow_id}/trace-analysis`
///
/// Upsert trace analysis data. Replaces any existing analysis for this workflow.
pub async fn put_trace_analysis(
    workflow_id: web::Path<String>,
    body: web::Json<NewTraceAnalysis>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let input = body.into_inner();
    let service = TraceAnalysisService::new(db_pool.get_ref().clone());

    let analysis = service
        .upsert(&workflow_id, input)
        .map_err(error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().json(analysis))
}
