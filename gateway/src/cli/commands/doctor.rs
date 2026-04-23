//! `vllora doctor` — diagnostic report of setup status.
//!
//! Track: C | Feature: 005-install-flow | Design: parent §2.8
//!
//! Aggregates probes from `crate::setup::claude_readiness()` + locally-computed
//! filesystem / port / env probes per `specs/005-install-flow/contracts/doctor-checklist.md`.
//!
//! Human-readable table by default, stream-JSON when `--json`. Exit code 1 when
//! any `required` probe fails; warnings don't fail the run.

use crate::setup::SetupStatus;
use clap::Parser;
use diesel::{sql_query, QueryableByName, RunQueryDsl};
use serde::Serialize;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use vllora_core::metadata::pool::DbPool;

const DEFAULT_GATEWAY_PORT: u16 = 9090;

#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Output as JSON instead of human-readable table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Serialize, Clone, Debug)]
struct ProbeRow {
    name: &'static str,
    ok: bool,
    required: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    remediation: Option<String>,
}

impl ProbeRow {
    fn from_status(s: SetupStatus, required: bool) -> Self {
        Self {
            name: s.name,
            ok: s.ok,
            required,
            message: s.message,
            remediation: s.remediation,
        }
    }
}

pub async fn handle_doctor(db_pool: DbPool, args: Args) -> Result<(), crate::CliError> {
    let rows = collect_probes(&db_pool);
    if args.json {
        print_json(&rows)?;
    } else {
        print_table(&rows);
    }

    let required_failed = rows.iter().filter(|r| r.required && !r.ok).count();
    if required_failed > 0 {
        // Print hint, then exit non-zero via an error — but don't treat as a
        // CliError::CustomError (that would clutter the output). Use a dedicated
        // sentinel.
        eprintln!("\n{} required check(s) failed. Run `vllora init` or follow remediation above.", required_failed);
        std::process::exit(1);
    }
    Ok(())
}

fn collect_probes(db_pool: &DbPool) -> Vec<ProbeRow> {
    let mut rows: Vec<ProbeRow> = Vec::new();

    // Required probes (per doctor-checklist.md) —
    let (cli_probe, auth_probe) = {
        let mut claude = crate::setup::claude_readiness();
        // claude_readiness returns exactly [claude-cli-on-path, claude-auth-configured] per check_all().
        let auth = claude.pop().expect("claude_readiness should return 2 statuses");
        let cli = claude.pop().expect("claude_readiness should return 2 statuses");
        (cli, auth)
    };
    rows.push(ProbeRow::from_status(cli_probe, /* required */ true));
    rows.push(ProbeRow::from_status(auth_probe, /* required */ true));
    rows.push(ProbeRow::from_status(check_vllora_dir_writable(), true));
    rows.push(ProbeRow::from_status(check_gateway_db_initialized(db_pool), true));
    rows.push(ProbeRow::from_status(check_plugin_symlink(), true));
    rows.push(ProbeRow::from_status(check_gateway_port(), true));

    // Warning probes (don't block)
    rows.push(ProbeRow::from_status(check_uv_on_path(), false));
    rows.push(ProbeRow::from_status(check_env_var("hf-token-present", "HF_TOKEN", "needed for hf:// sources"), false));
    rows.push(ProbeRow::from_status(check_env_var("aws-creds-present", "AWS_ACCESS_KEY_ID", "needed for s3:// sources"), false));
    rows.push(ProbeRow::from_status(check_claude_plugin_version(), false));

    rows
}

fn check_uv_on_path() -> SetupStatus {
    match Command::new("uv").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            SetupStatus {
                name: "uv-on-path",
                ok: true,
                message: if version.is_empty() { "uv found".into() } else { format!("uv found: {}", version) },
                remediation: None,
            }
        }
        _ => SetupStatus {
            name: "uv-on-path",
            ok: false,
            message: "uv not found (only needed for Python helper scripts in finetune-skill/scripts/)".into(),
            remediation: Some("Install uv: https://docs.astral.sh/uv/".into()),
        },
    }
}

