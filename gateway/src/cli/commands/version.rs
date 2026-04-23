//! `vllora version` — print CLI + gateway + plugin versions.
//!
//! Track: C | Feature: 005-install-flow | Design: parent §2.8

pub async fn handle_version() -> Result<(), crate::CliError> {
    // TODO [C]: print structured version info (cli crate version, gateway binary version, plugin manifest version).
    unimplemented!("TODO Track C — 005-install-flow — version");
}
