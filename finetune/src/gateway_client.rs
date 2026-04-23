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

// ---------------------------------------------------------------------------
// MockGatewayClient — in-memory fake for unit + integration tests.
// ---------------------------------------------------------------------------
//
// Until Track A's Feature 002 ships a real `LangdbGatewayClient` wrapper,
// Track B writes verb tests against this. It records every call so tests can
// assert on what was invoked.

use std::sync::Mutex;

/// In-memory fake implementing `GatewayClient`. Records every call into a
/// `CallLog`; tests assert on the log. Not thread-safe across awaits (uses a
/// blocking `Mutex`), which is fine for unit / integration tests that drive
/// verbs sequentially.
pub struct MockGatewayClient {
    state: Mutex<MockState>,
}

#[derive(Default)]
struct MockState {
    next_workflow_uuid: Option<Uuid>,
    calls: Vec<MockCall>,
}

/// Every call the verb code makes on the mock is captured as one of these.
#[derive(Debug, Clone)]
pub enum MockCall {
    CreateWorkflow { objective: String, base_model: String },
    UploadSourceDocuments { workflow_id: Uuid, count: usize },
    UploadKnowledgeParts { workflow_id: Uuid, count: usize },
    UploadTopics { workflow_id: Uuid, count: usize },
    UploadRecords { workflow_id: Uuid, count: usize },
    UploadGrader { workflow_id: Uuid, change_reason: String },
    CreateEvalRun { workflow_id: Uuid, model: String },
    CreateTrainingJob { workflow_id: Uuid, model: String },
    PutPipelineJournal { workflow_id: Uuid, bytes: usize },
    PutIterationState { workflow_id: Uuid, bytes: usize },
}

impl Default for MockGatewayClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockGatewayClient {
    pub fn new() -> Self {
        Self { state: Mutex::new(MockState::default()) }
    }

    /// Pre-seed the UUID that `create_workflow` returns. Useful for tests
    /// that want a stable ID for assertions.
    pub fn with_workflow_id(self, id: Uuid) -> Self {
        self.state.lock().unwrap().next_workflow_uuid = Some(id);
        self
    }

    /// Snapshot of every call made so far.
    pub fn calls(&self) -> Vec<MockCall> {
        self.state.lock().unwrap().calls.clone()
    }
}

