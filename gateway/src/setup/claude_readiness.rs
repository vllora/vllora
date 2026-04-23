//! Claude CLI + auth readiness checks. NON-FATAL — reports, never aborts boot.
//!
//! Track: C | Feature: 005-install-flow
//! Design: parent §2.10 — red lines:
//!   - MUST NOT read ~/.claude/.credentials.json directly
//!   - Check auth via subprocess (`claude config list` or equivalent)
//!
//! Called from main.rs on every invocation to populate the status that
//! `vllora doctor` reports. Not called from the critical path — we never
//! block startup on Claude Code being installed.

use super::SetupStatus;

/// Run all readiness checks. Each returns a SetupStatus; aggregation is the caller's concern.
pub fn check_all() -> Vec<SetupStatus> {
    vec![
        check_claude_cli_on_path(),
        check_claude_auth_configured(),
    ]
}

/// Is `claude` binary on PATH? Run `claude --version` via subprocess.
///
/// TODO [C]: implement via std::process::Command.
fn check_claude_cli_on_path() -> SetupStatus {
    SetupStatus {
        name: "claude-cli",
        ok: false,
        message: "TODO [C] — not implemented".into(),
        remediation: Some("Install Claude Code CLI from https://claude.com/claude-code".into()),
    }
}

/// Is Claude auth configured? Options:
///   - ANTHROPIC_API_KEY env var set
///   - `claude config list` reports logged-in subscription
///   - apiKeyHelper configured
///
/// NEVER read ~/.claude/.credentials.json directly (ToS compliance — parent §2.10.1).
///
/// TODO [C]: implement via `claude config list` subprocess + env-var check.
fn check_claude_auth_configured() -> SetupStatus {
    SetupStatus {
        name: "claude-auth",
        ok: false,
        message: "TODO [C] — not implemented".into(),
        remediation: Some("Run `claude login` or set ANTHROPIC_API_KEY".into()),
    }
}
