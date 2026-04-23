//! `vllora finetune eval` — readiness-gate eval loop.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §5.5
//!
//! For each iteration (up to --max-iterations):
//!   - Create eval run on gateway
//!   - Poll until terminal
//!   - If readiness=fail with grader-root-cause: spawn grader_drafter(refine),
//!     upload new grader, loop
//!   - If readiness=pass: exit
//!
//! MVP: uses the mock gateway's canned `poll_eval_run` → `readiness_score: 0.82`
//! which we treat as pass on the first iteration. Real impl interprets actual
//! eval metrics + grader signals.

use std::collections::BTreeMap;
use std::path::Path;

use clap::Parser;
use serde_json::json;
use uuid::Uuid;
use vllora_core::metadata::pool::DbPool;
use vllora_finetune::gateway_client::GatewayClient;
use vllora_finetune::state::change_log::FileChangeLog;
use vllora_finetune::state::journal::FileJournal;
use vllora_finetune::state::{lock, ChangeLog, Journal};

use super::shared;
use super::workers::claude_client::{self, ClaudeClient, WorkerStatus};
use super::workers::grader_drafter;

#[derive(Parser, Debug, Clone)]
pub struct Args {
    #[arg(long, default_value_t = 5)]
    pub max_iterations: u32,
    /// Base model to eval on. Default: 4B (tool-calling).
    #[arg(long, default_value = "qwen-3.5-4b")]
    pub model: String,
    #[arg(long)]
    pub force: bool,
}

pub async fn handle(_db_pool: DbPool, args: Args) -> Result<(), crate::CliError> {
    let gateway = shared::make_gateway_client();
    let worker = claude_client::default_client();
    let project_dir = shared::project_dir()?;
    handle_inner(&*gateway, &*worker, &project_dir, args).await
}

pub async fn handle_inner<G: GatewayClient + ?Sized, W: ClaudeClient + ?Sized>(
    gateway: &G,
    worker: &W,
    project_dir: &Path,
    args: Args,
) -> Result<(), crate::CliError> {
    let journal = shared_open_journal(project_dir)?;

    // Records must exist (import-dataset OR generate with quality_gate pass).
    let have_records = is_done(&journal, "import-dataset")? || is_done(&journal, "generate")?;
    if !have_records {
        return precondition(
            "no records — run /finetune-generate or /finetune-import-dataset first",
        );
    }
    if journal.is_phase_done("eval").map_err(to_cli)? && !args.force {
        shared::emit(&shared::phase_done(
            "eval",
            "done",
            Some("/finetune-train"),
            Some("eval already complete (--force to re-run)"),
        ));
        return Ok(());
    }

    let workflow_id = read_workflow_id(&journal)?;
    let _guard = lock::acquire(project_dir).map_err(|e| cli(format!("lock: {}", e)))?;
    journal
        .write_step_start("eval", std::process::id())
        .map_err(to_cli)?;

    let mut readiness = "fail".to_string();
    let mut iteration = 0u32;
    let mut grader_version = 1i32;

    while iteration < args.max_iterations {
        iteration += 1;
        // Create eval run.
        let run_id = gateway
            .create_eval_run(workflow_id, &args.model, grader_version)
            .await
            .map_err(to_cli)?;
        // Poll (MVP: single call; mock returns terminal immediately).
        let status = gateway.poll_eval_run(run_id).await.map_err(to_cli)?;
        let score = status.readiness_score.unwrap_or(0.0);
        let outcome = if score >= 0.75 { "pass" } else { "fail" };

        shared::emit(&json!({
            "type": "worker_iteration",
            "phase": "eval",
            "iteration": iteration,
            "outcome": outcome,
            "metrics": {
                "readiness_score": score,
                "avg_score": status.avg_score.unwrap_or(0.0),
            },
            "emitted_at": chrono::Utc::now().to_rfc3339(),
        }));

        // Record iteration in journal.
        let mut f = BTreeMap::new();
        f.insert("readiness_score".into(), json!(score));
        f.insert("outcome".into(), json!(outcome));
        let _ = journal.write_step_iteration("eval", iteration, f);

        if outcome == "pass" {
            readiness = "pass".into();
            break;
        }

        // Refine grader on fail.
        let refine = grader_drafter::run(
            worker,
            grader_drafter::Mode::Refine,
            &workflow_id.to_string(),
            &project_dir.display().to_string(),
            json!({ "iteration": iteration, "prior_score": score }),
        )
        .await
        .map_err(to_cli)?;
        if refine.status == WorkerStatus::Ok {
            let reason = format!("refined in /finetune-eval iteration {}", iteration);
            let new_version = gateway
                .upload_grader(workflow_id, "/* refined grader */", &reason)
                .await
                .map_err(to_cli)?;
            grader_version = new_version;
            // Record the refinement in change-log.md for auditability.
            let change_log = FileChangeLog::open_or_create(project_dir).map_err(to_cli)?;
            let _ = change_log.append(
                "agent:grader_drafter:refine",
                &format!(
                    "Refined grader after readiness=fail (score={:.2}, iteration={})",
                    score, iteration
                ),
                "",
            );
        }
    }

    let mut fields = BTreeMap::new();
    fields.insert("readiness".into(), json!(readiness));
    fields.insert("iteration".into(), json!(iteration));
    fields.insert("grader_version".into(), json!(grader_version));
    journal.write_step_done("eval", fields).map_err(to_cli)?;
    if let Ok(doc) = journal.read() {
        let _ = gateway
            .put_pipeline_journal(workflow_id, &doc.to_string())
            .await;
    }

    let (next, summary) = if readiness == "pass" {
        (
            Some("/finetune-train"),
            format!("readiness=pass at iteration {}", iteration),
        )
    } else {
        (
            Some("/finetune-plan"),
            format!("readiness=fail after {} iteration(s) — revisit plan/generate", iteration),
        )
    };
    shared::emit(&shared::phase_done("eval", "done", next, Some(&summary)));
    Ok(())
}