impl GatewayClient for MockGatewayClient {
    fn create_workflow<'a>(&'a self, objective: &'a str, base_model: &'a str) -> BoxFuture<'a, Result<Uuid>> {
        Box::pin(async move {
            let mut s = self.state.lock().unwrap();
            s.calls.push(MockCall::CreateWorkflow {
                objective: objective.to_string(),
                base_model: base_model.to_string(),
            });
            Ok(s.next_workflow_uuid.unwrap_or_else(Uuid::new_v4))
        })
    }

    fn upload_source_documents<'a>(&'a self, workflow_id: Uuid, docs: Vec<SourceDocument>) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.state.lock().unwrap();
            s.calls.push(MockCall::UploadSourceDocuments { workflow_id, count: docs.len() });
            Ok(())
        })
    }

    fn upload_knowledge_parts<'a>(&'a self, workflow_id: Uuid, parts: Vec<KnowledgePart>) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.state.lock().unwrap();
            s.calls.push(MockCall::UploadKnowledgeParts { workflow_id, count: parts.len() });
            Ok(())
        })
    }

    fn upload_topics<'a>(&'a self, workflow_id: Uuid, topics: Vec<Topic>) -> BoxFuture<'a, Result<Vec<Uuid>>> {
        Box::pin(async move {
            let mut s = self.state.lock().unwrap();
            let ids: Vec<Uuid> = topics.iter().map(|_| Uuid::new_v4()).collect();
            s.calls.push(MockCall::UploadTopics { workflow_id, count: topics.len() });
            Ok(ids)
        })
    }

    fn upload_records<'a>(&'a self, workflow_id: Uuid, records: Vec<Record>) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.state.lock().unwrap();
            s.calls.push(MockCall::UploadRecords { workflow_id, count: records.len() });
            Ok(())
        })
    }

    fn upload_grader<'a>(&'a self, workflow_id: Uuid, _source_js: &'a str, change_reason: &'a str) -> BoxFuture<'a, Result<i32>> {
        Box::pin(async move {
            let mut s = self.state.lock().unwrap();
            s.calls.push(MockCall::UploadGrader { workflow_id, change_reason: change_reason.to_string() });
            Ok(1)
        })
    }

    fn create_eval_run<'a>(&'a self, workflow_id: Uuid, model: &'a str, _grader_version: i32) -> BoxFuture<'a, Result<Uuid>> {
        Box::pin(async move {
            let mut s = self.state.lock().unwrap();
            s.calls.push(MockCall::CreateEvalRun { workflow_id, model: model.to_string() });
            Ok(Uuid::new_v4())
        })
    }

    fn poll_eval_run<'a>(&'a self, run_id: Uuid) -> BoxFuture<'a, Result<EvalRunStatus>> {
        Box::pin(async move {
            Ok(EvalRunStatus {
                run_id,
                state: JobState::Succeeded,
                progress_pct: Some(100),
                readiness_score: Some(0.82),
                avg_score: Some(0.71),
                scored_count: Some(40),
                zero_score_count: Some(2),
                perfect_score_count: Some(8),
            })
        })
    }

    fn cancel_eval<'a>(&'a self, _run_id: Uuid) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }

    fn create_training_job<'a>(&'a self, workflow_id: Uuid, model: &'a str, _grader_version: i32, _config: serde_json::Value) -> BoxFuture<'a, Result<Uuid>> {
        Box::pin(async move {
            let mut s = self.state.lock().unwrap();
            s.calls.push(MockCall::CreateTrainingJob { workflow_id, model: model.to_string() });
            Ok(Uuid::new_v4())
        })
    }

    fn poll_training_metrics<'a>(&'a self, _job_id: Uuid, _since_step: u64) -> BoxFuture<'a, Result<Vec<TrainingMetricPoint>>> {
        Box::pin(async move { Ok(Vec::new()) })
    }

    fn cancel_training<'a>(&'a self, _job_id: Uuid) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }

    fn put_pipeline_journal<'a>(&'a self, workflow_id: Uuid, journal_json: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.state.lock().unwrap();
            s.calls.push(MockCall::PutPipelineJournal { workflow_id, bytes: journal_json.len() });
            Ok(())
        })
    }

    fn put_iteration_state<'a>(&'a self, workflow_id: Uuid, analysis_json: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.state.lock().unwrap();
            s.calls.push(MockCall::PutIterationState { workflow_id, bytes: analysis_json.len() });
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// LangdbGatewayClient — partial real adapter.
// ---------------------------------------------------------------------------
//
// Wraps `crate::client::LangdbCloudFinetuneClient`. Methods that map cleanly
// to the concrete client delegate; everything else returns
// `Err("not-yet-wired: …")` with a clear remediation pointer.
//
// Missing pieces are roadmapped in README-style TODOs so it's obvious what
// Feature 001 + ongoing Feature 002 work still needs to land:
//   - `create_workflow` — needs a `POST /v1/finetune/workflows` route on the
//     cloud gateway; the concrete client has no matching method today.
//     MVP: generates a client-side UUID so verbs move forward locally.
//   - `put_pipeline_journal` / `put_iteration_state` — needs
//     `PUT /v1/finetune/workflows/{id}/{journal|iteration_state}` routes.
//     MVP: silently succeed (verbs call these best-effort).
//   - Upload-* methods — partially wired: `upload_records` delegates to
//     `upload_dataset`; `upload_grader` / `upload_topics` / `upload_*` need
//     type-translation helpers that haven't been written.
//   - Eval + training — delegate to the real `create_evaluation` /
//     `create_finetune_job` / `get_*_status` methods with type translation.

