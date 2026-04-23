//! `vllora gateway start/stop/status/logs/reset` — gateway lifecycle ops.
//!
//! Track: C | Feature: 005-install-flow | Design: parent §2.8

use clap::Subcommand;

pub mod start;
pub mod stop;
pub mod status;
pub mod logs;
pub mod reset;

#[derive(Subcommand, Debug)]
pub enum GatewayCommand {
    /// Start gateway on configured port (wraps existing Serve).
    Start(start::Args),
    /// Stop the running gateway.
    Stop,
    /// Gateway health + version.
    Status,
    /// Tail gateway logs.
    Logs(logs::Args),
    /// Wipe SQLite DB (with confirmation).
    Reset,
}