fn is_done(j: &FileJournal, phase: &str) -> Result<bool, crate::CliError> {
    j.is_phase_done(phase).map_err(to_cli)
}

fn shared_open_journal(project_dir: &Path) -> Result<FileJournal, crate::CliError> {
    if !project_dir.join("pipeline-journal.json").is_file() {
        shared::emit(&shared::error_event(
            "PRECONDITION_UNMET",
            "finetune-project/ does not exist — run /finetune-init first",
            None,
        ));
        return Err(cli("precondition unmet: finetune-project/ missing"));
    }
    FileJournal::open_or_create(project_dir, "unused")
        .map_err(|e| cli(format!("open journal: {}", e)))
}
fn read_workflow_id(j: &FileJournal) -> Result<Uuid, crate::CliError> {
    let doc = j.read().map_err(to_cli)?;
    let s = doc
        .get("workflow_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| cli("journal missing workflow_id"))?;
    Uuid::parse_str(s).map_err(|e| cli(format!("invalid workflow_id: {}", e)))
}
fn precondition(msg: &str) -> Result<(), crate::CliError> {
    shared::emit(&shared::error_event("PRECONDITION_UNMET", msg, None));
    Err(cli(format!("precondition unmet: {}", msg)))
}
fn to_cli<E: std::fmt::Display>(e: E) -> crate::CliError {
    crate::CliError::CustomError(format!("{}", e))
}
fn cli<S: Into<String>>(s: S) -> crate::CliError {
    crate::CliError::CustomError(s.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vllora_finetune::gateway_client::{MockCall, MockGatewayClient};

    fn fresh() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vllora-eval-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }
    fn init_to_generate(dir: &Path) -> Uuid {
        let wf = Uuid::new_v4();
        std::fs::create_dir_all(dir).unwrap();
        let j = FileJournal::open_or_create(dir, &wf.to_string()).unwrap();
        j.write_step_start("init", 1).unwrap();
        let mut f = BTreeMap::new();
        f.insert("workflow_id".into(), json!(wf.to_string()));
        j.write_step_done("init", f).unwrap();
        for phase in ["sources", "plan", "generate"] {
            j.write_step_start(phase, 1).unwrap();
            j.write_step_done(phase, BTreeMap::new()).unwrap();
        }
        wf
    }

    #[tokio::test]
    async fn readiness_passes_on_first_iteration() {
        let dir = fresh();
        let wf = init_to_generate(&dir);
        let gateway = MockGatewayClient::new().with_workflow_id(wf);
        let worker = claude_client::StubClaudeClient::new();
        handle_inner(
            &gateway,
            &worker,
            &dir,
            Args {
                max_iterations: 5,
                model: "qwen-3.5-4b".into(),
                force: false,
            },
        )
        .await
        .unwrap();
        let j = FileJournal::open_or_create(&dir, "unused").unwrap();
        let doc = j.read().unwrap();
        assert_eq!(
            doc["phases"]["eval"]["fields"]["readiness"].as_str(),
            Some("pass")
        );
        // Gateway saw create_eval_run.
        assert!(gateway
            .calls()
            .iter()
            .any(|c| matches!(c, MockCall::CreateEvalRun { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn no_records_blocks() {
        let dir = fresh();
        let wf = Uuid::new_v4();
        std::fs::create_dir_all(&dir).unwrap();
        let j = FileJournal::open_or_create(&dir, &wf.to_string()).unwrap();
        j.write_step_start("init", 1).unwrap();
        let mut f = BTreeMap::new();
        f.insert("workflow_id".into(), json!(wf.to_string()));
        j.write_step_done("init", f).unwrap();
        let gateway = MockGatewayClient::new();
        let worker = claude_client::StubClaudeClient::new();
        let err = handle_inner(
            &gateway,
            &worker,
            &dir,
            Args {
                max_iterations: 1,
                model: "qwen-3.5-4b".into(),
                force: false,
            },
        )
        .await
        .unwrap_err();
        assert!(format!("{}", err).contains("no records"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