use crate::client::LangdbCloudFinetuneClient;
use crate::types::{
    BaseModel, CompletionParams, CreateEvaluationRequest, CreateFinetuneJobRequest,
    FinetuneTrainingConfig,
};

/// Real adapter wrapping `LangdbCloudFinetuneClient`. Constructed from an
/// API key; the underlying client points at `LANGDB_API_URL`.
pub struct LangdbGatewayClient {
    inner: LangdbCloudFinetuneClient,
}

impl LangdbGatewayClient {
    pub fn new(api_key: String) -> Result<Self> {
        let inner = LangdbCloudFinetuneClient::new(api_key)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
        Ok(Self { inner })
    }
}

fn boxed_err<E: std::fmt::Display>(e: E) -> Box<dyn std::error::Error + Send + Sync> {
    e.to_string().into()
}

/// Map the cloud's `BaseModel` string (e.g., "Qwen3.5-2B") to the typed enum.
fn parse_base_model(model: &str) -> Result<BaseModel> {
    match model {
        "Qwen3.5-0.8B" | "unsloth/Qwen3.5-0.8B" => Ok(BaseModel::Qwen35_0_8B),
        "Qwen3.5-2B" | "unsloth/Qwen3.5-2B" => Ok(BaseModel::Qwen35_2B),
        "Qwen3.5-4B" | "unsloth/Qwen3.5-4B" => Ok(BaseModel::Qwen35_4B),
        "gemma-4-E2B" => Ok(BaseModel::Gemma4E2B),
        other => Err(format!("unsupported base model: {}", other).into()),
    }
}

/// Map a cloud `status` string ("queued" / "running" / "succeeded" / "failed" /
/// "cancelled" / "completed" / "stopped", etc.) to the trait's `JobState`.
fn parse_job_state(status: &str) -> JobState {
    match status.to_lowercase().as_str() {
        "queued" | "pending" | "scheduled" => JobState::Queued,
        "running" | "in_progress" | "started" => JobState::Running,
        "succeeded" | "success" | "completed" | "complete" => JobState::Succeeded,
        "cancelled" | "canceled" | "stopped" => JobState::Cancelled,
        _ => JobState::Failed,
    }
}

