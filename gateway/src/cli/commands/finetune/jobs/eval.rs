//! jobs eval run | stop.
//!
//! Track: A | Feature: 001+002 | Design: Feature 001 FR-018 catalog

use clap::Parser;
use vllora_core::metadata::pool::DbPool;

#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Idempotency key — same key + payload returns existing job_id.
    #[arg(long)]
    pub idempotency_key: Option<String>,

    /// Create job only; skip live tracking. Use `jobs status` later.
    #[arg(long)]
    pub only_tracking: bool,

    /// Arbitrary JSON input for the operation (per-op schemas TBD).
    #[arg(long)]
    pub input: Option<String>,
}

pub async fn handle(_db_pool: DbPool, _args: Args) -> Result<(), crate::CliError> {
    unimplemented!("TODO Track A — 001+002 — eval");
}
