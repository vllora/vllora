//! `vllora finetune plan` — build topic hierarchy + initial grader draft.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §5.3
//! Contract: specs/003-cli-pipeline-verbs/contracts/verb-contract.md#plan

use std::collections::BTreeMap;
use std::path::Path;

use clap::Parser;
use serde_json::json;
use uuid::Uuid;
use vllora_core::metadata::pool::DbPool;
use vllora_finetune::gateway_client::GatewayClient;
use vllora_finetune::state::analysis::FileAnalysis;
use vllora_finetune::state::change_log::FileChangeLog;
use vllora_finetune::state::journal::FileJournal;
use vllora_finetune::state::{lock, Analysis, ChangeLog, Journal};

use super::shared;
use super::workers::claude_client::{self, ClaudeClient, WorkerStatus};
use super::workers::{grader_drafter, relation_builder};

#[derive(Parser, Debug, Clone)]
pub struct Args {
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
    let journal = open_journal(project_dir)?;
    if !journal.is_phase_done("sources").map_err(to_cli)? {
        return precondition("sources is not done — run /finetune-sources first");
    }
    if journal.is_phase_done("plan").map_err(to_cli)? && !args.force {
        shared::emit(&shared::phase_done(
            "plan",
            "done",
            Some("/finetune-generate"),
            Some("plan already complete (re-run with --force to regenerate)"),
        ));
        return Ok(());
    }

    let workflow_id = read_workflow_id(&journal)?;
    let _guard = lock::acquire(project_dir).map_err(|e| to_cli_msg(format!("lock: {}", e)))?;

    journal
        .write_step_start("plan", std::process::id())
        .map_err(to_cli)?;

    // Step 1: relation_builder.
    worker_start("relation_builder", None);
    let rel = relation_builder::run(
        worker,
        &workflow_id.to_string(),
        &project_dir.display().to_string(),
        /* knowledge_parts_count */ 0,
    )
    .await
    .map_err(to_cli)?;
    if rel.status != WorkerStatus::Ok {
        return fail(journal, "plan", "relation_builder did not complete");
    }
    let topic_count = rel.artifacts.first().map(|a| a.count).unwrap_or(0);
    worker_done("relation_builder", None, "ok", topic_count);
    let _ = gateway.upload_topics(workflow_id, Vec::new()).await;

    // Step 2: grader_drafter(init).
    worker_start("grader_drafter", Some("init"));
    let draft = grader_drafter::run(
        worker,
        grader_drafter::Mode::Init,
        &workflow_id.to_string(),
        &project_dir.display().to_string(),
        json!({ "topic_count": topic_count }),
    )
    .await
    .map_err(to_cli)?;
    if draft.status != WorkerStatus::Ok {
        return fail(journal, "plan", "grader_drafter(init) did not complete");
    }
    worker_done(
        "grader_drafter",
        Some("init"),
        "ok",
        draft.artifacts.first().map(|a| a.count).unwrap_or(0),
    );
    let _ = gateway
        .upload_grader(workflow_id, "/* draft grader */", "initial draft from /finetune-plan")
        .await;

    // Record the grader creation in change-log.md (audit trail).
    if let Ok(change_log) = FileChangeLog::open_or_create(project_dir) {
        let _ = change_log.append(
            "agent:grader_drafter:init",
            &format!("Initial grader draft for {} topics", topic_count),
            "",
        );
    }

    // Append plan-phase reasoning to analysis.json. Downstream phases consult
    // this before deciding what to do.
    if let Ok(analysis) =
        FileAnalysis::open_or_create(project_dir, &workflow_id.to_string())
    {
        let _ = analysis.append_phase(
            "plan",
            json!({
                "reasoning": format!(
                    "Topic hierarchy drafted: {} leaf topic(s). Grader v1 seeded.",
                    topic_count
                ),
                "decisions": [{
                    "label": "topic_count",
                    "choice": topic_count,
                    "rationale": "relation_builder output — one topic per distinct skill",
                }],
                "artifacts": ["plan.md", "change-log.md"],
            }),
        );
    }

    // Step 3: write a minimal plan.md so /finetune-generate can read it.
    let plan_md_path = project_dir.join("plan.md");
    let plan_md = format!(
        "# Plan\n\nWorkflow: {}\nTopics proposed: {}\nGrader draft: initial\n",
        workflow_id, topic_count
    );
    std::fs::write(&plan_md_path, plan_md)
        .map_err(|e| to_cli_msg(format!("write plan.md: {}", e)))?;

    let mut fields = BTreeMap::new();
    fields.insert("topic_count".into(), json!(topic_count));
    fields.insert("grader_version".into(), json!(1));
    journal.write_step_done("plan", fields).map_err(to_cli)?;
    if let Ok(doc) = journal.read() {
        let _ = gateway
            .put_pipeline_journal(workflow_id, &doc.to_string())
            .await;
    }

    shared::emit(&shared::phase_done(
        "plan",
        "done",
        Some("/finetune-generate"),
        Some(&format!(
            "{} topics, grader v1 drafted, plan.md written",
            topic_count
        )),
    ));
    Ok(())
}

