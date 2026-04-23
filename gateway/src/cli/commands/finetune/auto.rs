//! `vllora finetune auto` — autonomous loop.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §2.3.2
//!
//! Runs `status` to find the next phase, executes it, repeats until the
//! pipeline completes, fails, or hits `--max-iterations`.
//!
//! MVP constraints:
//!   - `init` cannot be auto-invoked (needs objective). If the user hasn't
//!     run init, auto surfaces a clear error and exits.
//!   - `import-dataset` cannot be auto-invoked (needs a source path).
//!   - `train` auto-invokes only when the user passed `--allow-train` — otherwise
//!     we stop at readiness=pass and surface the manual next step (training
//!     is expensive; the user should confirm).

use std::path::Path;

use clap::Parser;
use serde_json::Value;
use vllora_core::metadata::pool::DbPool;
use vllora_finetune::gateway_client::GatewayClient;
use vllora_finetune::state::journal::FileJournal;
use vllora_finetune::state::Journal;

use super::shared;
use super::workers::claude_client::{self, ClaudeClient};
use super::{eval, generate, plan, train};

#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Cap the outer loop (default 10 — more than any real pipeline needs).
    #[arg(long, default_value_t = 10)]
    pub max_iterations: u32,
    /// When set, auto-invokes `/finetune-train` after eval readiness=pass.
    /// Off by default because training is expensive.
    #[arg(long)]
    pub allow_train: bool,
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
    if !project_dir.join("pipeline-journal.json").is_file() {
        return Err(crate::CliError::CustomError(
            "no project — run /finetune-init first; auto needs an initialised workflow".into(),
        ));
    }

    let mut iterations = 0u32;
    let mut prior_next: Option<String> = None;

    loop {
        if iterations >= args.max_iterations {
            shared::emit(&shared::phase_done(
                "auto",
                "done",
                None,
                Some(&format!(
                    "hit --max-iterations={}; stopping",
                    args.max_iterations
                )),
            ));
            return Ok(());
        }
        iterations += 1;

        let journal = FileJournal::open_or_create(project_dir, "unused").map_err(to_cli)?;
        let doc = journal.read().map_err(to_cli)?;
        let phases = doc.get("phases").cloned().unwrap_or(Value::Null);

        let next = next_command(&phases, args.allow_train);

        // Termination: pipeline complete, manual step required, or loop.
        if next.is_none() {
            shared::emit(&shared::phase_done(
                "auto",
                "done",
                None,
                Some("pipeline complete"),
            ));
            return Ok(());
        }
        let next_command = next.unwrap();

        // Loop-break detection: if we'd run the same next command twice in a row
        // AND the underlying phase made no progress, stop.
        if prior_next.as_deref() == Some(&next_command) {
            shared::emit(&shared::phase_done(
                "auto",
                "done",
                Some(&next_command),
                Some(&format!(
                    "no progress across iterations — manual action needed at {}",
                    next_command
                )),
            ));
            return Ok(());
        }
        prior_next = Some(next_command.clone());

        shared::emit(&shared::progress(
            "auto",
            &format!("iteration {}/{} — running {}", iterations, args.max_iterations, next_command),
            None,
        ));

        // Dispatch.
        let result = match next_command.as_str() {
            "/finetune-plan" => plan::handle_inner(gateway, worker, project_dir, plan::Args { force: false }).await,
            "/finetune-generate" => {
                generate::handle_inner(
                    gateway,
                    worker,
                    project_dir,
                    generate::Args {
                        topics: 8,
                        per_topic: 5,
                        force: false,
                    },
                )
                .await
            }
            "/finetune-eval" => {
                eval::handle_inner(
                    gateway,
                    worker,
                    project_dir,
                    eval::Args {
                        max_iterations: 3,
                        model: "qwen-3.5-4b".into(),
                        force: false,
                    },
                )
                .await
            }
            "/finetune-train" if args.allow_train => {
                train::handle_inner(
                    gateway,
                    worker,
                    project_dir,
                    train::Args {
                        model: "qwen-3.5-4b".into(),
                        config: None,
                        force: false,
                    },
                )
                .await
            }
            other => {
                // Any other "next" (init, sources, import-dataset, train without
                // allow-train) is a manual step in auto mode.
                shared::emit(&shared::phase_done(
                    "auto",
                    "done",
                    Some(other),
                    Some(&format!(
                        "manual step required: {} is not auto-dispatchable",
                        other
                    )),
                ));
                return Ok(());
            }
        };
        if let Err(e) = result {
            shared::emit(&shared::error_event("INTERNAL", &format!("{}", e), None));
            return Err(e);
        }
    }
}

