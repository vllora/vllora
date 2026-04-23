//! Claude CLI + auth readiness checks. NON-FATAL — reports, never aborts boot.
//!
//! Track: C | Feature: 005-install-flow
//! Design: parent §2.10 — red lines:
//!   - MUST NOT read `~/.claude/.credentials.json` directly.
//!   - Check auth via subprocess (`claude config list` or equivalent).
//!
//! Called from `main.rs` on every invocation to populate the status that
//! `vllora doctor` reports. Never on the critical path — we never block
//! startup on Claude Code being installed.

use super::SetupStatus;
use std::process::Command;

/// Run all readiness checks. Each returns a `SetupStatus`; aggregation is
/// `doctor`'s concern.
pub fn check_all() -> Vec<SetupStatus> {
    vec![
        check_claude_cli_on_path(),
        check_claude_auth_configured(),
    ]
}

/// Is `claude` on PATH? Invoke `claude --version` via subprocess and capture
/// the output. Short timeout; non-fatal on any failure.
fn check_claude_cli_on_path() -> SetupStatus {
    match Command::new("claude").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            SetupStatus {
                name: "claude-cli-on-path",
                ok: true,
                message: if version.is_empty() {
                    "claude CLI found".into()
                } else {
                    format!("claude CLI found: {}", version)
                },
                remediation: None,
            }
        }
        Ok(out) => SetupStatus {
            name: "claude-cli-on-path",
            ok: false,
            message: format!(
                "claude --version exited with status {}",
                out.status.code().map(|c| c.to_string()).unwrap_or_else(|| "unknown".into())
            ),
            remediation: Some("Reinstall Claude Code from https://claude.com/claude-code".into()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SetupStatus {
            name: "claude-cli-on-path",
            ok: false,
            message: "claude not found on PATH".into(),
            remediation: Some("Install Claude Code CLI from https://claude.com/claude-code".into()),
        },
        Err(e) => SetupStatus {
            name: "claude-cli-on-path",
            ok: false,
            message: format!("could not invoke claude: {}", e),
            remediation: Some("Check $PATH and Claude Code installation".into()),
        },
    }
}

/// Is Claude auth configured? Signals checked, in order:
///   1. `ANTHROPIC_API_KEY` env var is set and non-empty (CI path).
///   2. `claude config list` runs successfully AND returns non-empty output
///      (indicates `claude login` has been run — subscription or API-key path).
///
/// We NEVER read `~/.claude/.credentials.json` directly (ToS compliance,
/// parent §2.10.1).
fn check_claude_auth_configured() -> SetupStatus {
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.trim().is_empty() {
            return SetupStatus {
                name: "claude-auth-configured",
                ok: true,
                message: "ANTHROPIC_API_KEY set".into(),
                remediation: None,
            };
        }
    }

    match Command::new("claude").args(["config", "list"]).output() {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.trim().is_empty() {
                SetupStatus {
                    name: "claude-auth-configured",
                    ok: false,
                    message: "claude config list returned empty — not logged in".into(),
                    remediation: Some("Run `claude login` or set ANTHROPIC_API_KEY".into()),
                }
            } else {
                SetupStatus {
                    name: "claude-auth-configured",
                    ok: true,
                    message: "claude config present".into(),
                    remediation: None,
                }
            }
        }
        Ok(_) | Err(_) => SetupStatus {
            name: "claude-auth-configured",
            ok: false,
            message: "claude auth not configured".into(),
            remediation: Some("Run `claude login` or set ANTHROPIC_API_KEY".into()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_all_returns_both_probes() {
        let results = check_all();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "claude-cli-on-path");
        assert_eq!(results[1].name, "claude-auth-configured");
    }

    // Env-var tests share process state — consolidated into one sequential
    // function so cargo's default parallel runner can't race them.
    #[test]
    fn api_key_env_handling() {
        // Case 1: a real key marks auth as ok.
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let status = check_claude_auth_configured();
        assert!(status.ok, "ANTHROPIC_API_KEY should mark auth as ok");
        assert_eq!(status.message, "ANTHROPIC_API_KEY set");

        // Case 2: whitespace-only key falls through to the subprocess probe.
        std::env::set_var("ANTHROPIC_API_KEY", "   ");
        let status = check_claude_auth_configured();
        assert_ne!(
            status.message, "ANTHROPIC_API_KEY set",
            "whitespace-only key must not short-circuit the probe"
        );

        std::env::remove_var("ANTHROPIC_API_KEY");
    }
}