fn check_gateway_port() -> SetupStatus {
    let port: u16 = std::env::var("VLLORA_GATEWAY_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_GATEWAY_PORT);

    // Case 1: port is free — bindable.
    if TcpListener::bind(("127.0.0.1", port)).is_ok() {
        return SetupStatus {
            name: "gateway-port",
            ok: true,
            message: format!("port {} available", port),
            remediation: None,
        };
    }

    // Case 2: port in use — probe /health to see whether a vllora server is responding.
    if port_responds_as_vllora(port) {
        return SetupStatus {
            name: "gateway-port",
            ok: true,
            message: format!("port {} owned by a running vllora serve", port),
            remediation: None,
        };
    }

    // Case 3: port in use by something that isn't us.
    SetupStatus {
        name: "gateway-port",
        ok: false,
        message: format!("port {} in use by another process", port),
        remediation: Some("Stop the conflicting process or set VLLORA_GATEWAY_PORT".into()),
    }
}

fn port_responds_as_vllora(port: u16) -> bool {
    use std::io::{Read, Write};
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(2)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let req = b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    if stream.write_all(req).is_err() {
        return false;
    }
    let mut buf = [0u8; 512];
    let n = stream.read(&mut buf).unwrap_or(0);
    let response = String::from_utf8_lossy(&buf[..n]);
    // Any 2xx response from /health counts — vllora's gateway answers `OK` to /health.
    response.starts_with("HTTP/1.1 2") || response.starts_with("HTTP/1.0 2")
}

fn check_gateway_db_initialized(db_pool: &DbPool) -> SetupStatus {
    #[derive(QueryableByName)]
    struct IntegrityRow {
        #[diesel(sql_type = diesel::sql_types::Text)]
        integrity_check: String,
    }

    let mut conn = match db_pool.get() {
        Ok(c) => c,
        Err(e) => {
            return SetupStatus {
                name: "gateway-db-initialized",
                ok: false,
                message: format!("cannot open DB connection: {}", e),
                remediation: Some("Delete ~/.vllora/vllora.db and re-run vllora".into()),
            };
        }
    };

    match sql_query("PRAGMA integrity_check").load::<IntegrityRow>(&mut conn) {
        Ok(rows) => {
            let first = rows.first().map(|r| r.integrity_check.as_str()).unwrap_or("");
            if first == "ok" {
                SetupStatus {
                    name: "gateway-db-initialized",
                    ok: true,
                    message: "SQLite integrity OK".into(),
                    remediation: None,
                }
            } else {
                let report = rows
                    .iter()
                    .take(3)
                    .map(|r| r.integrity_check.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                SetupStatus {
                    name: "gateway-db-initialized",
                    ok: false,
                    message: format!("integrity check: {}", report),
                    remediation: Some("Delete ~/.vllora/vllora.db and re-run vllora".into()),
                }
            }
        }
        Err(e) => SetupStatus {
            name: "gateway-db-initialized",
            ok: false,
            message: format!("integrity-check query failed: {}", e),
            remediation: Some("Delete ~/.vllora/vllora.db and re-run vllora".into()),
        },
    }
}

fn check_claude_plugin_version() -> SetupStatus {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => {
            return SetupStatus {
                name: "claude-plugin-version",
                ok: false,
                message: "$HOME not set".into(),
                remediation: None,
            };
        }
    };
    let manifest = PathBuf::from(home).join(".claude/plugins/vllora-finetune/plugin.json");

    let content = match std::fs::read_to_string(&manifest) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return SetupStatus {
                name: "claude-plugin-version",
                ok: false,
                message: "plugin.json not found (plugin symlink missing?)".into(),
                remediation: Some("Re-run vllora to create the plugin symlink".into()),
            };
        }
        Err(e) => {
            return SetupStatus {
                name: "claude-plugin-version",
                ok: false,
                message: format!("cannot read plugin.json: {}", e),
                remediation: None,
            };
        }
    };

    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            return SetupStatus {
                name: "claude-plugin-version",
                ok: false,
                message: format!("plugin.json parse error: {}", e),
                remediation: None,
            };
        }
    };

    let req = json.get("requires").and_then(|r| r.get("cli")).and_then(|v| v.as_str());
    match req {
        None => SetupStatus {
            name: "claude-plugin-version",
            ok: true,
            message: "plugin manifest has no requires.cli constraint".into(),
            remediation: None,
        },
        Some(req_str) => match check_cli_requirement(req_str) {
            Ok(msg) => SetupStatus {
                name: "claude-plugin-version",
                ok: true,
                message: msg,
                remediation: None,
            },
            Err(msg) => SetupStatus {
                name: "claude-plugin-version",
                ok: false,
                message: msg,
                remediation: Some(
                    "Upgrade the stale side: `pip install --upgrade vllora` or re-sync the plugin".into(),
                ),
            },
        },
    }
}

