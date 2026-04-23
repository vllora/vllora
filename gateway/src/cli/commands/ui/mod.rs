//! `vllora ui start/stop/open` — optional React UI management.
//!
//! Track: C | Feature: 005-install-flow | Design: parent §2.8

use clap::Subcommand;

pub mod start;
pub mod stop;
pub mod open;

#[derive(Subcommand, Debug)]
pub enum UiCommand {
    /// Start UI at :5173.
    Start(start::Args),
    /// Stop UI.
    Stop,
    /// Open browser to UI.
    Open,
}
