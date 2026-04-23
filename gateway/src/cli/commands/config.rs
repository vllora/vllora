//! `vllora config get/set` — ~/.vllora/config.yaml management.
//!
//! Track: C | Feature: 005-install-flow | Design: parent §2.8

use clap::{Parser, Subcommand};
use vllora_core::metadata::pool::DbPool;

#[derive(Parser, Debug, Clone)]
pub struct Args {
    #[command(subcommand)]
    pub op: ConfigOp,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigOp {
    /// Print a config value.
    Get { key: String },
    /// Set a config value.
    Set { key: String, value: String },
}

pub async fn handle_config(_db_pool: DbPool, _args: Args) -> Result<(), crate::CliError> {
    // TODO [C]: read/write ~/.vllora/config.yaml via serde_yaml.
    unimplemented!("TODO Track C — 005-install-flow — config");
}
