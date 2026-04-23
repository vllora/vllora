//! `vllora doctor` — diagnostic report of setup status.
//!
//! Track: C | Feature: 005-install-flow | Design: parent §2.8

use clap::Parser;
use vllora_core::metadata::pool::DbPool;

#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Output as JSON instead of human-readable table.
    #[arg(long)]
    pub json: bool,
}

pub async fn handle_doctor(_db_pool: DbPool, _args: Args) -> Result<(), crate::CliError> {
    // TODO [C]: call crate::setup::claude_readiness() + port + gateway health + plugin symlink + provider env vars.
    // Render as table (default) or JSON (--json).
    unimplemented!("TODO Track C — 005-install-flow — doctor");
}
