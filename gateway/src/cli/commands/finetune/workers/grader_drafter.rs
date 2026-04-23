//! grader_drafter worker — three modes: init / finalize / refine.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §6.5.5 + §5.9
//!
//! - `init` (during `plan`): first draft from topic spec + sample records.
//! - `finalize` (during `generate`): lock in after records settle.
//! - `refine` (during `eval` when `readiness=fail` w/ grader root cause):
//!   read failure signals, produce a fixed grader version + `change-log.md` entry.
//!
//! Prompts: `finetune/src/prompts/grader-drafter-{init,finalize,refine}.md`.

use super::claude_client::{ClaudeClient, Result, WorkerInput, WorkerResult};
use serde_json::json;

pub const WORKER_NAME: &str = "grader_drafter";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Init,
    Finalize,
    Refine,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Init => "init",
            Mode::Finalize => "finalize",
            Mode::Refine => "refine",
        }
    }

    pub fn phase(&self) -> &'static str {
        match self {
            Mode::Init => "plan",
            Mode::Finalize => "generate",
            Mode::Refine => "eval",
        }
    }
}

pub async fn run<C: ClaudeClient + ?Sized>(
    client: &C,
    mode: Mode,
    workflow_id: &str,
    project_dir: &str,
    context: serde_json::Value,
) -> Result<WorkerResult> {
    let input = WorkerInput::new(WORKER_NAME, mode.phase(), workflow_id, project_dir)
        .with_mode(mode.as_str())
        .with_inputs(json!({ "context": context }));
    client.run(input).await
}
