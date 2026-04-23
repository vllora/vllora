//! `vllora finetune train` — submit GRPO + spawn training_monitor.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §5.6

use std::collections::BTreeMap;
use std::path::Path;

use clap::Parser;
use serde_json::json;
use uuid::Uuid;
use vllora_core::metadata::pool::DbPool;
use vllora_finetune::gateway_client::GatewayClient;
use vllora_finetune::state::journal::FileJournal;
use vllora_finetune::state::{lock, Journal};

use super::shared;
use super::workers::claude_client::{self, ClaudeClient, WorkerStatus};
use super::workers::training_monitor;

#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Base model to fine-tune (default qwen-3.5-4b).
    #[arg(long, default_value = "qwen-3.5-4b")]
    pub model: String,
    /// Optional YAML config path (hyperparameter overrides).
    #[arg(long)]
    pub config: Option<std::path::PathBuf>,
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
    if !journal.is_phase_done("eval").map_err(to_cli)? {
        return precondition("eval is not done — run /finetune-eval first");
    }
    let doc = journal.read().map_err(to_cli)?;
    let readiness = doc["phases"]["eval"]["fields"]["readiness"]
        .as_str()
        .unwrap_or("fail");
    if readiness != "pass" {
        return precondition(&format!(
            "eval readiness={} — training requires readiness=pass",
            readiness
        ));
    }
    if journal.is_phase_done("train").map_err(to_cli)? && !args.force {
        shared::emit(&shared::phase_done(
            "train",
            "done",
            None,
            Some("train already complete (--force to re-run)"),
        ));
        return Ok(());
    }

    let workflow_id = read_workflow_id(&journal)?;
    let _guard = lock::acquire(project_dir).map_err(|e| cli(format!("lock: {}", e)))?;
    journal
        .write_step_start("train", std::process::id())
        .map_err(to_cli)?;

    // Parse optional config.
    let config = if let Some(p) = &args.config {
        match std::fs::read_to_string(p) {
            Ok(yaml) => match serde_yaml::from_str::<serde_json::Value>(&yaml) {
                Ok(v) => v,
                Err(_) => json!({}),
            },
            Err(_) => json!({}),
        }
    } else {
        json!({})
    };

    shared::emit(&shared::progress("train", "submitting training job", None));
    let grader_version = doc["phases"]["eval"]["fields"]["grader_version"]
        .as_i64()
        .unwrap_or(1) as i32;
    let job_id = gateway
        .create_training_job(workflow_id, &args.model, grader_version, config)
        .await
        .map_err(to_cli)?;

    // Spawn training_monitor (one iteration for MVP — real impl loops).
    shared::emit(&json!({
        "type": "worker_start",
        "worker": "training_monitor",
        "target": job_id.to_string(),
        "emitted_at": chrono::Utc::now().to_rfc3339(),
    }));
    let monitor_res = training_monitor::run(
        worker,
        &workflow_id.to_string(),
        &project_dir.display().to_string(),
        &job_id.to_string(),
        1,
    )
    .await
    .map_err(to_cli)?;
    let monitor_ok = monitor_res.status == WorkerStatus::Ok;
    shared::emit(&json!({
        "type": "worker_done",
        "worker": "training_monitor",
        "status": if monitor_ok { "ok" } else { "incomplete" },
        "target": job_id.to_string(),
        "emitted_at": chrono::Utc::now().to_rfc3339(),
    }));

    // Write a placeholder monitor-report-1.md so downstream consumers (plugin, UI) can Read it.
    let report_path = project_dir.join("monitor-report-1.md");
    let _ = std::fs::write(
        &report_path,
        format!("# Training Monitor Report — round 1\n\nJob: {}\nStatus: {}\n",
            job_id,
            if monitor_ok { "ok" } else { "incomplete" }
        ),
    );

    // Derive an adapter ID; real impl reads it from gateway.
    let adapter_id = format!("adapter-{}", job_id);

    let mut fields = BTreeMap::new();
    fields.insert("training_job_id".into(), json!(job_id.to_string()));
    fields.insert("adapter_id".into(), json!(adapter_id));
    fields.insert("model".into(), json!(args.model));
    journal.write_step_done("train", fields).map_err(to_cli)?;
    if let Ok(doc) = journal.read() {
        let _ = gateway
            .put_pipeline_journal(workflow_id, &doc.to_string())
            .await;
    }

    shared::emit(&shared::phase_done(
        "train",
        "done",
        None,
        Some(&format!("adapter: adapter-{}", job_id)),
    ));
    Ok(())
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
            "vllora-train-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }
    fn init_to_eval_pass(dir: &Path) -> Uuid {
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
        j.write_step_start("eval", 1).unwrap();
        let mut ef = BTreeMap::new();
        ef.insert("readiness".into(), json!("pass"));
        ef.insert("grader_version".into(), json!(1));
        j.write_step_done("eval", ef).unwrap();
        wf
    }

    #[tokio::test]
    async fn happy_path_returns_adapter_id() {
        let dir = fresh();
        let wf = init_to_eval_pass(&dir);
        let gateway = MockGatewayClient::new().with_workflow_id(wf);
        let worker = claude_client::StubClaudeClient::new();
        handle_inner(
            &gateway,
            &worker,
            &dir,
            Args {
                model: "qwen-3.5-4b".into(),
                config: None,
                force: false,
            },
        )
        .await
        .unwrap();
        let j = FileJournal::open_or_create(&dir, "unused").unwrap();
        let doc = j.read().unwrap();
        assert!(doc["phases"]["train"]["fields"]["adapter_id"]
            .as_str()
            .unwrap()
            .starts_with("adapter-"));
        assert!(gateway
            .calls()
            .iter()
            .any(|c| matches!(c, MockCall::CreateTrainingJob { .. })));
        assert!(dir.join("monitor-report-1.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn readiness_fail_blocks() {
        let dir = fresh();
        let wf = Uuid::new_v4();
        std::fs::create_dir_all(&dir).unwrap();
        let j = FileJournal::open_or_create(&dir, &wf.to_string()).unwrap();
        j.write_step_start("init", 1).unwrap();
        let mut f = BTreeMap::new();
        f.insert("workflow_id".into(), json!(wf.to_string()));
        j.write_step_done("init", f).unwrap();
        for phase in ["sources", "plan", "generate"] {
            j.write_step_start(phase, 1).unwrap();
            j.write_step_done(phase, BTreeMap::new()).unwrap();
        }
        j.write_step_start("eval", 1).unwrap();
        let mut ef = BTreeMap::new();
        ef.insert("readiness".into(), json!("fail"));
        j.write_step_done("eval", ef).unwrap();

        let gateway = MockGatewayClient::new();
        let worker = claude_client::StubClaudeClient::new();
        let err = handle_inner(
            &gateway,
            &worker,
            &dir,
            Args {
                model: "qwen-3.5-4b".into(),
                config: None,
                force: false,
            },
        )
        .await
        .unwrap_err();
        assert!(format!("{}", err).contains("readiness=pass"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
