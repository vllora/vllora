//! `GatewayClient` trait — Block 0 Feature 002 contract.
//!
//! The trait is the interface Track B's pipeline verbs + workers consume. The
//! concrete impl (`crate::client::LangdbCloudFinetuneClient`) is adapted to
//! implement this trait during Feature 002 — Track A's job.
//!
//! Track B writes verb + worker tests against this trait via a
//! `MockGatewayClient` so they never depend on a live gateway.
//!
//! Canonical contract:
//! `finetune-workflow-speckit/specs/002-state-and-gateway-client/contracts/gateway-client.rs`

use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Placeholder error alias. Track A replaces with a typed `GatewayError` enum
/// mapping to Feature 001's error contract (`INVALID_REQUEST` / `NOT_FOUND` /
/// `UNAUTHORIZED` / `FORBIDDEN` / `CONFLICT`).
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

/// Short-hand for an owned, send-safe boxed future. Used instead of
/// `async fn in trait` so the trait stays object-safe behind `dyn GatewayClient`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The contract Feature 003's pipeline verbs depend on. Mockable for unit
/// tests; concrete impl wraps the existing `LangdbCloudFinetuneClient`
/// (adapter added by Track A during Feature 002).
pub trait GatewayClient: Send + Sync {
    // ----- Workflow lifecycle -----
    fn create_workflow<'a>(
        &'a self,
        objective: &'a str,
        base_model: &'a str,
    ) -> BoxFuture<'a, Result<Uuid>>;

    // ----- Uploads (carry Feature 001 traceability fields) -----
    fn upload_source_documents<'a>(
        &'a self,
        workflow_id: Uuid,
        docs: Vec<SourceDocument>,
    ) -> BoxFuture<'a, Result<()>>;

    fn upload_knowledge_parts<'a>(
        &'a self,
        workflow_id: Uuid,
        parts: Vec<KnowledgePart>,
    ) -> BoxFuture<'a, Result<()>>;

    fn upload_topics<'a>(
        &'a self,
        workflow_id: Uuid,
        topics: Vec<Topic>,
    ) -> BoxFuture<'a, Result<Vec<Uuid>>>;

    fn upload_records<'a>(
        &'a self,
        workflow_id: Uuid,
        records: Vec<Record>,
    ) -> BoxFuture<'a, Result<()>>;

    fn upload_grader<'a>(
        &'a self,
        workflow_id: Uuid,
        source_js: &'a str,
        change_reason: &'a str,
    ) -> BoxFuture<'a, Result<i32>>;

    // ----- Eval lifecycle -----
    fn create_eval_run<'a>(
        &'a self,
        workflow_id: Uuid,
        model: &'a str,
        grader_version: i32,
    ) -> BoxFuture<'a, Result<Uuid>>;

    fn poll_eval_run<'a>(&'a self, run_id: Uuid) -> BoxFuture<'a, Result<EvalRunStatus>>;

    fn cancel_eval<'a>(&'a self, run_id: Uuid) -> BoxFuture<'a, Result<()>>;

    // ----- Training lifecycle -----
    fn create_training_job<'a>(
        &'a self,
        workflow_id: Uuid,
        model: &'a str,
        grader_version: i32,
        config: serde_json::Value,
    ) -> BoxFuture<'a, Result<Uuid>>;

    fn poll_training_metrics<'a>(
        &'a self,
        job_id: Uuid,
        since_step: u64,
    ) -> BoxFuture<'a, Result<Vec<TrainingMetricPoint>>>;

    fn cancel_training<'a>(&'a self, job_id: Uuid) -> BoxFuture<'a, Result<()>>;

    // ----- Journal / analysis mirror (server-side copies of local state) -----
    fn put_pipeline_journal<'a>(
        &'a self,
        workflow_id: Uuid,
        journal_json: &'a str,
    ) -> BoxFuture<'a, Result<()>>;

    fn put_iteration_state<'a>(
        &'a self,
        workflow_id: Uuid,
        analysis_json: &'a str,
    ) -> BoxFuture<'a, Result<()>>;
}

// ----- Request / response types -----

/// Per-record source-document payload. `origin_uri` is the Feature 001
/// traceability field — never dropped between worker and DB row.
pub struct SourceDocument {
    pub origin_uri: String,
    pub origin_source_id: Option<String>,
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

pub struct KnowledgePart {
    pub document_id: Uuid,
    pub origin_uri: String,
    pub text: String,
    pub ordinal: u32,
}

/// Topics are addressed by local slug on the client side; gateway assigns UUIDs
/// and returns them in `upload_topics`.
pub struct Topic {
    pub slug: String,
    pub parent_slug: Option<String>,
    pub system_prompt: String,
    pub description: String,
}

pub struct Record {
    pub topic_id: Uuid,
    pub origin_uri: Option<String>,
    pub origin_source_id: Option<String>,
    pub messages_json: String,
    pub ground_truth_json: String,
    pub tools_json: Option<String>,
}

pub struct EvalRunStatus {
    pub run_id: Uuid,
    pub state: JobState,
    pub progress_pct: Option<u8>,
    pub readiness_score: Option<f32>,
    pub avg_score: Option<f32>,
    pub scored_count: Option<u64>,
    pub zero_score_count: Option<u64>,
    pub perfect_score_count: Option<u64>,
}

pub struct TrainingMetricPoint {
    pub step: u64,
    pub loss: f32,
    pub kl: f32,
    pub reward_mean: f32,
    pub reward_std: f32,
    pub clipped_frac: f32,
    pub emitted_at: DateTime<Utc>,
}

pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
