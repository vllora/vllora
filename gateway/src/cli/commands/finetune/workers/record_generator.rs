//! record_generator worker — one per leaf topic.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §6.5.4
//!
//! Invoked per leaf topic during `generate`. Reads topic-linked knowledge parts,
//! emits training records with numbered `source_parts` for traceability. Uses
//! only Claude (no OpenAI / teacher model). Prompt:
//! `finetune/src/prompts/record-generator.md`.

use super::claude_client::{ClaudeClient, Result, WorkerInput, WorkerResult};
use serde_json::json;

pub const WORKER_NAME: &str = "record_generator";
pub const PHASE: &str = "generate";

pub async fn run<C: ClaudeClient + ?Sized>(
    client: &C,
    workflow_id: &str,
    project_dir: &str,
    topic_slug: &str,
    target_count: u64,
) -> Result<WorkerResult> {
    let input = WorkerInput::new(WORKER_NAME, PHASE, workflow_id, project_dir).with_inputs(json!({
        "topic_slug": topic_slug,
        "target_count": target_count,
    }));
    client.run(input).await
}
