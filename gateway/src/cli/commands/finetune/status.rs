//! `vllora finetune status` — pure read. Emit a single `status` event
//! summarising the pipeline journal + the suggested next command.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs
//! Contract: specs/003-cli-pipeline-verbs/contracts/verb-contract.md#status

use std::path::Path;

use clap::Parser;
use serde_json::{json, Value};
use vllora_core::metadata::pool::DbPool;
use vllora_finetune::state::journal::FileJournal;
use vllora_finetune::state::Journal;

use super::shared;

#[derive(Parser, Debug, Clone)]
pub struct Args {}

pub async fn handle(_db_pool: DbPool, args: Args) -> Result<(), crate::CliError> {
    let project_dir = shared::project_dir()?;
    handle_inner(&project_dir, args).await
}

pub async fn handle_inner(project_dir: &Path, _args: Args) -> Result<(), crate::CliError> {
    // `status` is read-only and must not fail when `init` hasn't run yet —
    // emit a status event with `current_phase: null` + next: /finetune-init.
    if !project_dir.join("pipeline-journal.json").is_file() {
        shared::emit(&shared::status(
            None,
            json!({}),
            Some("/finetune-init"),
        ));
        return Ok(());
    }

    // Load the journal. The schema version check here is the first place
    // a forward-incompatible journal file would be surfaced to users.
    let journal = FileJournal::open_or_create(project_dir, "unknown")
        .map_err(|e| crate::CliError::CustomError(format!("open journal: {}", e)))?;
    let doc = journal
        .read()
        .map_err(|e| crate::CliError::CustomError(format!("read journal: {}", e)))?;

    let current_phase = doc
        .get("current_phase")
        .and_then(|v| v.as_str())
        .map(String::from);

    let phases = doc.get("phases").cloned().unwrap_or_else(|| json!({}));

    let next_command = suggest_next_command(&phases, current_phase.as_deref());

    shared::emit(&shared::status(
        current_phase.as_deref(),
        phases,
        Some(&next_command),
    ));
    Ok(())
}

/// Map the current journal state to the slash command the user should run next.
/// This is the only place in the codebase that knows the §2.6 transition order.
fn suggest_next_command(phases: &Value, current_phase: Option<&str>) -> String {
    if let Some(phase) = current_phase {
        // Something is running — tell the user to wait.
        return format!("(in progress: {})", phase);
    }

    // If import-dataset ran, skip straight to eval regardless of sources/plan/generate.
    if phase_status(phases, "import-dataset") == Some("done") {
        return next_after_import_dataset(phases);
    }

    // Canonical order: init → sources → plan → generate → eval → train
    let sequence = [
        ("init", "/finetune-sources"),
        ("sources", "/finetune-plan"),
        ("plan", "/finetune-generate"),
        ("generate", "/finetune-eval"),
        ("eval", "/finetune-train"),
        ("train", "(pipeline complete)"),
    ];

    for (phase, next) in sequence {
        if phase_status(phases, phase) != Some("done") {
            // First incomplete phase determines the next command.
            return match phase {
                "init" => "/finetune-init".to_string(),
                "sources" => "/finetune-sources".to_string(),
                "plan" => "/finetune-plan".to_string(),
                "generate" => "/finetune-generate".to_string(),
                "eval" => "/finetune-eval".to_string(),
                "train" => "/finetune-train".to_string(),
                _ => next.to_string(),
            };
        }
    }

    "(pipeline complete)".to_string()
}

fn next_after_import_dataset(phases: &Value) -> String {
    if phase_status(phases, "eval") != Some("done") {
        "/finetune-eval".to_string()
    } else if phase_status(phases, "train") != Some("done") {
        "/finetune-train".to_string()
    } else {
        "(pipeline complete)".to_string()
    }
}

fn phase_status<'a>(phases: &'a Value, name: &str) -> Option<&'a str> {
    phases.get(name).and_then(|p| p.get("status")).and_then(|s| s.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn fresh_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vllora-status-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }

    #[tokio::test]
    async fn no_project_suggests_init() {
        let dir = fresh_dir();
        handle_inner(&dir, Args {}).await.unwrap();
        // The event was written to stdout; we can't capture it easily here,
        // so also exercise the helper directly.
        let next = suggest_next_command(&json!({}), None);
        assert_eq!(next, "/finetune-init");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn fresh_project_suggests_sources_after_init() {
        let dir = fresh_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let j = FileJournal::open_or_create(&dir, "wf-001").unwrap();
        j.write_step_start("init", 1).unwrap();
        j.write_step_done("init", BTreeMap::new()).unwrap();

        handle_inner(&dir, Args {}).await.unwrap();

        let doc = j.read().unwrap();
        let next = suggest_next_command(&doc["phases"], None);
        assert_eq!(next, "/finetune-sources");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_dataset_done_short_circuits_to_eval() {
        let phases = json!({
            "init": {"status": "done"},
            "import-dataset": {"status": "done"},
        });
        let next = suggest_next_command(&phases, None);
        assert_eq!(next, "/finetune-eval");
    }

    #[test]
    fn running_phase_shows_in_progress() {
        let phases = json!({"sources": {"status": "running"}});
        let next = suggest_next_command(&phases, Some("sources"));
        assert_eq!(next, "(in progress: sources)");
    }

    #[test]
    fn train_done_is_terminal() {
        let phases = json!({
            "init": {"status": "done"},
            "sources": {"status": "done"},
            "plan": {"status": "done"},
            "generate": {"status": "done"},
            "eval": {"status": "done"},
            "train": {"status": "done"},
        });
        let next = suggest_next_command(&phases, None);
        assert_eq!(next, "(pipeline complete)");
    }
}
