//! Worker abstraction + canned-output stub for Feature 003 MVP.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §6
//!
//! A verb invokes a worker to do LLM-heavy work. The worker shape:
//! `Worker::run(&self, input) -> WorkerResult`.
//!
//! Production: impls spawn `claude -p` subprocesses, parse stream-JSON, and
//! return a typed `WorkerResult`. Inherits user's Claude auth via environment
//! (never reads credential files — parent §2.10.1).
//!
//! MVP: a `StubClaudeClient` returns canned results matching the worker
//! contract. Verbs compile + test green against this. Swap to a real
//! subprocess impl when Track B ships `claude -p` integration without
//! changing callers.

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Placeholder error alias — same shape used by `finetune::state::Result`.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The canonical input every worker receives. Matches
/// `specs/003-cli-pipeline-verbs/contracts/worker-contract.md#workerinput-schema`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInput {
    pub worker: String,
    pub mode: Option<String>,
    pub workflow_id: String,
    pub phase: String,
    pub project_dir: String,
    #[serde(default)]
    pub inputs: Value,
    #[serde(default)]
    pub limits: Value,
}

impl WorkerInput {
    pub fn new(worker: &str, phase: &str, workflow_id: &str, project_dir: &str) -> Self {
        Self {
            worker: worker.to_string(),
            mode: None,
            workflow_id: workflow_id.to_string(),
            phase: phase.to_string(),
            project_dir: project_dir.to_string(),
            inputs: Value::Null,
            limits: Value::Null,
        }
    }

    pub fn with_mode(mut self, mode: &str) -> Self {
        self.mode = Some(mode.to_string());
        self
    }

    pub fn with_inputs(mut self, v: Value) -> Self {
        self.inputs = v;
        self
    }
}

/// The canonical output. Matches worker-contract.md#workerresult-schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResult {
    pub status: WorkerStatus,
    pub worker: String,
    pub mode: Option<String>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub reasoning: String,
    #[serde(default)]
    pub decisions: Vec<Decision>,
    #[serde(default)]
    pub metrics: Value,
    pub error: Option<WorkerError>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkerStatus {
    Ok,
    Incomplete,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub kind: String,
    pub r#ref: String,
    #[serde(default)]
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub label: String,
    pub choice: String,
    pub rationale: String,
    #[serde(default)]
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerError {
    pub code: String,
    pub message: String,
}

/// The trait every worker implements. Mockable for unit tests.
pub trait ClaudeClient: Send + Sync {
    fn run<'a>(&'a self, input: WorkerInput) -> BoxFuture<'a, Result<WorkerResult>>;
}

/// MVP stub: returns canned `ok` results. Real impl swaps in a
/// `claude -p` subprocess wrapper that streams events and parses them.
pub struct StubClaudeClient;

impl StubClaudeClient {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StubClaudeClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeClient for StubClaudeClient {
    fn run<'a>(&'a self, input: WorkerInput) -> BoxFuture<'a, Result<WorkerResult>> {
        Box::pin(async move {
            // Match the canned shapes `mock_vllora` emits for plugin behavioural
            // tests. Every worker exits `ok` with a one-artifact result.
            let worker = input.worker.clone();
            let mode = input.mode.clone();
            let (kind, count) = canned_artifact(&worker, mode.as_deref());
            Ok(WorkerResult {
                status: WorkerStatus::Ok,
                worker: worker.clone(),
                mode,
                artifacts: vec![Artifact {
                    kind: kind.to_string(),
                    r#ref: format!("stub-{}", worker),
                    count,
                }],
                reasoning: format!("stub {} completed without issues", worker),
                decisions: Vec::new(),
                metrics: Value::Null,
                error: None,
            })
        })
    }
}

fn canned_artifact(worker: &str, mode: Option<&str>) -> (&'static str, u64) {
    match worker {
        "knowledge_extractor" => ("knowledge_parts", 3),
        "relation_builder" => ("topics", 8),
        "trace_analyzer" => ("knowledge_parts", 2),
        "record_generator" => ("records", 40),
        "grader_drafter" => match mode {
            Some("init") => ("grader", 1),
            Some("finalize") => ("grader", 1),
            Some("refine") => ("grader", 1),
            _ => ("grader", 1),
        },
        "training_monitor" => ("monitor_report", 1),
        _ => ("unknown", 0),
    }
}

/// Convenience: build the default production client. For MVP this is the
/// stub; real code swaps this for the subprocess impl.
pub fn default_client() -> Box<dyn ClaudeClient> {
    Box::new(StubClaudeClient::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_returns_ok_for_known_worker() {
        let client = StubClaudeClient::new();
        let input = WorkerInput::new("knowledge_extractor", "sources", "wf", "/tmp");
        let result = client.run(input).await.unwrap();
        assert_eq!(result.status, WorkerStatus::Ok);
        assert_eq!(result.artifacts.len(), 1);
        assert_eq!(result.artifacts[0].kind, "knowledge_parts");
    }

    #[tokio::test]
    async fn grader_drafter_honors_mode() {
        let client = StubClaudeClient::new();
        let input =
            WorkerInput::new("grader_drafter", "plan", "wf", "/tmp").with_mode("init");
        let result = client.run(input).await.unwrap();
        assert_eq!(result.mode.as_deref(), Some("init"));
        assert_eq!(result.artifacts[0].kind, "grader");
    }

    #[tokio::test]
    async fn unknown_worker_returns_unknown_artifact() {
        let client = StubClaudeClient::new();
        let input = WorkerInput::new("does_not_exist", "sources", "wf", "/tmp");
        let result = client.run(input).await.unwrap();
        assert_eq!(result.artifacts[0].kind, "unknown");
        assert_eq!(result.artifacts[0].count, 0);
    }
}
