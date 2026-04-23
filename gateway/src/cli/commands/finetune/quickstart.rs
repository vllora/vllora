//! `vllora finetune quickstart` — wizard that chains init + sources.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §5.8
//!
//! MVP: takes the same flags as `init` + `sources` and runs them in sequence.
//! Interactive stdin prompts (the full wizard experience) is future work —
//! for now the plugin narrator + CLI flags cover the same ground.

use std::path::Path;

use clap::Parser;
use vllora_core::metadata::pool::DbPool;
use vllora_finetune::gateway_client::GatewayClient;

use super::shared;
use super::workers::claude_client::{self, ClaudeClient};
use super::{init, sources};

#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Training objective.
    pub objective: String,
    /// One or more source paths / URIs.
    pub sources: Vec<String>,
    #[arg(long, default_value = "qwen-3.5-2b")]
    pub base_model: String,
    #[arg(long)]
    pub name: Option<String>,
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
    if args.objective.trim().is_empty() {
        return Err(crate::CliError::CustomError(
            "objective is required for quickstart".into(),
        ));
    }

    // Step 1: init
    init::handle_inner(
        gateway,
        project_dir,
        init::Args {
            objective: args.objective.clone(),
            base_model: args.base_model,
            name: args.name,
            force: false,
        },
    )
    .await?;

    // Step 2: sources (skip when the user didn't provide any — quickstart
    // is still useful purely as init-with-narration).
    if !args.sources.is_empty() {
        sources::handle_inner(
            gateway,
            worker,
            project_dir,
            sources::Args {
                sources: args.sources,
                parallel: 12,
                force: false,
            },
        )
        .await?;
    }

    shared::emit(&shared::phase_done(
        "quickstart",
        "done",
        Some("/finetune-plan"),
        Some("init + sources chained"),
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use vllora_finetune::gateway_client::MockGatewayClient;

    fn fresh() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vllora-quick-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }

    #[tokio::test]
    async fn chains_init_then_sources() {
        let dir = fresh();
        let wf = Uuid::new_v4();
        let gateway = MockGatewayClient::new().with_workflow_id(wf);
        let worker = claude_client::StubClaudeClient::new();
        let pdf = dir.join("x.pdf");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&pdf, b"%PDF-1.4").unwrap();

        handle_inner(
            &gateway,
            &worker,
            &dir,
            Args {
                objective: "build a support agent".into(),
                sources: vec![pdf.display().to_string()],
                base_model: "qwen-3.5-2b".into(),
                name: None,
            },
        )
        .await
        .unwrap();

        let j = vllora_finetune::state::journal::FileJournal::open_or_create(
            &dir,
            &wf.to_string(),
        )
        .unwrap();
        use vllora_finetune::state::Journal;
        assert!(j.is_phase_done("init").unwrap());
        assert!(j.is_phase_done("sources").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn empty_objective_rejected() {
        let dir = fresh();
        let gateway = MockGatewayClient::new();
        let worker = claude_client::StubClaudeClient::new();
        let err = handle_inner(
            &gateway,
            &worker,
            &dir,
            Args {
                objective: "".into(),
                sources: vec![],
                base_model: "qwen-3.5-2b".into(),
                name: None,
            },
        )
        .await
        .unwrap_err();
        assert!(format!("{}", err).contains("objective"));
    }
}
