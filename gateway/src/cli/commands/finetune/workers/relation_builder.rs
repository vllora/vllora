//! relation_builder worker — builds topic hierarchy + topic-part relations.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §6.5.2
//!
//! Invoked once during `plan`. Reads knowledge parts from the gateway and
//! emits topics (Domain → Skill) + relations (max 15 parts per leaf).
//! Prompt: `finetune/src/prompts/relation-builder.md`.

use super::claude_client::{ClaudeClient, Result, WorkerInput, WorkerResult};
use serde_json::json;

pub const WORKER_NAME: &str = "relation_builder";
pub const PHASE: &str = "plan";

pub async fn run<C: ClaudeClient + ?Sized>(
    client: &C,
    workflow_id: &str,
    project_dir: &str,
    knowledge_parts_count: u64,
) -> Result<WorkerResult> {
    let input = WorkerInput::new(WORKER_NAME, PHASE, workflow_id, project_dir).with_inputs(json!({
        "knowledge_parts_count": knowledge_parts_count,
    }));
    client.run(input).await
}