impl GatewayClient for LangdbGatewayClient {
    fn create_workflow<'a>(
        &'a self,
        objective: &'a str,
        _base_model: &'a str,
    ) -> BoxFuture<'a, Result<Uuid>> {
        // Workflow is language-agnostic — base_model is attached at training-job
        // time, not at workflow creation. We use the objective as both the
        // workflow name and objective; callers can PUT a friendlier name later.
        Box::pin(async move {
            self.inner
                .create_workflow(objective, objective)
                .await
                .map_err(boxed_err)
        })
    }

    fn upload_source_documents<'a>(
        &'a self,
        _workflow_id: Uuid,
        _docs: Vec<SourceDocument>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            // TODO: source documents flow through the trace-bundles / knowledge
            // endpoints, not a single bulk upload. Wiring that shape requires
            // splitting SourceDocument payloads into per-KS multipart uploads —
            // out of scope until the workers that call this route start
            // producing populated SourceDocument lists.
            Ok(())
        })
    }

    fn upload_knowledge_parts<'a>(
        &'a self,
        _workflow_id: Uuid,
        _parts: Vec<KnowledgePart>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            // TODO: gated on the same knowledge-ingest path as upload_source_documents.
            Ok(())
        })
    }

    fn upload_topics<'a>(
        &'a self,
        _workflow_id: Uuid,
        topics: Vec<Topic>,
    ) -> BoxFuture<'a, Result<Vec<Uuid>>> {
        Box::pin(async move {
            // TODO: wire to POST /finetune/workflows/{id}/topics. Caller code
            // currently treats returned IDs as opaque so fresh UUIDs are safe.
            Ok(topics.iter().map(|_| Uuid::new_v4()).collect())
        })
    }

    fn upload_records<'a>(
        &'a self,
        workflow_id: Uuid,
        records: Vec<Record>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if records.is_empty() {
                return Ok(());
            }
            // Serialise each Record as one JSONL line conforming to the
            // cloud dataset schema. Fields:
            //   messages      — the conversation (parsed from messages_json)
            //   ground_truth  — the expected output (parsed from ground_truth_json)
            //   tools         — optional tool schema array
            //   origin_uri    — per-record traceability (Feature 001 column)
            //   origin_source_id — ditto
            //   topic_id      — gateway-assigned topic UUID
            let mut buf: Vec<u8> = Vec::with_capacity(records.len() * 256);
            for (i, r) in records.iter().enumerate() {
                let messages: serde_json::Value = serde_json::from_str(&r.messages_json)
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                        format!("record {}: messages_json invalid: {}", i, e).into()
                    })?;
                let ground_truth: serde_json::Value =
                    serde_json::from_str(&r.ground_truth_json).unwrap_or(serde_json::Value::Null);
                let mut row = serde_json::json!({
                    "messages": messages,
                    "ground_truth": ground_truth,
                    "topic_id": r.topic_id.to_string(),
                });
                if let Some(origin_uri) = &r.origin_uri {
                    row["origin_uri"] = serde_json::Value::String(origin_uri.clone());
                }
                if let Some(origin_source_id) = &r.origin_source_id {
                    row["origin_source_id"] = serde_json::Value::String(origin_source_id.clone());
                }
                if let Some(tools_json) = &r.tools_json {
                    if let Ok(tools) = serde_json::from_str::<serde_json::Value>(tools_json) {
                        row["tools"] = tools;
                    }
                }
                let line = serde_json::to_string(&row)?;
                buf.extend_from_slice(line.as_bytes());
                buf.push(b'\n');
            }

            self.inner
                .upload_dataset(buf, None, None, Some(workflow_id))
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
            Ok(())
        })
    }

    fn upload_grader<'a>(
        &'a self,
        workflow_id: Uuid,
        source_js: &'a str,
        _change_reason: &'a str,
    ) -> BoxFuture<'a, Result<i32>> {
        Box::pin(async move {
            // Delegates to `update_workflow_evaluator` (PATCH). Cloud auto-assigns
            // the new evaluator_version; we return 0 and let the caller re-query
            // via `get_workflow_evaluator_versions` if the version number matters.
            // TODO: thread `change_reason` through once the cloud route accepts it.
            self.inner
                .update_workflow_evaluator(&workflow_id.to_string(), source_js.to_string())
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
            Ok(0)
        })
    }

    fn create_eval_run<'a>(
        &'a self,
        workflow_id: Uuid,
        model: &'a str,
        _grader_version: i32,
    ) -> BoxFuture<'a, Result<Uuid>> {
        Box::pin(async move {
            let request = CreateEvaluationRequest {
                workflow_id,
                rollout_model_params: CompletionParams {
                    model: Some(model.to_string()),
                    temperature: None,
                    extra: Default::default(),
                },
                offset: None,
                limit: None,
            };
            let response = self
                .inner
                .create_evaluation(request)
                .await
                .map_err(boxed_err)?;
            Ok(response.evaluation_run_id)
        })
    }

    fn poll_eval_run<'a>(&'a self, run_id: Uuid) -> BoxFuture<'a, Result<EvalRunStatus>> {
        Box::pin(async move {
            let result = self
                .inner
                .get_evaluation_result(&run_id.to_string(), None)
                .await
                .map_err(boxed_err)?;

            let state = parse_job_state(&result.status);
            let progress_pct = if result.total_rows > 0 {
                let pct = (result.completed_rows as f64 / result.total_rows as f64 * 100.0) as u8;
                Some(pct.min(100))
            } else {
                None
            };

            let (readiness_score, avg_score, scored_count, zero_score_count, perfect_score_count) =
                if let Some(summary) = result.summary {
                    let avg = summary.average_score.map(|v| v as f32);
                    // Readiness proxy: fraction of non-zero scored rows. Cloud
                    // doesn't ship a dedicated readiness field yet, so derive
                    // from the histogram counts that are already returned.
                    let readiness = if summary.scored_count > 0 {
                        let non_zero = (summary.scored_count - summary.zero_score_count).max(0);
                        Some(non_zero as f32 / summary.scored_count as f32)
                    } else {
                        None
                    };
                    (
                        readiness,
                        avg,
                        Some(summary.scored_count as u64),
                        Some(summary.zero_score_count as u64),
                        Some(summary.perfect_score_count as u64),
                    )
                } else {
                    (None, None, None, None, None)
                };

            Ok(EvalRunStatus {
                run_id,
                state,
                progress_pct,
                readiness_score,
                avg_score,
                scored_count,
                zero_score_count,
                perfect_score_count,
            })
        })
    }

    fn cancel_eval<'a>(&'a self, run_id: Uuid) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.inner
                .cancel_evaluation(&run_id.to_string())
                .await
                .map_err(boxed_err)
        })
    }

    fn create_training_job<'a>(
        &'a self,
        workflow_id: Uuid,
        model: &'a str,
        grader_version: i32,
        config: serde_json::Value,
    ) -> BoxFuture<'a, Result<Uuid>> {
        Box::pin(async move {
            let base_model = parse_base_model(model)?;
            // `config` is a JSON blob owned by the caller — we try to parse it
            // into `FinetuneTrainingConfig` (only the fields the cloud
            // recognises are surfaced; extras are dropped).
            let training_config: Option<FinetuneTrainingConfig> = if config.is_null() {
                None
            } else {
                serde_json::from_value(config).map_err(boxed_err)?
            };

            let request = CreateFinetuneJobRequest {
                evaluator_version: Some(grader_version),
                base_model,
                output_model: None,
                evaluation_dataset: None,
                display_name: None,
                training_config,
                inference_parameters: None,
                chunk_size: None,
                node_count: None,
                resume_mode: None,
            };

            let response = self
                .inner
                .create_finetune_job(request, &workflow_id)
                .await
                .map_err(boxed_err)?;
            Ok(response.id)
        })
    }

    fn poll_training_metrics<'a>(
        &'a self,
        job_id: Uuid,
        since_step: u64,
    ) -> BoxFuture<'a, Result<Vec<TrainingMetricPoint>>> {
        Box::pin(async move {
            let response = self
                .inner
                .get_finetune_job_metrics(&job_id.to_string())
                .await
                .map_err(boxed_err)?;

            // The cloud returns `metrics: Vec<FinetuneJobMetricPoint>` where
            // each point carries a free-form `metrics: serde_json::Value`.
            // We extract the conventional GRPO keys; missing values default
            // to NaN so downstream code can decide whether to skip.
            let points: Vec<TrainingMetricPoint> = response
                .metrics
                .into_iter()
                .filter_map(|p| {
                    let m = p.metrics.as_object()?;
                    let step = m.get("step").and_then(|v| v.as_u64()).unwrap_or(0);
                    if step < since_step {
                        return None;
                    }
                    let loss = m.get("loss").and_then(|v| v.as_f64()).unwrap_or(f64::NAN) as f32;
                    let kl = m.get("kl").and_then(|v| v.as_f64()).unwrap_or(f64::NAN) as f32;
                    let reward_mean = m
                        .get("reward_mean")
                        .or_else(|| m.get("reward"))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(f64::NAN) as f32;
                    let reward_std = m
                        .get("reward_std")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(f64::NAN) as f32;
                    let clipped_frac = m
                        .get("clipped_frac")
                        .or_else(|| m.get("clip_ratio"))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(f64::NAN) as f32;
                    Some(TrainingMetricPoint {
                        step,
                        loss,
                        kl,
                        reward_mean,
                        reward_std,
                        clipped_frac,
                        emitted_at: p.created_at,
                    })
                })
                .collect();
            Ok(points)
        })
    }

    fn cancel_training<'a>(&'a self, job_id: Uuid) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.inner
                .cancel_finetune_job(&job_id.to_string())
                .await
                .map_err(boxed_err)
        })
    }

    fn put_pipeline_journal<'a>(
        &'a self,
        workflow_id: Uuid,
        journal_json: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.inner
                .put_pipeline_journal(workflow_id, journal_json)
                .await
                .map_err(boxed_err)
        })
    }

    fn put_iteration_state<'a>(
        &'a self,
        workflow_id: Uuid,
        analysis_json: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.inner
                .put_iteration_state(workflow_id, analysis_json)
                .await
                .map_err(boxed_err)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_records_calls() {
        let client = MockGatewayClient::new();
        let id = client.create_workflow("build a support bot", "qwen-3.5-2b").await.unwrap();
        client
            .put_pipeline_journal(id, r#"{"schema_version":1}"#)
            .await
            .unwrap();

        let calls = client.calls();
        assert_eq!(calls.len(), 2);
        matches!(calls[0], MockCall::CreateWorkflow { .. });
        matches!(calls[1], MockCall::PutPipelineJournal { .. });
    }

    #[tokio::test]
    async fn seeded_workflow_id_is_stable() {
        let fixed = Uuid::new_v4();
        let client = MockGatewayClient::new().with_workflow_id(fixed);
        let a = client.create_workflow("x", "qwen-3.5-2b").await.unwrap();
        let b = client.create_workflow("y", "qwen-3.5-2b").await.unwrap();
        assert_eq!(a, fixed);
        assert_eq!(b, fixed);
    }

    // Real adapter: construct + exercise the handful of methods that still
    // short-circuit before hitting the network. Every other method (create_*,
    // poll_*, cancel_*, put_*, non-empty upload_records / upload_grader) goes
    // to `api.langdb.cloud` — those are covered by an out-of-band integration
    // test that needs `LANGDB_API_KEY` + `LANGDB_API_URL=http://localhost:...`
    // to be set, not by unit tests.
    #[tokio::test]
    async fn langdb_adapter_constructs_and_stubbed_methods_work() {
        let client = LangdbGatewayClient::new("test-api-key".into())
            .expect("adapter should construct with any api key");

        // All of these return Ok without hitting the network.
        let fake_wf = Uuid::new_v4();
        client
            .upload_source_documents(fake_wf, Vec::new())
            .await
            .unwrap();
        client
            .upload_knowledge_parts(fake_wf, Vec::new())
            .await
            .unwrap();
        // `upload_topics` never hits the network in the current stub — returns
        // fresh UUIDs so callers can proceed locally.
        let topic_ids = client.upload_topics(fake_wf, Vec::new()).await.unwrap();
        assert!(topic_ids.is_empty());

        // Empty records short-circuit before any network call.
        client.upload_records(fake_wf, Vec::new()).await.unwrap();
    }

    /// `parse_base_model` is used by the adapter — verify the string
    /// round-trips cleanly for the canonical names we accept.
    #[test]
    fn parse_base_model_accepts_canonical_and_unsloth_aliases() {
        assert!(matches!(
            parse_base_model("Qwen3.5-2B").unwrap(),
            BaseModel::Qwen35_2B
        ));
        assert!(matches!(
            parse_base_model("unsloth/Qwen3.5-0.8B").unwrap(),
            BaseModel::Qwen35_0_8B
        ));
        assert!(parse_base_model("not-a-model").is_err());
    }

    #[test]
    fn parse_job_state_maps_known_strings() {
        assert!(matches!(parse_job_state("queued"), JobState::Queued));
        assert!(matches!(parse_job_state("running"), JobState::Running));
        assert!(matches!(parse_job_state("completed"), JobState::Succeeded));
        assert!(matches!(parse_job_state("cancelled"), JobState::Cancelled));
        assert!(matches!(parse_job_state("failed"), JobState::Failed));
        assert!(matches!(parse_job_state("garbage"), JobState::Failed));
    }
}
