//! `mock_vllora` — test fixture binary for Feature 004 plugin behavioural tests.
//!
//! Track: C | Feature: 004-claude-code-plugin
//!
//! Emits canned stream-JSON events matching
//! `specs/003-cli-pipeline-verbs/contracts/stream-json.schema.json` for each
//! finetune verb. Behavioural tests in `gateway/tests/plugin_behavior.rs`
//! invoke this binary instead of the real `vllora` to exercise the plugin
//! Markdown / orchestrator path without depending on Track B's verb
//! implementations.
//!
//! Usage:
//!
//! ```text
//! mock_vllora finetune <verb> [args...]
//! mock_vllora finetune <verb> --json-only            # suppresses any stderr
//! MOCK_VLLORA_FIXTURE=/path/to/override.jsonl        # optional per-test override
//! ```
//!
//! The binary is intentionally dependency-light — no serde, no clap — so it
//! compiles in a heartbeat and changes have zero blast radius on the main
//! `vllora` binary.

use std::io::{self, Write};

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    // Accept exactly `finetune <verb>` — anything else exits with
    // code 2 (precondition unmet).
    if argv.len() < 2 || argv[0] != "finetune" {
        usage();
        std::process::exit(2);
    }
    let verb = &argv[1];

    // Per-test override: point at any JSONL file and its lines are emitted
    // verbatim. Lets tests exercise edge cases without teaching the mock
    // new tricks.
    if let Ok(path) = std::env::var("MOCK_VLLORA_FIXTURE") {
        emit_fixture_file(&path);
        return;
    }

    match fixture_for(verb) {
        Some(events) => emit(events),
        None => {
            eprintln!("mock_vllora: unknown verb '{}'", verb);
            std::process::exit(2);
        }
    }
}

fn usage() {
    eprintln!("usage: mock_vllora finetune <verb> [args...]");
    eprintln!("       MOCK_VLLORA_FIXTURE=<path> mock_vllora finetune <verb>");
    eprintln!("verbs: init, sources, import-dataset, plan, generate, eval, train, status, quickstart, auto");
}

/// Canned events per verb. Each line is already a valid stream-JSON event.
///
/// Shapes match `stream-json.schema.json` — `progress`, `worker_start`,
/// `worker_done`, `worker_iteration`, `phase_done`, `status`. Minimal payloads:
/// tests that need richer ones should set `MOCK_VLLORA_FIXTURE`.
fn fixture_for(verb: &str) -> Option<&'static [&'static str]> {
    match verb {
        "init" => Some(&[
            r#"{"type":"progress","phase":"init","message":"scaffolding finetune-project/"}"#,
            r#"{"type":"phase_done","phase":"init","status":"done","next":"/finetune-sources","summary":"workflow created"}"#,
        ]),
        "sources" => Some(&[
            r#"{"type":"progress","phase":"sources","message":"resolving sources"}"#,
            r#"{"type":"worker_start","worker":"knowledge_extractor","target":"fixture.pdf"}"#,
            r#"{"type":"worker_done","worker":"knowledge_extractor","status":"ok","target":"fixture.pdf","artifacts_count":3}"#,
            r#"{"type":"phase_done","phase":"sources","status":"done","next":"/finetune-plan","summary":"1 document ingested"}"#,
        ]),
        "import-dataset" => Some(&[
            r#"{"type":"progress","phase":"import-dataset","message":"validating records"}"#,
            r#"{"type":"phase_done","phase":"import-dataset","status":"done","next":"/finetune-eval","summary":"20 records imported"}"#,
        ]),
        "plan" => Some(&[
            r#"{"type":"worker_start","worker":"relation_builder"}"#,
            r#"{"type":"worker_done","worker":"relation_builder","status":"ok","artifacts_count":1}"#,
            r#"{"type":"worker_start","worker":"grader_drafter","mode":"init"}"#,
            r#"{"type":"worker_done","worker":"grader_drafter","mode":"init","status":"ok","artifacts_count":1}"#,
            r#"{"type":"phase_done","phase":"plan","status":"done","next":"/finetune-generate","summary":"plan.md written"}"#,
        ]),
        "generate" => Some(&[
            r#"{"type":"worker_start","worker":"record_generator"}"#,
            r#"{"type":"worker_done","worker":"record_generator","status":"ok","artifacts_count":40}"#,
            r#"{"type":"worker_start","worker":"grader_drafter","mode":"finalize"}"#,
            r#"{"type":"worker_done","worker":"grader_drafter","mode":"finalize","status":"ok","artifacts_count":1}"#,
            r#"{"type":"phase_done","phase":"generate","status":"done","next":"/finetune-eval","summary":"40 records generated"}"#,
        ]),
        "eval" => Some(&[
            r#"{"type":"worker_iteration","phase":"eval","iteration":1,"outcome":"pass","metrics":{"readiness_score":0.82,"avg_score":0.71}}"#,
            r#"{"type":"phase_done","phase":"eval","status":"done","next":"/finetune-train","summary":"readiness=pass"}"#,
        ]),
        "train" => Some(&[
            r#"{"type":"progress","phase":"train","message":"training started"}"#,
            r#"{"type":"progress","phase":"train","message":"step 100/500","pct":20}"#,
            r#"{"type":"phase_done","phase":"train","status":"done","summary":"adapter: fixture-adapter-xyz"}"#,
        ]),
        "status" => Some(&[
            r#"{"type":"status","current_phase":null,"phases":{"init":{"status":"done"}},"next_command":"/finetune-sources"}"#,
        ]),
        "quickstart" => Some(&[
            r#"{"type":"progress","phase":"quickstart","message":"interactive wizard"}"#,
            r#"{"type":"phase_done","phase":"quickstart","status":"done","next":"/finetune-plan","summary":"init + sources chained"}"#,
        ]),
        "auto" => Some(&[
            r#"{"type":"progress","phase":"auto","message":"iteration 1 / 3"}"#,
            r#"{"type":"phase_done","phase":"auto","status":"done","summary":"pipeline completed in 3 iterations"}"#,
        ]),
        _ => None,
    }
}

fn emit(events: &[&str]) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in events {
        writeln!(&mut out, "{}", line).ok();
        out.flush().ok();
    }
}

fn emit_fixture_file(path: &str) {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let stdout = io::stdout();
            let mut out = stdout.lock();
            for line in content.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                writeln!(&mut out, "{}", line).ok();
            }
        }
        Err(e) => {
            eprintln!("mock_vllora: cannot read fixture {}: {}", path, e);
            std::process::exit(1);
        }
    }
}