fn next_command(phases: &Value, allow_train: bool) -> Option<String> {
    let status = |p: &str| {
        phases
            .get(p)
            .and_then(|v| v.get("status"))
            .and_then(|s| s.as_str())
    };
    // import-dataset shortcut.
    if status("import-dataset") == Some("done") {
        if status("eval") != Some("done") {
            return Some("/finetune-eval".into());
        }
        let readiness = phases["eval"]["fields"]["readiness"].as_str().unwrap_or("fail");
        if readiness == "pass" {
            return if allow_train && status("train") != Some("done") {
                Some("/finetune-train".into())
            } else if status("train") != Some("done") {
                Some("/finetune-train".into()) // surface as manual step
            } else {
                None
            };
        }
        // readiness fail — auto can't fix a data-side regression; surface.
        return Some("/finetune-plan".into());
    }

    for (phase, command) in [
        ("init", "/finetune-init"),
        ("sources", "/finetune-sources"),
        ("plan", "/finetune-plan"),
        ("generate", "/finetune-generate"),
    ] {
        if status(phase) != Some("done") {
            return Some(command.into());
        }
    }
    if status("eval") != Some("done") {
        return Some("/finetune-eval".into());
    }
    let readiness = phases["eval"]["fields"]["readiness"].as_str().unwrap_or("fail");
    if readiness != "pass" {
        return Some("/finetune-plan".into());
    }
    if status("train") != Some("done") {
        return if allow_train {
            Some("/finetune-train".into())
        } else {
            Some("/finetune-train".into())
        };
    }
    None
}

fn to_cli<E: std::fmt::Display>(e: E) -> crate::CliError {
    crate::CliError::CustomError(format!("{}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn next_command_after_init() {
        let phases = json!({"init": {"status": "done"}});
        assert_eq!(next_command(&phases, false), Some("/finetune-sources".into()));
    }

    #[test]
    fn next_command_after_generate() {
        let phases = json!({
            "init": {"status": "done"},
            "sources": {"status": "done"},
            "plan": {"status": "done"},
            "generate": {"status": "done"},
        });
        assert_eq!(next_command(&phases, false), Some("/finetune-eval".into()));
    }

    #[test]
    fn next_command_after_eval_pass_no_train_flag() {
        let phases = json!({
            "init": {"status": "done"},
            "sources": {"status": "done"},
            "plan": {"status": "done"},
            "generate": {"status": "done"},
            "eval": {"status": "done", "fields": {"readiness": "pass"}},
        });
        // Without --allow-train, we still suggest train — auto stops there.
        assert_eq!(next_command(&phases, false), Some("/finetune-train".into()));
    }

    #[test]
    fn next_command_import_dataset_path() {
        let phases = json!({
            "init": {"status": "done"},
            "import-dataset": {"status": "done"},
        });
        assert_eq!(next_command(&phases, false), Some("/finetune-eval".into()));
    }

    #[test]
    fn next_command_pipeline_complete() {
        let phases = json!({
            "init": {"status": "done"},
            "sources": {"status": "done"},
            "plan": {"status": "done"},
            "generate": {"status": "done"},
            "eval": {"status": "done", "fields": {"readiness": "pass"}},
            "train": {"status": "done"},
        });
        assert_eq!(next_command(&phases, true), None);
    }
}
