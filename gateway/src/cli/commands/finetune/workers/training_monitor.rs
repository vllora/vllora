//! training_monitor worker — long-running poller during `train`.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §6.5.6
//!
//! Spawned once per training job. Polls gateway metrics every ~30s, writes
//! `monitor-report-{N}.md`, detects anomalies (clipping spikes, reward
//! saturation, KL blow-up when β≠0). Auto-cancels the run on safety thresholds.
//! Prompt: `finetune/src/prompts/training-monitor.md`.

use super::claude_client::{ClaudeClient, Result, WorkerInput, WorkerResult};
use serde_json::json;

pub const WORKER_NAME: &str = "training_monitor";
pub const PHASE: &str = "train";

pub async fn run<C: ClaudeClient + ?Sized>(
    client: &C,
    workflow_id: &str,
    project_dir: &str,
    training_job_id: &str,
    iteration: u32,
) -> Result<WorkerResult> {
    let input = WorkerInput::new(WORKER_NAME, PHASE, workflow_id, project_dir).with_inputs(json!({
        "training_job_id": training_job_id,
        "iteration": iteration,
    }));
    client.run(input).await
}
