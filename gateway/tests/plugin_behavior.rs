//! Behavioural tests for the Claude Code plugin path.
//!
//! Track: C | Feature: 004-claude-code-plugin
//!
//! Each thin plugin verb (`commands/finetune-<verb>.md`) shells out to
//! `vllora finetune <verb>` via Bash. When Track B lands real verbs, that
//! will produce live stream-JSON; until then, we point plugin-path tests at
//! `mock_vllora` (the test fixture in `src/bin/mock_vllora.rs`) which emits
//! canned stream-JSON per `specs/003-*/contracts/stream-json.schema.json`.
//!
//! Cargo sets `CARGO_BIN_EXE_mock_vllora` automatically for integration
//! tests, so we don't need to hunt for the binary.

use std::process::Command;

/// Absolute path to the compiled mock binary, provided by Cargo.
fn mock_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mock_vllora")
}

/// Run `mock_vllora finetune <verb> [args...]` and return stdout as a string.
fn run(verb: &str, extra: &[&str]) -> String {
    let mut cmd = Command::new(mock_bin());
    cmd.arg("finetune").arg(verb);
    for a in extra {
        cmd.arg(a);
    }
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn mock_vllora: {e}"));
    assert!(
        output.status.success(),
        "mock_vllora finetune {} exited non-zero: stdout={:?} stderr={:?}",
        verb,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8(output.stdout).expect("stdout is utf-8")
}

/// Parse a JSONL stdout into one event per line.
fn parse_events(stdout: &str) -> Vec<serde_json::Value> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("invalid JSON line {line:?}: {e}"))
        })
        .collect()
}

#[test]
fn every_verb_produces_stream_json() {
    // Every verb the plugin exposes must emit at least one valid stream-JSON
    // event. Covers the surface that thin plugin commands actually wrap.
    for verb in ["init", "sources", "import-dataset", "plan", "generate", "eval", "train", "status", "quickstart"] {
        let stdout = run(verb, &[]);
        let events = parse_events(&stdout);
        assert!(
            !events.is_empty(),
            "verb '{}' produced no stream-JSON events",
            verb
        );
    }
}

#[test]
fn terminal_event_is_phase_done_or_status() {
    // Per spec 003's verb-contract.md, every verb except `status` ends with
    // a `phase_done` event. `status` emits a `status` event as its terminal.
    for verb in ["init", "sources", "import-dataset", "plan", "generate", "eval", "train", "quickstart"] {
        let stdout = run(verb, &[]);
        let events = parse_events(&stdout);
        let last = events.last().expect("at least one event");
        assert_eq!(
            last["type"].as_str(),
            Some("phase_done"),
            "verb '{}' terminal event is not phase_done: {:?}",
            verb,
            last
        );
    }

    let stdout = run("status", &[]);
    let last = parse_events(&stdout).last().cloned().expect("at least one event");
    assert_eq!(last["type"].as_str(), Some("status"));
}

#[test]
fn pipeline_verbs_surface_next_hint() {
    // Plugin thin commands relay `next` to the user. Verify the mock
    // populates it for every verb that advances the pipeline.
    let transitions = [
        ("init", "/finetune-sources"),
        ("sources", "/finetune-plan"),
        ("import-dataset", "/finetune-eval"),
        ("plan", "/finetune-generate"),
        ("generate", "/finetune-eval"),
        ("eval", "/finetune-train"),
        ("quickstart", "/finetune-plan"),
    ];
    for (verb, expected_next) in transitions {
        let stdout = run(verb, &[]);
        let last = parse_events(&stdout)
            .last()
            .cloned()
            .expect("at least one event");
        assert_eq!(
            last["next"].as_str(),
            Some(expected_next),
            "verb '{}' next hint mismatch — got: {:?}",
            verb,
            last
        );
    }
}

#[test]
fn eval_emits_worker_iteration_with_outcome() {
    let stdout = run("eval", &[]);
    let events = parse_events(&stdout);
    let iteration = events
        .iter()
        .find(|e| e["type"].as_str() == Some("worker_iteration"))
        .expect("eval should emit worker_iteration");
    assert_eq!(iteration["iteration"].as_u64(), Some(1));
    assert_eq!(iteration["outcome"].as_str(), Some("pass"));
}

#[test]
fn unknown_verb_exits_nonzero() {
    let output = Command::new(mock_bin())
        .args(["finetune", "nonsense"])
        .output()
        .expect("spawn");
    assert!(!output.status.success(), "unknown verb should exit non-zero");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn fixture_file_override_is_honored() {
    // A per-test override lets tests exercise edge cases without teaching
    // the mock new tricks.
    let tmp = std::env::temp_dir().join(format!("mock-vllora-fixture-{}.jsonl", std::process::id()));
    std::fs::write(
        &tmp,
        "{\"type\":\"error\",\"code\":\"PRECONDITION_UNMET\",\"message\":\"init not done\"}\n",
    )
    .expect("write fixture");

    let output = Command::new(mock_bin())
        .args(["finetune", "plan"])
        .env("MOCK_VLLORA_FIXTURE", &tmp)
        .output()
        .expect("spawn");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let events = parse_events(&stdout);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["type"].as_str(), Some("error"));
    assert_eq!(events[0]["code"].as_str(), Some("PRECONDITION_UNMET"));

    let _ = std::fs::remove_file(&tmp);
}
