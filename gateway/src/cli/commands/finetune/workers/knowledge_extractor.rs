//! knowledge_extractor worker — one per PDF (parallelizable).
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §6.5.1
//!
//! Spawned per local path by the `sources` verb. Reads a PDF / trace bundle,
//! produces `knowledge_parts` artifacts tagged with `origin_uri`. Prompt lives
//! in `finetune/src/prompts/knowledge-extractor.md`.

use super::claude_client::{ClaudeClient, Result, WorkerInput, WorkerResult};
use serde_json::json;

pub const WORKER_NAME: &str = "knowledge_extractor";
pub const PHASE: &str = "sources";

pub async fn run<C: ClaudeClient + ?Sized>(
    client: &C,
    workflow_id: &str,
    project_dir: &str,
    origin_uri: &str,
    local_path: &str,
) -> Result<WorkerResult> {
    let input = WorkerInput::new(WORKER_NAME, PHASE, workflow_id, project_dir).with_inputs(json!({
        "origin_uri": origin_uri,
        "local_path": local_path,
    }));
    client.run(input).await
}
