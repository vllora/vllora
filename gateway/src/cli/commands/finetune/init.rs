//! `vllora finetune init <objective>` — scaffold `finetune-project/` and
//! register a workflow on the gateway.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs
//! Contract: specs/003-cli-pipeline-verbs/contracts/verb-contract.md#init-objective

use std::collections::BTreeMap;
use std::path::Path;

use clap::Parser;
use serde_json::json;
use vllora_core::metadata::pool::DbPool;
use vllora_finetune::gateway_client::GatewayClient;
use vllora_finetune::state::journal::FileJournal;
use vllora_finetune::state::{lock, Journal};

use super::shared;

#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Training objective (one sentence). Stored on the workflow row.
    pub objective: String,
    /// Base model (default: qwen-3.5-2b). Pass qwen-3.5-4b only for tool-calling agents.
    #[arg(long, default_value = "qwen-3.5-2b")]
    pub base_model: String,
    /// Optional display name / slug for the workflow.
    #[arg(long)]
    pub name: Option<String>,
    /// Reset the journal and re-create the workflow even if one exists.
    #[arg(long)]
    pub force: bool,
}

pub async fn handle(_db_pool: DbPool, args: Args) -> Result<(), crate::CliError> {
    let gateway = shared::make_gateway_client();
    let project_dir = shared::project_dir()?;
    handle_inner(&*gateway, &project_dir, args).await
}

/// Dependency-injected entry point — verbs' unit tests call this directly
/// with a `MockGatewayClient` + scratch `project_dir`.
pub async fn handle_inner<G: GatewayClient + ?Sized>(
    gateway: &G,
    project_dir: &Path,
    args: Args,
) -> Result<(), crate::CliError> {
    if args.objective.trim().is_empty() {
        shared::emit(&shared::error_event(
            "INVALID_REQUEST",
            "objective must be non-empty",
            Some("Usage: vllora finetune init \"<objective>\""),
        ));
        return Err(crate::CliError::CustomError(
            "objective must be non-empty".into(),
        ));
    }

    std::fs::create_dir_all(project_dir)?;

    shared::emit(&shared::progress(
        "init",
        "scaffolding finetune-project/",
        None,
    ));

    // Single-writer lock. Guard dropped at scope end.
    let _guard = lock::acquire(project_dir).map_err(|e| {
        crate::CliError::CustomError(format!("could not acquire project lock: {}", e))
    })?;

    // Ask the gateway for a workflow ID.
    let workflow_id = gateway
        .create_workflow(&args.objective, &args.base_model)
        .await
        .map_err(|e| crate::CliError::CustomError(format!("gateway.create_workflow: {}", e)))?;

    // Open-or-create the local journal bound to that ID.
    let journal = FileJournal::open_or_create(project_dir, &workflow_id.to_string())
        .map_err(|e| crate::CliError::CustomError(format!("journal init: {}", e)))?;

    // If init is already `done` and --force wasn't passed, this is a no-op.
    if journal
        .is_phase_done("init")
        .map_err(|e| crate::CliError::CustomError(format!("journal read: {}", e)))?
        && !args.force
    {
        shared::emit(&shared::phase_done(
            "init",
            "done",
            Some("/finetune-sources"),
            Some(&format!(
                "workflow {} already initialised (run with --force to reset)",
                workflow_id
            )),
        ));
        return Ok(());
    }

    // Start + done for this phase. Keep it single-step; no sub-work yet.
    journal
        .write_step_start("init", std::process::id())
        .map_err(|e| crate::CliError::CustomError(format!("journal write_step_start: {}", e)))?;

    let mut fields = BTreeMap::new();
    fields.insert("workflow_id".into(), json!(workflow_id.to_string()));
    fields.insert("objective".into(), json!(args.objective));
    fields.insert("base_model".into(), json!(args.base_model));
    if let Some(n) = &args.name {
        fields.insert("name".into(), json!(n));
    }

    journal
        .write_step_done("init", fields)
        .map_err(|e| crate::CliError::CustomError(format!("journal write_step_done: {}", e)))?;

    // Mirror the journal to the server for cross-machine resume. Best-effort —
    // local write is the source of truth.
    if let Ok(doc) = journal.read() {
        let journal_json = doc.to_string();
        let _ = gateway.put_pipeline_journal(workflow_id, &journal_json).await;
    }

    shared::emit(&shared::phase_done(
        "init",
        "done",
        Some("/finetune-sources"),
        Some(&format!("workflow created: {}", workflow_id)),
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use vllora_finetune::gateway_client::{MockCall, MockGatewayClient};

    fn fresh_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vllora-init-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }

    #[tokio::test]
    async fn happy_path_creates_workflow_and_journal() {
        let dir = fresh_dir();
        let fixed = Uuid::new_v4();
        let gateway = MockGatewayClient::new().with_workflow_id(fixed);

        let args = Args {
            objective: "build a support agent".into(),
            base_model: "qwen-3.5-2b".into(),
            name: None,
            force: false,
        };
        handle_inner(&gateway, &dir, args).await.unwrap();

        // Journal exists with the right workflow_id + init done.
        let journal = FileJournal::open_or_create(&dir, &fixed.to_string()).unwrap();
        let doc = journal.read().unwrap();
        assert_eq!(doc["workflow_id"].as_str(), Some(fixed.to_string().as_str()));
        assert_eq!(doc["phases"]["init"]["status"].as_str(), Some("done"));
        assert_eq!(
            doc["phases"]["init"]["fields"]["objective"].as_str(),
            Some("build a support agent")
        );

        // Mock recorded the expected calls.
        let calls = gateway.calls();
        assert!(matches!(calls[0], MockCall::CreateWorkflow { .. }));
        assert!(matches!(calls[1], MockCall::PutPipelineJournal { .. }));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn empty_objective_rejected() {
        let dir = fresh_dir();
        let gateway = MockGatewayClient::new();
        let args = Args {
            objective: "   ".into(),
            base_model: "qwen-3.5-2b".into(),
            name: None,
            force: false,
        };
        let err = handle_inner(&gateway, &dir, args).await.unwrap_err();
        assert!(format!("{}", err).contains("non-empty"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rerun_is_idempotent_without_force() {
        let dir = fresh_dir();
        let fixed = Uuid::new_v4();
        let gateway = MockGatewayClient::new().with_workflow_id(fixed);

        let args = Args {
            objective: "obj".into(),
            base_model: "qwen-3.5-2b".into(),
            name: None,
            force: false,
        };
        handle_inner(&gateway, &dir, args.clone()).await.unwrap();
        // Second call without --force should succeed as a no-op. Doesn't
        // re-write the journal phase (started_at doesn't change).
        let before = std::fs::read_to_string(dir.join("pipeline-journal.json")).unwrap();
        handle_inner(&gateway, &dir, args).await.unwrap();
        let after = std::fs::read_to_string(dir.join("pipeline-journal.json")).unwrap();
        assert_eq!(before, after, "idempotent re-run must not touch the journal");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
