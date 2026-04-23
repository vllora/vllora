//! trace_analyzer worker — OTel trace → topics/prompts/grader hints.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §6.5.3
//!
//! Alternative to knowledge_extractor when the corpus is OTel trace bundles
//! rather than PDFs. Emits knowledge-part artifacts with trace-derived
//! semantics. Prompt: `finetune/src/prompts/trace-analyzer.md`.

use super::claude_client::{ClaudeClient, Result, WorkerInput, WorkerResult};
use serde_json::json;

pub const WORKER_NAME: &str = "trace_analyzer";
pub const PHASE: &str = "sources";

pub async fn run<C: ClaudeClient + ?Sized>(
    client: &C,
    workflow_id: &str,
    project_dir: &str,
    trace_bundle_path: &str,
) -> Result<WorkerResult> {
    let input = WorkerInput::new(WORKER_NAME, PHASE, workflow_id, project_dir).with_inputs(json!({
        "trace_bundle_path": trace_bundle_path,
    }));
    client.run(input).await
}
