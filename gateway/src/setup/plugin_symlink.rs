//! Plugin symlink management.
//!
//! Track: C | Feature: 005-install-flow
//!
//! At runtime, determine the plugin source directory (from the pip wheel or
//! the cargo workspace during dev), and ensure ~/.claude/plugins/vllora-finetune/
//! symlinks to it. Idempotent — if the symlink exists and points at the right
//! target, do nothing.

use crate::CliError;
use std::path::PathBuf;

/// Resolve the bundled plugin directory path.
///
/// TODO [C]:
///   - In dev: resolve relative to `std::env::current_exe()` → `../../plugin/`
///   - In pip wheel: resolve via maturin's data-dir convention
///   - Fall back to VLLORA_PLUGIN_DIR env var for testing
fn plugin_source_dir() -> Result<PathBuf, CliError> {
    unimplemented!("TODO Track C — 005-install-flow — plugin_source_dir");
}

/// Resolve the target symlink location: ~/.claude/plugins/vllora-finetune/
fn plugin_target_dir() -> Result<PathBuf, CliError> {
    unimplemented!("TODO Track C — 005-install-flow — plugin_target_dir");
}

/// Ensure the symlink exists and points at the bundled plugin.
/// - If target doesn't exist: create symlink.
/// - If target exists and points at expected source: noop.
/// - If target exists and points elsewhere: warn (don't overwrite user config).
/// - If ~/.claude/plugins/ doesn't exist: create it (user may not have used any plugin before).
pub fn ensure() -> Result<(), CliError> {
    let _src = plugin_source_dir()?;
    let _tgt = plugin_target_dir()?;
    unimplemented!("TODO Track C — 005-install-flow — ensure symlink");
}

/// Remove the symlink (called by `vllora doctor --clean` — future).
#[allow(dead_code)]
pub fn remove() -> Result<(), CliError> {
    unimplemented!("TODO Track C — 005-install-flow — remove symlink");
}
