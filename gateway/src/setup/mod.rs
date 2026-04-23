//! Idempotent setup — runs on every `vllora` invocation.
//!
//! Track: C | Feature: 005-install-flow
//! Design: parent §2.8
//!
//! This module extends the existing boot pattern in `gateway/src/main.rs`:
//!
//!   get_db_pool()          → creates ~/.vllora/ + opens SQLite
//!   init_db()              → runs migrations
//!   seed_database()        → seeds default project
//!   seed::seed_models()    → seeds models if empty (from handle_serve)
//!   seed::seed_providers() → seeds providers if empty (from handle_serve)
//!
//! We add:
//!
//!   setup::ensure_plugin_symlink()  → links bundled plugin/ to ~/.claude/plugins/vllora-finetune/
//!   setup::claude_readiness()       → non-fatal check of `claude` CLI + auth state
//!
//! These run on every invocation (idempotent — noop if already done).
//!
//! ## Wire-up
//!
//! In `gateway/src/main.rs`, after `let db_pool = get_db_pool()?;` (line ~90):
//!
//! ```rust,ignore
//! let db_pool = get_db_pool()?;
//! setup::ensure_plugin_symlink()?;      // NEW — idempotent
//! let _claude_status = setup::claude_readiness();  // NEW — non-fatal, reported by doctor
//! ```
//!
//! TODO [C]: implement the three functions + add the two lines above to main.rs.

use crate::CliError;

pub mod plugin_symlink;
pub mod claude_readiness;

/// Setup subsystem status — used by `vllora doctor`.
#[derive(Debug, Clone)]
pub struct SetupStatus {
    pub name: &'static str,
    pub ok: bool,
    pub message: String,
    pub remediation: Option<String>,
}

/// Create ~/.claude/plugins/vllora-finetune/ symlink to the bundled plugin directory.
/// Idempotent — noop if symlink already points at the expected target.
///
/// The plugin/ directory is bundled inside the pip wheel. At runtime, resolve its
/// path relative to the executable (per maturin wheel layout).
pub fn ensure_plugin_symlink() -> Result<(), CliError> {
    plugin_symlink::ensure()
}

/// Check Claude Code CLI + auth state. Non-fatal — reports, doesn't abort.
/// `vllora doctor` calls this to display the status.
pub fn claude_readiness() -> Vec<SetupStatus> {
    claude_readiness::check_all()
}
