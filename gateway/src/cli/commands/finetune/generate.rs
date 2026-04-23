//! `vllora finetune generate` — record_generator × topics + grader_drafter(finalize)
//! + quality gate.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §5.4

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
use super::workers::{grader_drafter, record_generator};

#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Number of topics to generate for (MVP default 8).
    #[arg(long, default_value_t = 8)]
    pub topics: u32,
    /// Target records per topic (MVP default 5).
    #[arg(long, default_value_t = 5)]
    pub per_topic: u32,
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
    if !journal.is_phase_done("plan").map_err(to_cli)? {
        return precondition("plan is not done — run /finetune-plan first");
    }
    if journal.is_phase_done("generate").map_err(to_cli)? && !args.force {
        shared::emit(&shared::phase_done(
            "generate",
            "done",
            Some("/finetune-eval"),
            Some("generate already complete (--force to regenerate)"),
        ));
        return Ok(());
    }
    let workflow_id = read_workflow_id(&journal)?;
    let _guard = lock::acquire(project_dir).map_err(|e| cli(format!("lock: {}", e)))?;
    journal
        .write_step_start("generate", std::process::id())
        .map_err(to_cli)?;

    // Per-topic record_generator.
    let mut total_records = 0u64;
    for idx in 0..args.topics {
        let slug = format!("topic-{}", idx);
        super::shared::emit(&json!({
            "type": "worker_start",
            "worker": "record_generator",
            "target": slug,
            "emitted_at": chrono::Utc::now().to_rfc3339(),
        }));
        let res = record_generator::run(
            worker,
            &workflow_id.to_string(),
            &project_dir.display().to_string(),
            &slug,
            args.per_topic as u64,
        )
        .await
        .map_err(to_cli)?;
        let count = if res.status == WorkerStatus::Ok {
            res.artifacts.first().map(|a| a.count).unwrap_or(0)
        } else {
            0
        };
        super::shared::emit(&json!({
            "type": "worker_done",
            "worker": "record_generator",
            "target": slug,
            "status": if res.status == WorkerStatus::Ok { "ok" } else { "incomplete" },
            "artifacts_count": count,
            "emitted_at": chrono::Utc::now().to_rfc3339(),
        }));
        total_records += count;
    }
    let _ = gateway.upload_records(workflow_id, Vec::new()).await;

    // grader_drafter(finalize).
    super::shared::emit(&json!({
        "type": "worker_start",
        "worker": "grader_drafter",
        "mode": "finalize",
        "emitted_at": chrono::Utc::now().to_rfc3339(),
    }));
    let fin = grader_drafter::run(
        worker,
        grader_drafter::Mode::Finalize,
        &workflow_id.to_string(),
        &project_dir.display().to_string(),
        json!({ "record_count": total_records }),
    )
    .await
    .map_err(to_cli)?;
    let fin_ok = fin.status == WorkerStatus::Ok;
    super::shared::emit(&json!({
        "type": "worker_done",
        "worker": "grader_drafter",
        "mode": "finalize",
        "status": if fin_ok { "ok" } else { "incomplete" },
        "emitted_at": chrono::Utc::now().to_rfc3339(),
    }));
    let _ = gateway
        .upload_grader(workflow_id, "/* finalized grader */", "finalized by /finetune-generate")
        .await;

    // Simple quality gate: pass if we have at least 1 record + grader was ok.
    let quality_pass = total_records > 0 && fin_ok;
    let mut fields = BTreeMap::new();
    fields.insert("record_count".into(), json!(total_records));
    fields.insert("grader_version".into(), json!(2));
    fields.insert(
        "quality_gate".into(),
        json!(if quality_pass { "pass" } else { "fail" }),
    );
    journal.write_step_done("generate", fields).map_err(to_cli)?;
    if let Ok(doc) = journal.read() {
        let _ = gateway
            .put_pipeline_journal(workflow_id, &doc.to_string())
            .await;
    }

    shared::emit(&shared::phase_done(
        "generate",
        "done",
        Some(if quality_pass { "/finetune-eval" } else { "/finetune-plan" }),
        Some(&format!(
            "{} records; quality_gate: {}",
            total_records,
            if quality_pass { "pass" } else { "fail" }
        )),
    ));
    Ok(())
}

// ----- helpers (repeated across verbs; easy to lift into shared.rs later) -----

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
            "vllora-gen-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }
    fn init_to_plan_done(dir: &Path) -> Uuid {
        let wf = Uuid::new_v4();
        std::fs::create_dir_all(dir).unwrap();
        let j = FileJournal::open_or_create(dir, &wf.to_string()).unwrap();
        j.write_step_start("init", 1).unwrap();
        let mut f = BTreeMap::new();
        f.insert("workflow_id".into(), json!(wf.to_string()));
        j.write_step_done("init", f).unwrap();
        j.write_step_start("sources", 1).unwrap();
        j.write_step_done("sources", BTreeMap::new()).unwrap();
        j.write_step_start("plan", 1).unwrap();
        j.write_step_done("plan", BTreeMap::new()).unwrap();
        wf
    }

    #[tokio::test]
    async fn happy_path_runs_and_quality_gate_passes() {
        let dir = fresh();
        let wf = init_to_plan_done(&dir);
        let gateway = MockGatewayClient::new().with_workflow_id(wf);
        let worker = claude_client::StubClaudeClient::new();
        handle_inner(
            &gateway,
            &worker,
            &dir,
            Args {
                topics: 3,
                per_topic: 5,
                force: false,
            },
        )
        .await
        .unwrap();
        let j = FileJournal::open_or_create(&dir, "unused").unwrap();
        let doc = j.read().unwrap();
        assert_eq!(
            doc["phases"]["generate"]["fields"]["quality_gate"].as_str(),
            Some("pass")
        );
        assert!(gateway
            .calls()
            .iter()
            .any(|c| matches!(c, MockCall::UploadRecords { .. })));
        assert!(gateway
            .calls()
            .iter()
            .any(|c| matches!(c, MockCall::UploadGrader { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn plan_not_done_blocks() {
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
                topics: 1,
                per_topic: 1,
                force: false,
            },
        )
        .await
        .unwrap_err();
        assert!(format!("{}", err).contains("precondition"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
