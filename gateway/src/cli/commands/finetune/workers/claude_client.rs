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

/// Convenience: build the default production client.
///
/// Selection rule:
///   - `VLLORA_WORKER_MODE=real|live` → `SubprocessClaudeClient` (spawns
///     `claude -p` subprocesses, parses stream-JSON).
///   - Otherwise → `StubClaudeClient` (canned `ok` results, deterministic,
///     no process spawning — safe default for CI + tests).
pub fn default_client() -> Box<dyn ClaudeClient> {
    let mode = std::env::var("VLLORA_WORKER_MODE")
        .unwrap_or_default()
        .to_lowercase();
    if matches!(mode.as_str(), "real" | "live") {
        Box::new(SubprocessClaudeClient::new())
    } else {
        Box::new(StubClaudeClient::new())
    }
}

// ---------------------------------------------------------------------------
// SubprocessClaudeClient — real `claude -p` spawner.
// ---------------------------------------------------------------------------
//
// Spawns `claude -p --output-format stream-json --verbose --max-turns N`
// with a system prompt loaded from `finetune/src/prompts/<worker>.md` and
// the `WorkerInput` JSON fed on stdin. Parses stream-JSON events line-by-line
// from stdout and collects them into a typed `WorkerResult`.
//
// Auth: inherits the parent process environment — claude -p reads its own
// credentials from `claude login` or `ANTHROPIC_API_KEY`. We never read
// credential files here (parent §2.10.1).

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const DEFAULT_MAX_TURNS: u32 = 15;
const DEFAULT_TIMEOUT_SECS: u64 = 600;

pub struct SubprocessClaudeClient {
    claude_bin: String,
    prompts_dir: std::path::PathBuf,
}

impl SubprocessClaudeClient {
    pub fn new() -> Self {
        let claude_bin = std::env::var("VLLORA_CLAUDE_BIN").unwrap_or_else(|_| "claude".into());
        let prompts_dir = std::env::var("VLLORA_PROMPTS_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                // Cargo dev layout: <exe>/../../finetune/src/prompts
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .map(|p| p.join("finetune").join("src").join("prompts"))
                    .unwrap_or_else(|| std::path::PathBuf::from("./finetune/src/prompts"))
            });
        Self {
            claude_bin,
            prompts_dir,
        }
    }

    fn prompt_path(&self, worker: &str, mode: Option<&str>) -> std::path::PathBuf {
        let filename = match (worker, mode) {
            ("grader_drafter", Some(m)) => format!("grader-drafter-{}.md", m),
            (w, _) => format!("{}.md", w.replace('_', "-")),
        };
        self.prompts_dir.join(filename)
    }
}

impl Default for SubprocessClaudeClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeClient for SubprocessClaudeClient {
    fn run<'a>(&'a self, input: WorkerInput) -> BoxFuture<'a, Result<WorkerResult>> {
        Box::pin(async move {
            let system_prompt = std::fs::read_to_string(
                self.prompt_path(&input.worker, input.mode.as_deref()),
            )
            .map_err(|e| {
                Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "loading prompt for '{}' (mode={:?}): {}",
                    input.worker, input.mode, e
                ))
            })?;

            let max_turns = input
                .limits
                .get("max_turns")
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_MAX_TURNS as u64);
            let timeout_secs = input
                .limits
                .get("timeout_seconds")
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_TIMEOUT_SECS);

            let input_json = serde_json::to_string(&input)?;

            let mut child = Command::new(&self.claude_bin)
                .args([
                    "-p",
                    "--output-format",
                    "stream-json",
                    "--verbose",
                    "--max-turns",
                    &max_turns.to_string(),
                    "--append-system-prompt",
                    &system_prompt,
                ])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| {
                    Box::<dyn std::error::Error + Send + Sync>::from(format!(
                        "spawn `{}`: {} (set VLLORA_CLAUDE_BIN or install claude-code)",
                        self.claude_bin, e
                    ))
                })?;

            // Feed the WorkerInput JSON on stdin, then close stdin so claude
            // processes + exits. If stdin write fails, keep going — claude
            // may still produce output from what it did read.
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(input_json.as_bytes()).await;
                let _ = stdin.shutdown().await;
            }

            let stdout = child.stdout.take().ok_or_else(|| {
                Box::<dyn std::error::Error + Send + Sync>::from("claude -p has no stdout")
            })?;

            let read_future = async move {
                let mut reader = BufReader::new(stdout).lines();
                let mut final_event: Option<serde_json::Value> = None;
                while let Ok(Some(line)) = reader.next_line().await {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                        if v.get("type").and_then(|t| t.as_str()) == Some("result") {
                            final_event = Some(v);
                            continue;
                        }
                        // Pass-through other events so the verb's stream-JSON
                        // remains coherent for Feature 004 plugin narrators.
                        let stdout = std::io::stdout();
                        let mut out = stdout.lock();
                        use std::io::Write as _;
                        let _ = writeln!(&mut out, "{}", trimmed);
                        let _ = out.flush();
                    }
                }
                let exit_status = child.wait().await;
                (final_event, exit_status)
            };

            let (final_event, exit_status) =
                match timeout(Duration::from_secs(timeout_secs), read_future).await {
                    Ok(v) => v,
                    Err(_) => {
                        return Ok(WorkerResult {
                            status: WorkerStatus::Incomplete,
                            worker: input.worker,
                            mode: input.mode,
                            artifacts: vec![],
                            reasoning: format!("claude -p timed out after {}s", timeout_secs),
                            decisions: vec![],
                            metrics: Value::Null,
                            error: Some(WorkerError {
                                code: "TIMEOUT".into(),
                                message: format!("timed out after {}s", timeout_secs),
                            }),
                        });
                    }
                };

            let exit_ok = exit_status.as_ref().map(|s| s.success()).unwrap_or(false);

            if let Some(ev) = final_event {
                let result_text = ev
                    .get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // If the worker returned a JSON object conforming to our
                // WorkerResult shape, prefer that over the plain text.
                if let Ok(parsed) = serde_json::from_str::<WorkerResult>(&result_text) {
                    return Ok(parsed);
                }

                let subtype = ev.get("subtype").and_then(|v| v.as_str()).unwrap_or("");
                let status = if subtype == "success" && exit_ok {
                    WorkerStatus::Ok
                } else {
                    WorkerStatus::Incomplete
                };
                return Ok(WorkerResult {
                    status,
                    worker: input.worker,
                    mode: input.mode,
                    artifacts: vec![],
                    reasoning: result_text,
                    decisions: vec![],
                    metrics: ev.get("usage").cloned().unwrap_or(Value::Null),
                    error: None,
                });
            }

            Ok(WorkerResult {
                status: WorkerStatus::Error,
                worker: input.worker,
                mode: input.mode,
                artifacts: vec![],
                reasoning: String::new(),
                decisions: vec![],
                metrics: Value::Null,
                error: Some(WorkerError {
                    code: "NO_TERMINAL_EVENT".into(),
                    message: "claude -p produced no `result` event".into(),
                }),
            })
        })
    }
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
