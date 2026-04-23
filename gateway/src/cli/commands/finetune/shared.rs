//! Shared helpers for Feature 003 pipeline verbs.
//!
//! - `project_dir()` — resolve `finetune-project/` under the current working
//!   directory (or `VLLORA_PROJECT_DIR` when set for tests).
//! - `emit()` + typed builders — stream-JSON event emission matching
//!   `specs/003-cli-pipeline-verbs/contracts/stream-json.schema.json`.
//! - `make_gateway_client()` — MVP implementation returns a mock until
//!   Track A's Feature 002 ships the real adapter. Verbs call this from
//!   their `handle()` entry point; `handle_inner()` takes `&dyn GatewayClient`
//!   so tests can substitute.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs

use std::io::Write;
use std::path::PathBuf;

use chrono::Utc;
use serde_json::{json, Value};
use vllora_finetune::gateway_client::{GatewayClient, LangdbGatewayClient, MockGatewayClient};

pub const FINETUNE_PROJECT_DIR: &str = "finetune-project";
pub const GATEWAY_MODE_ENV: &str = "VLLORA_GATEWAY_MODE";
pub const LANGDB_API_KEY_ENV: &str = "LANGDB_API_KEY";

/// Resolve the project directory. Respects `VLLORA_PROJECT_DIR` when set
/// (so tests can point at a scratch directory without changing `cwd`).
pub fn project_dir() -> Result<PathBuf, crate::CliError> {
    if let Ok(override_dir) = std::env::var("VLLORA_PROJECT_DIR") {
        return Ok(PathBuf::from(override_dir));
    }
    let cwd = std::env::current_dir()?;
    Ok(cwd.join(FINETUNE_PROJECT_DIR))
}

/// Build the `GatewayClient` for production verb invocations.
///
/// Selection rule:
///   - `VLLORA_GATEWAY_MODE=real` (or `live`) + `LANGDB_API_KEY` set → real
///     `LangdbGatewayClient` (Feature 002 adapter). Hits the cloud client
///     under the hood; many methods are still stubs while Feature 001 server
///     routes land — see `finetune/src/gateway_client.rs` for the per-method
///     TODO map.
///   - Otherwise (default) → `MockGatewayClient` for safety. Makes dev +
///     CI runs deterministic and doesn't require credentials.
///
/// Switching is done via env var rather than a flag so every verb +
/// integration test picks up the same behaviour without individual plumbing.
pub fn make_gateway_client() -> Box<dyn GatewayClient> {
    let mode = std::env::var(GATEWAY_MODE_ENV).unwrap_or_default().to_lowercase();
    if matches!(mode.as_str(), "real" | "live") {
        match std::env::var(LANGDB_API_KEY_ENV) {
            Ok(key) if !key.trim().is_empty() => match LangdbGatewayClient::new(key) {
                Ok(client) => return Box::new(client),
                Err(e) => {
                    eprintln!(
                        "Warning: {}={} but LangdbGatewayClient failed to construct ({}). Falling back to mock.",
                        GATEWAY_MODE_ENV, mode, e
                    );
                }
            },
            _ => {
                eprintln!(
                    "Warning: {}={} but {} is not set — falling back to mock gateway.",
                    GATEWAY_MODE_ENV, mode, LANGDB_API_KEY_ENV
                );
            }
        }
    }
    Box::new(MockGatewayClient::new())
}

// ---------------------------------------------------------------------------
// Stream-JSON emission
// ---------------------------------------------------------------------------

/// Emit a single stream-JSON event on stdout as a newline-terminated
/// compact JSON line. Flushes immediately so Claude Code plugin narrators
/// see progress in real time.
pub fn emit(event: &Value) {
    // Match the wire format used by `mock_vllora`: compact JSON + newline.
    let line = event.to_string();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(&mut out, "{}", line);
    let _ = out.flush();
}

/// `progress` event.
pub fn progress(phase: &str, message: &str, pct: Option<u8>) -> Value {
    let mut v = json!({
        "type": "progress",
        "phase": phase,
        "message": message,
        "emitted_at": Utc::now().to_rfc3339(),
    });
    if let Some(p) = pct {
        v["pct"] = json!(p);
    }
    v
}

/// `phase_done` event. `next` is the suggested slash command for the user;
/// `summary` is a one-line recap.
pub fn phase_done(phase: &str, status: &str, next: Option<&str>, summary: Option<&str>) -> Value {
    let mut v = json!({
        "type": "phase_done",
        "phase": phase,
        "status": status,
        "emitted_at": Utc::now().to_rfc3339(),
    });
    if let Some(n) = next {
        v["next"] = json!(n);
    }
    if let Some(s) = summary {
        v["summary"] = json!(s);
    }
    v
}

/// `status` event (the terminal event for `/finetune-status`).
pub fn status(current_phase: Option<&str>, phases: Value, next_command: Option<&str>) -> Value {
    let mut v = json!({
        "type": "status",
        "current_phase": current_phase,
        "phases": phases,
        "emitted_at": Utc::now().to_rfc3339(),
    });
    if let Some(nc) = next_command {
        v["next_command"] = json!(nc);
    }
    v
}

/// `error` event.
pub fn error_event(code: &str, message: &str, hint: Option<&str>) -> Value {
    let mut v = json!({
        "type": "error",
        "code": code,
        "message": message,
        "emitted_at": Utc::now().to_rfc3339(),
    });
    if let Some(h) = hint {
        v["hint"] = json!(h);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_has_required_fields() {
        let ev = progress("sources", "extracting", Some(25));
        assert_eq!(ev["type"], json!("progress"));
        assert_eq!(ev["phase"], json!("sources"));
        assert_eq!(ev["message"], json!("extracting"));
        assert_eq!(ev["pct"], json!(25));
        assert!(ev["emitted_at"].is_string());
    }

    #[test]
    fn phase_done_omits_optional_fields_when_none() {
        let ev = phase_done("init", "done", None, None);
        assert!(ev.get("next").is_none());
        assert!(ev.get("summary").is_none());
    }

    #[test]
    fn phase_done_includes_next_when_present() {
        let ev = phase_done("init", "done", Some("/finetune-sources"), Some("workflow created"));
        assert_eq!(ev["next"], json!("/finetune-sources"));
        assert_eq!(ev["summary"], json!("workflow created"));
    }

    #[test]
    fn project_dir_respects_override() {
        std::env::set_var("VLLORA_PROJECT_DIR", "/tmp/fake-project");
        let d = project_dir().unwrap();
        assert_eq!(d, PathBuf::from("/tmp/fake-project"));
        std::env::remove_var("VLLORA_PROJECT_DIR");
    }
}
