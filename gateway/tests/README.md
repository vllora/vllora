# Gateway tests

Two test surfaces:

| Kind | Location | What it covers |
|---|---|---|
| Unit | `src/**/*.rs` under `#[cfg(test)]` modules | Pure-function / module-local behavior. Run with `cargo test -p vllora`. |
| Integration | `tests/*.rs` (this directory) | Cross-module + process-boundary tests. Same command runs both. |

## The `mock_vllora` fixture binary

`mock_vllora` is a test-only binary at [`src/bin/mock_vllora.rs`](../src/bin/mock_vllora.rs). It emits canned stream-JSON events per `vllora finetune <verb>` call, matching [stream-json.schema.json](../../../finetune-workflow-speckit/specs/003-cli-pipeline-verbs/contracts/stream-json.schema.json).

### Why it exists

The plugin (Feature 004) shells out to `vllora finetune <verb>` verbs. Until Track B lands the real verbs (Feature 003), the plugin path can't be exercised against live output. `mock_vllora` fills the gap: it's a zero-dependency Rust binary that emits schema-compliant events so plugin tests have a real feedback loop.

Side benefit: Track B can diff their real verb output against `mock_vllora`'s canned events to catch schema regressions.

### Quick start

Build it:

```bash
cargo build -p vllora --bin mock_vllora
```

Invoke it:

```bash
./target/debug/mock_vllora finetune init "build a support agent"
# {"type":"progress","phase":"init","message":"scaffolding finetune-project/"}
# {"type":"phase_done","phase":"init","status":"done","next":"/finetune-sources","summary":"workflow created"}
```

### In integration tests

Cargo auto-exposes the mock path as `CARGO_BIN_EXE_mock_vllora` inside any file in `tests/`. See [`plugin_behavior.rs`](plugin_behavior.rs) for the canonical usage pattern:

```rust
fn mock_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mock_vllora")
}

let output = Command::new(mock_bin())
    .args(["finetune", "eval"])
    .output()
    .expect("spawn mock_vllora");
```

No `PATH` shenanigans required — invoke the binary directly via the env var.

### Supported verbs

The mock recognizes every verb in `FinetuneCommand` (excluding `Jobs`, which is Layer B). Each has a hard-coded event sequence that ends with a `phase_done` (or `status` for the `status` verb):

| Verb | Events emitted |
|---|---|
| `init` | `progress` → `phase_done` (next: `/finetune-sources`) |
| `sources` | `progress` → `worker_start` → `worker_done` → `phase_done` (next: `/finetune-plan`) |
| `import-dataset` | `progress` → `phase_done` (next: `/finetune-eval`) |
| `plan` | `worker_start` (×2 — `relation_builder`, `grader_drafter:init`) → `worker_done` (×2) → `phase_done` (next: `/finetune-generate`) |
| `generate` | `worker_start` / `worker_done` (×2 — `record_generator`, `grader_drafter:finalize`) → `phase_done` (next: `/finetune-eval`) |
| `eval` | `worker_iteration` (outcome: pass, metrics set) → `phase_done` (next: `/finetune-train`) |
| `train` | `progress` (×2) → `phase_done` (adapter ID in summary) |
| `status` | single `status` event with `next_command` |
| `quickstart` | `progress` → `phase_done` (next: `/finetune-plan`) |
| `auto` | `progress` → `phase_done` |

Unknown verbs exit with code `2` (precondition unmet per the verb contract's exit-code convention).

### Per-test fixture override

When a canned sequence isn't enough for what your test needs (e.g., simulating an error, iterating through failures, forcing a specific metric), point the mock at a JSONL file via `MOCK_VLLORA_FIXTURE`:

```rust
let tmp = std::env::temp_dir().join("my-fixture.jsonl");
std::fs::write(&tmp, "{\"type\":\"error\",\"code\":\"PRECONDITION_UNMET\",\"message\":\"init not done\"}\n").unwrap();

let output = Command::new(mock_bin())
    .args(["finetune", "plan"])
    .env("MOCK_VLLORA_FIXTURE", &tmp)
    .output()
    .expect("spawn");
```

Each non-empty line in the fixture file is emitted verbatim on stdout. No validation — you own the shape.

### For Track B (contract-diff usage)

If you're implementing a real verb and want to sanity-check its output against the schema, the mock's event shape is a known-good reference:

```bash
# What the mock emits:
./target/debug/mock_vllora finetune sources /tmp/fixture.pdf > mock.jsonl

# What your real verb emits:
cargo run -p vllora -- finetune sources /tmp/fixture.pdf > real.jsonl

# Diff:
diff <(jq -c '{type, phase}' mock.jsonl) <(jq -c '{type, phase}' real.jsonl)
```

If your real verb emits events outside the mock's event-type set (`progress` / `worker_start` / `worker_done` / `worker_iteration` / `phase_done` / `status` / `error`), that's a schema contract violation per Feature 003 FR-015.

### Limitations

- **Not a replacement for real integration testing.** The mock guarantees schema validity and plausible event ordering; it does not simulate gateway state, workflow IDs, or cross-invocation side effects.
- **Canned events ignore arguments.** `mock_vllora finetune init "X"` and `mock_vllora finetune init "Y"` emit identical output. Use `MOCK_VLLORA_FIXTURE` when argument-sensitive behavior matters.
- **Zero DB writes, zero filesystem side effects.** The mock never touches `~/.vllora/` or the project directory. If your test needs those artifacts, create them manually in a scratch dir.

## Plugin behavioral tests

[`plugin_behavior.rs`](plugin_behavior.rs) — 6 tests covering the parity between plugin thin verbs, the mock, and the stream-JSON schema:

| Test | Guarantee |
|---|---|
| `every_verb_produces_stream_json` | Every plugin-surfaced verb emits ≥1 valid event. |
| `terminal_event_is_phase_done_or_status` | Pipeline verbs end with `phase_done`; `status` ends with `status`. |
| `pipeline_verbs_surface_next_hint` | Each verb's terminal event populates `next` per the §2.6 transition map. |
| `eval_emits_worker_iteration_with_outcome` | Eval-specific: `worker_iteration` event has the `outcome` field. |
| `unknown_verb_exits_nonzero` | Mock exits code 2 on unknown verb. |
| `fixture_file_override_is_honored` | `MOCK_VLLORA_FIXTURE` injection works. |

## Plugin static tests

[`../src/cli/commands/finetune/plugin_tests.rs`](../src/cli/commands/finetune/plugin_tests.rs) — 7 filesystem-level invariants on the `plugin/` bundle:

| Test | Guarantee |
|---|---|
| `orchestrator_file_exists` | `plugin/commands/finetune.md` present. |
| `every_thin_verb_has_a_matching_command_file` | Parity against the 9 verbs in spec 004 FR-003. |
| `every_expected_skill_exists` | 5 reference skills present. |
| `no_plugin_file_exceeds_max_lines` | ≤150 lines per file (FR-008). |
| `no_forbidden_code_fences` | No `python` / `rust` / `js` / `ts` / `bash-script` fenced blocks (FR-005). |
| `thin_verbs_shell_to_matching_cli_verb` | Each thin verb mentions `vllora finetune <matching-verb>` (FR-006). |
| `plugin_files_do_not_read_credentials` | No credential-path strings (FR-013). |

## Running the suite

```bash
cargo test -p vllora                    # everything
cargo test -p vllora plugin_tests       # static only
cargo test -p vllora --test plugin_behavior  # integration only
cargo test -p vllora setup              # setup/install-flow module
```
