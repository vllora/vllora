//! Layer B job operations — thin CLI over vllora_finetune client methods.
//!
//! Track: A | Feature: 001 + 002 | Design: parent §2.4 + Feature 001 FR-018

use clap::Subcommand;
use vllora_core::metadata::pool::DbPool;

pub mod status;
pub mod knowledge;
pub mod records;
pub mod grader;
pub mod eval;
pub mod train;
pub mod test_job;

#[derive(Subcommand, Debug)]
pub enum JobsCommand {
    /// Retrieve job status by ID (read-only).
    Status(status::Args),
    /// Knowledge add.
    Knowledge(knowledge::Args),
    /// Records import | generate.
    Records(records::Args),
    /// Grader import | generate | dryrun.
    Grader(grader::Args),
    /// Eval run | stop.
    Eval(eval::Args),
    /// Train run | stop.
    Train(train::Args),
    /// Smoke-test the job lifecycle end to end.
    #[command(name = "test-job")]
    TestJob(test_job::Args),
}

pub async fn handle_jobs(
    db_pool: DbPool,
    cmd: JobsCommand,
) -> Result<(), crate::CliError> {
    match cmd {
        JobsCommand::Status(args)    => status::handle(db_pool, args).await,
        JobsCommand::Knowledge(args) => knowledge::handle(db_pool, args).await,
        JobsCommand::Records(args)   => records::handle(db_pool, args).await,
        JobsCommand::Grader(args)    => grader::handle(db_pool, args).await,
        JobsCommand::Eval(args)      => eval::handle(db_pool, args).await,
        JobsCommand::Train(args)     => train::handle(db_pool, args).await,
        JobsCommand::TestJob(args)   => test_job::handle(db_pool, args).await,
    }
}