/// Parse `"vllora >= 0.1.0"`-style constraints and compare against the running
/// CLI version. Minimal semver — no pre-release / build-metadata handling.
/// Supported ops: `>=`, `>`, `==`.
fn check_cli_requirement(req: &str) -> Result<String, String> {
    let current = env!("CARGO_PKG_VERSION");
    let parts: Vec<&str> = req.split_whitespace().collect();
    if parts.len() != 3 || parts[0] != "vllora" {
        return Err(format!("unrecognised requires.cli format: '{}'", req));
    }
    let op = parts[1];
    let required = parts[2];
    let ok = match op {
        ">=" => version_cmp(current, required) >= std::cmp::Ordering::Equal,
        ">" => version_cmp(current, required) == std::cmp::Ordering::Greater,
        "==" => version_cmp(current, required) == std::cmp::Ordering::Equal,
        _ => return Err(format!("unsupported requires.cli op: '{}'", op)),
    };
    if ok {
        Ok(format!("vllora {} satisfies '{}'", current, req))
    } else {
        Err(format!("vllora {} does not satisfy '{}'", current, req))
    }
}

fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    parse_semver(a).cmp(&parse_semver(b))
}

fn parse_semver(v: &str) -> (u32, u32, u32) {
    let mut parts = v.split('.').map(|s| s.parse::<u32>().unwrap_or(0));
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);
    (major, minor, patch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_parse_and_compare() {
        assert_eq!(parse_semver("0.1.23"), (0, 1, 23));
        assert_eq!(parse_semver("1.0"), (1, 0, 0));
        assert_eq!(parse_semver("2.3.4"), (2, 3, 4));
        assert_eq!(parse_semver("garbage"), (0, 0, 0));
        assert!(version_cmp("0.2.0", "0.1.0").is_gt());
        assert!(version_cmp("0.1.0", "0.1.0").is_eq());
        assert!(version_cmp("0.1.23", "0.2.0").is_lt());
    }

    #[test]
    fn cli_requirement_parsing() {
        // Assume CARGO_PKG_VERSION is "0.1.23" — test by computing current from env.
        let current = env!("CARGO_PKG_VERSION");

        let satisfied = check_cli_requirement("vllora >= 0.0.1");
        assert!(satisfied.is_ok(), "{} should satisfy >= 0.0.1, got: {:?}", current, satisfied);

        let unsatisfied = check_cli_requirement("vllora >= 999.0.0");
        assert!(unsatisfied.is_err(), "{} should NOT satisfy >= 999.0.0", current);

        let bad_format = check_cli_requirement("something else");
        assert!(bad_format.is_err());

        let bad_op = check_cli_requirement("vllora ~ 0.1.0");
        assert!(bad_op.is_err());
    }
}

