//! `vllora finetune <verb>` — pipeline verb subcommand tree.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs
//! Design: parent §2.3.2 thin verbs + §5 per-command specs
//!
//! Wired into `gateway/src/cli/mod.rs` via `Commands::Finetune(FinetuneCommand)`.
//! Dispatched from `gateway/src/main.rs` via `handle_finetune(db_pool, cmd)`.

use clap::Subcommand;
use vllora_core::metadata::pool::DbPool;

pub mod init;
pub mod sources;
pub mod import_dataset;
pub mod plan;
pub mod generate;
pub mod eval;
pub mod train;
pub mod status;
pub mod quickstart;
pub mod auto;
pub mod jobs;
pub mod workers;

#[derive(Subcommand, Debug)]
pub enum FinetuneCommand {
    /// Scaffold finetune-project/ + create gateway workflow.
    Init(init::Args),
    /// Ingest sources (PDFs, OTel traces) from local paths or URIs.
    Sources(sources::Args),
    /// Import pre-built records (skips sources+plan+generate).
    #[command(name = "import-dataset")]
    ImportDataset(import_dataset::Args),
    /// Build topic hierarchy + grader draft + plan.md.
    Plan(plan::Args),
    /// Generate training records + finalize grader + quality gate.
    Generate(generate::Args),
    /// Dry-run eval on 4B + 0.8B; readiness gate; iterate.
    Eval(eval::Args),
    /// GRPO training + monitor + analyze; iterate.
    Train(train::Args),
    /// Pipeline status (pure read; print Next: /finetune-<verb>).
    Status(status::Args),
    /// Guided first-run wizard (chains init + sources).
    Quickstart(quickstart::Args),
    /// Autonomous loop: status → next-command until done or blocked.
    Auto(auto::Args),
    /// Layer B job operations (direct gateway job invocation).
    #[command(subcommand)]
    Jobs(jobs::JobsCommand),
}

pub async fn handle_finetune(
    db_pool: DbPool,
    cmd: FinetuneCommand,
) -> Result<(), crate::CliError> {
    match cmd {
        FinetuneCommand::Init(args)          => init::handle(db_pool, args).await,
        FinetuneCommand::Sources(args)       => sources::handle(db_pool, args).await,
        FinetuneCommand::ImportDataset(args) => import_dataset::handle(db_pool, args).await,
        FinetuneCommand::Plan(args)          => plan::handle(db_pool, args).await,
        FinetuneCommand::Generate(args)      => generate::handle(db_pool, args).await,
        FinetuneCommand::Eval(args)          => eval::handle(db_pool, args).await,
        FinetuneCommand::Train(args)         => train::handle(db_pool, args).await,
        FinetuneCommand::Status(args)        => status::handle(db_pool, args).await,
        FinetuneCommand::Quickstart(args)    => quickstart::handle(db_pool, args).await,
        FinetuneCommand::Auto(args)          => auto::handle(db_pool, args).await,
        FinetuneCommand::Jobs(cmd)           => jobs::handle_jobs(db_pool, cmd).await,
    }
}
