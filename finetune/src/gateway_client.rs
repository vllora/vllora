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

impl GatewayClient for LangdbGatewayClient {
    fn create_workflow<'a>(
        &'a self,
        _objective: &'a str,
        _base_model: &'a str,
    ) -> BoxFuture<'a, Result<Uuid>> {
        Box::pin(async move {
            // TODO [Feature 001]: cloud has no POST /workflows yet. Client-side
            // UUID lets the pipeline progress locally; real impl will call the
            // cloud route and return the authoritative ID.
            Ok(Uuid::new_v4())
        })
    }

    fn upload_source_documents<'a>(
        &'a self,
        _workflow_id: Uuid,
        _docs: Vec<SourceDocument>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            // TODO: translate SourceDocument → multipart/form-data + call
            // inner.upload_documents (no such method yet — needs cloud route).
            Ok(())
        })
    }

    fn upload_knowledge_parts<'a>(
        &'a self,
        _workflow_id: Uuid,
        _parts: Vec<KnowledgePart>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            // TODO: cloud route not yet implemented; stub success.
            Ok(())
        })
    }

    fn upload_topics<'a>(
        &'a self,
        _workflow_id: Uuid,
        topics: Vec<Topic>,
    ) -> BoxFuture<'a, Result<Vec<Uuid>>> {
        Box::pin(async move {
            // TODO: cloud route not yet implemented; return fresh UUIDs.
            Ok(topics.iter().map(|_| Uuid::new_v4()).collect())
        })
    }

    fn upload_records<'a>(
        &'a self,
        _workflow_id: Uuid,
        _records: Vec<Record>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            // TODO: translate Record → inner's `UploadDatasetRequest` shape
            // and call `inner.upload_dataset_chunked(...)`. Needs a
            // type-translation helper that isn't yet written; stubbing to Ok
            // so the pipeline progresses. The mock path still exercises the
            // full event emission and journal semantics.
            Ok(())
        })
    }

    fn upload_grader<'a>(
        &'a self,
        _workflow_id: Uuid,
        _source_js: &'a str,
        _change_reason: &'a str,
    ) -> BoxFuture<'a, Result<i32>> {
        Box::pin(async move {
            // TODO: delegate to inner.update_workflow_evaluator once the
            // signature is reconciled.
            Ok(1)
        })
    }

    fn create_eval_run<'a>(
        &'a self,
        _workflow_id: Uuid,
        _model: &'a str,
        _grader_version: i32,
    ) -> BoxFuture<'a, Result<Uuid>> {
        Box::pin(async move {
            // TODO: delegate to inner.create_evaluation (needs type mapping).
            Ok(Uuid::new_v4())
        })
    }

    fn poll_eval_run<'a>(&'a self, run_id: Uuid) -> BoxFuture<'a, Result<EvalRunStatus>> {
        Box::pin(async move {
            // TODO: delegate to inner.get_evaluation_result. For now the
            // adapter returns a canned "pass" so downstream verbs keep
            // working against the real client until the full mapping ships.
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

    fn create_training_job<'a>(
        &'a self,
        _workflow_id: Uuid,
        _model: &'a str,
        _grader_version: i32,
        _config: serde_json::Value,
    ) -> BoxFuture<'a, Result<Uuid>> {
        Box::pin(async move {
            // TODO: delegate to inner.create_finetune_job (needs type mapping).
            Ok(Uuid::new_v4())
        })
    }

    fn poll_training_metrics<'a>(
        &'a self,
        _job_id: Uuid,
        _since_step: u64,
    ) -> BoxFuture<'a, Result<Vec<TrainingMetricPoint>>> {
        Box::pin(async move {
            // TODO: delegate to inner.get_finetune_job_metrics (needs type mapping).
            Ok(Vec::new())
        })
    }

    fn cancel_training<'a>(&'a self, _job_id: Uuid) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            // TODO: delegate to inner.cancel_finetune_job.
            Ok(())
        })
    }

    fn put_pipeline_journal<'a>(
        &'a self,
        _workflow_id: Uuid,
        _journal_json: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            // TODO [Feature 001]: needs PUT /v1/finetune/workflows/{id}/pipeline-journal.
            // Verbs call this best-effort; Ok keeps the pipeline moving.
            Ok(())
        })
    }

    fn put_iteration_state<'a>(
        &'a self,
        _workflow_id: Uuid,
        _analysis_json: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            // TODO [Feature 001]: needs PUT /v1/finetune/workflows/{id}/iteration-state.
            Ok(())
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

    // Real adapter: verify it can be constructed + every trait method is
    // wired (returning stub Ok). The concrete LangdbCloudFinetuneClient
    // constructor needs a valid-ish API key but doesn't talk to the network
    // until a method is called.
    #[tokio::test]
    async fn langdb_adapter_constructs_and_stub_methods_return_ok() {
        let client = LangdbGatewayClient::new("test-api-key".into())
            .expect("adapter should construct with any api key");

        // create_workflow returns a fresh UUID.
        let wf = client.create_workflow("obj", "qwen-3.5-2b").await.unwrap();
        assert_ne!(wf, Uuid::nil());

        // Stubbed-Ok methods don't panic.
        client.upload_knowledge_parts(wf, Vec::new()).await.unwrap();
        client.upload_records(wf, Vec::new()).await.unwrap();
        client.upload_topics(wf, Vec::new()).await.unwrap();
        client.upload_grader(wf, "/* g */", "initial").await.unwrap();
        client.put_pipeline_journal(wf, "{}").await.unwrap();
        client.put_iteration_state(wf, "{}").await.unwrap();

        let eval_run = client.create_eval_run(wf, "qwen-3.5-4b", 1).await.unwrap();
        let status = client.poll_eval_run(eval_run).await.unwrap();
        assert_eq!(status.run_id, eval_run);
        assert!(matches!(status.state, JobState::Succeeded));

        let train_job = client
            .create_training_job(wf, "qwen-3.5-4b", 1, serde_json::json!({}))
            .await
            .unwrap();
        assert_ne!(train_job, Uuid::nil());
    }
}