fn check_vllora_dir_writable() -> SetupStatus {
    match vllora_dir() {
        Ok(dir) => {
            if !dir.exists() {
                return SetupStatus {
                    name: "vllora-dir-writable",
                    ok: false,
                    message: format!("{} does not exist", dir.display()),
                    remediation: Some("Re-run `vllora` to auto-create it, or check $HOME".into()),
                };
            }
            let probe = dir.join(".write-probe");
            match std::fs::write(&probe, b"ok") {
                Ok(_) => {
                    let _ = std::fs::remove_file(&probe);
                    SetupStatus {
                        name: "vllora-dir-writable",
                        ok: true,
                        message: format!("{} writable", dir.display()),
                        remediation: None,
                    }
                }
                Err(e) => SetupStatus {
                    name: "vllora-dir-writable",
                    ok: false,
                    message: format!("{} not writable: {}", dir.display(), e),
                    remediation: Some("Check directory permissions".into()),
                },
            }
        }
        Err(e) => SetupStatus {
            name: "vllora-dir-writable",
            ok: false,
            message: e,
            remediation: Some("Set $HOME to a writable path".into()),
        },
    }
}

fn check_plugin_symlink() -> SetupStatus {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => {
            return SetupStatus {
                name: "plugin-symlink",
                ok: false,
                message: "$HOME not set".into(),
                remediation: Some("Set $HOME".into()),
            };
        }
    };
    let target = PathBuf::from(&home).join(".claude/plugins/vllora-finetune");
    match std::fs::symlink_metadata(&target) {
        Ok(meta) if meta.file_type().is_symlink() => {
            match std::fs::read_link(&target) {
                Ok(link_to) => SetupStatus {
                    name: "plugin-symlink",
                    ok: true,
                    message: format!("{} → {}", target.display(), link_to.display()),
                    remediation: None,
                },
                Err(e) => SetupStatus {
                    name: "plugin-symlink",
                    ok: false,
                    message: format!("symlink exists but unreadable: {}", e),
                    remediation: Some("Re-run vllora to re-create the symlink".into()),
                },
            }
        }
        Ok(_) => SetupStatus {
            name: "plugin-symlink",
            ok: false,
            message: format!("{} exists but is not a symlink", target.display()),
            remediation: Some("Remove it manually and re-run vllora".into()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SetupStatus {
            name: "plugin-symlink",
            ok: false,
            message: format!("{} missing", target.display()),
            remediation: Some("Re-run vllora to create the plugin symlink".into()),
        },
        Err(e) => SetupStatus {
            name: "plugin-symlink",
            ok: false,
            message: format!("could not stat {}: {}", target.display(), e),
            remediation: None,
        },
    }
}

fn check_env_var(slug: &'static str, key: &str, purpose: &str) -> SetupStatus {
    match std::env::var(key) {
        Ok(v) if !v.trim().is_empty() => SetupStatus {
            name: slug,
            ok: true,
            message: format!("{} set", key),
            remediation: None,
        },
        _ => SetupStatus {
            name: slug,
            ok: false,
            message: format!("{} not set ({})", key, purpose),
            remediation: Some(format!("Set {} if you plan to use this provider", key)),
        },
    }
}

fn vllora_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "$HOME not set".to_string())?;
    Ok(PathBuf::from(home).join(".vllora"))
}

fn print_table(rows: &[ProbeRow]) {
    let total = rows.len();
    let ok = rows.iter().filter(|r| r.ok).count();
    let failed_required = rows.iter().filter(|r| r.required && !r.ok).count();
    let warnings = rows.iter().filter(|r| !r.required && !r.ok).count();

    println!("vllora doctor");
    println!();
    for row in rows {
        let marker = match (row.ok, row.required) {
            (true, _) => "✓",
            (false, true) => "✗",
            (false, false) => "⚠",
        };
        println!("{}  {:<24}  {}", marker, row.name, row.message);
        if !row.ok {
            if let Some(r) = &row.remediation {
                println!("    → {}", r);
            }
        }
    }

    println!();
    println!(
        "Summary: {} OK, {} failed, {} warnings (of {} checks)",
        ok,
        failed_required,
        warnings,
        total
    );
}

fn print_json(rows: &[ProbeRow]) -> Result<(), crate::CliError> {
    let json = serde_json::to_string_pretty(rows)?;
    println!("{}", json);
    Ok(())
}