// ----- helpers -----

fn open_journal(project_dir: &Path) -> Result<FileJournal, crate::CliError> {
    if !project_dir.join("pipeline-journal.json").is_file() {
        shared::emit(&shared::error_event(
            "PRECONDITION_UNMET",
            "finetune-project/ does not exist — run /finetune-init first",
            None,
        ));
        return Err(to_cli_msg("precondition unmet: finetune-project/ missing"));
    }
    FileJournal::open_or_create(project_dir, "unused")
        .map_err(|e| to_cli_msg(format!("open journal: {}", e)))
}

fn read_workflow_id(journal: &FileJournal) -> Result<Uuid, crate::CliError> {
    let doc = journal.read().map_err(to_cli)?;
    let s = doc
        .get("workflow_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| to_cli_msg("journal missing workflow_id"))?;
    Uuid::parse_str(s).map_err(|e| to_cli_msg(format!("invalid workflow_id: {}", e)))
}

fn precondition(msg: &str) -> Result<(), crate::CliError> {
    shared::emit(&shared::error_event("PRECONDITION_UNMET", msg, None));
    Err(to_cli_msg(format!("precondition unmet: {}", msg)))
}

fn fail(journal: FileJournal, phase: &str, msg: &str) -> Result<(), crate::CliError> {
    let _ = journal.write_step_failed(phase, msg);
    shared::emit(&shared::error_event("WORKER_UNRESPONSIVE", msg, None));
    Err(to_cli_msg(msg.to_string()))
}

fn worker_start(worker: &str, mode: Option<&str>) {
    let mut v = json!({
        "type": "worker_start",
        "worker": worker,
        "emitted_at": chrono::Utc::now().to_rfc3339(),
    });
    if let Some(m) = mode {
        v["mode"] = json!(m);
    }
    shared::emit(&v);
}

fn worker_done(worker: &str, mode: Option<&str>, status: &str, artifacts_count: u64) {
    let mut v = json!({
        "type": "worker_done",
        "worker": worker,
        "status": status,
        "artifacts_count": artifacts_count,
        "emitted_at": chrono::Utc::now().to_rfc3339(),
    });
    if let Some(m) = mode {
        v["mode"] = json!(m);
    }
    shared::emit(&v);
}

fn to_cli<E: std::fmt::Display>(e: E) -> crate::CliError {
    crate::CliError::CustomError(format!("{}", e))
}
fn to_cli_msg<S: Into<String>>(s: S) -> crate::CliError {
    crate::CliError::CustomError(s.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vllora_finetune::gateway_client::{MockCall, MockGatewayClient};

    fn fresh_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vllora-plan-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }

    fn init_to_sources_done(dir: &Path) -> Uuid {
        let wf = Uuid::new_v4();
        std::fs::create_dir_all(dir).unwrap();
        let j = FileJournal::open_or_create(dir, &wf.to_string()).unwrap();
        j.write_step_start("init", 1).unwrap();
        let mut f = BTreeMap::new();
        f.insert("workflow_id".into(), json!(wf.to_string()));
        j.write_step_done("init", f).unwrap();
        j.write_step_start("sources", 1).unwrap();
        j.write_step_done("sources", BTreeMap::new()).unwrap();
        wf
    }

    #[tokio::test]
    async fn happy_path_writes_plan_md() {
        let dir = fresh_dir();
        let wf = init_to_sources_done(&dir);
        let gateway = MockGatewayClient::new().with_workflow_id(wf);
        let worker = claude_client::StubClaudeClient::new();

        handle_inner(&gateway, &worker, &dir, Args { force: false })
            .await
            .unwrap();

        assert!(dir.join("plan.md").exists());
        let j = FileJournal::open_or_create(&dir, "unused").unwrap();
        assert!(j.is_phase_done("plan").unwrap());
        // Gateway saw upload_topics + upload_grader.
        assert!(gateway
            .calls()
            .iter()
            .any(|c| matches!(c, MockCall::UploadTopics { .. })));
        assert!(gateway
            .calls()
            .iter()
            .any(|c| matches!(c, MockCall::UploadGrader { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn sources_not_done_blocks() {
        let dir = fresh_dir();
        // init done but sources NOT done.
        let wf = Uuid::new_v4();
        std::fs::create_dir_all(&dir).unwrap();
        let j = FileJournal::open_or_create(&dir, &wf.to_string()).unwrap();
        j.write_step_start("init", 1).unwrap();
        let mut f = BTreeMap::new();
        f.insert("workflow_id".into(), json!(wf.to_string()));
        j.write_step_done("init", f).unwrap();

        let gateway = MockGatewayClient::new().with_workflow_id(wf);
        let worker = claude_client::StubClaudeClient::new();
        let err = handle_inner(&gateway, &worker, &dir, Args { force: false })
            .await
            .unwrap_err();
        assert!(format!("{}", err).contains("precondition"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
