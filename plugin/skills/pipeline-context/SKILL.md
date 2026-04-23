---
name: pipeline-context
description: |
  Architecture overview for the vllora fine-tune pipeline — what each phase
  produces, how artifacts flow, key invariants, where state lives.
  Always loaded during any `/finetune*` turn so Claude has the full map.
  Triggers: any /finetune* command; "fine-tune", "training pipeline",
  "what does this pipeline do", "pipeline state", "pipeline artifacts".
---

# Pipeline Context

## Pipeline overview

Six deterministic phases, each producing durable artifacts the next phase consumes. The CLI (`vllora finetune <verb>`) runs the phase; workers (`claude -p` subprocesses) handle LLM-heavy work; the gateway persists state.

```
init → sources → plan → generate → eval → train
```

Alternative entry: `import-dataset` replaces `sources → plan → generate` when the user has pre-built records.

## What each phase produces

| Phase | Input | Output artifacts |
|---|---|---|
| `init` | objective (string) | `finetune-project/` dir + `pipeline-journal.json` + workflow row on gateway |
| `sources` | paths / URIs | `knowledge_parts` in gateway (with `origin_uri`) + cache under `~/.vllora/cache/sources/` |
| `plan` | knowledge parts | `plan.md` (human-reviewable) + topics + relations + grader draft |
| `generate` | topics + knowledge | `records` in gateway (with `source_parts`) + finalized grader; quality gate enforced |
| `import-dataset` | .jsonl/.parquet/hf:// | `records` with `origin_uri` + `origin_source_id`; skips sources/plan/generate |
| `eval` | records + grader | eval runs + readiness verdict (`pass`/`fail`); may iterate `grader_drafter(refine)` |
| `train` | readiness=pass | GRPO job + `training_metrics` + `monitor-report-{N}.md` + adapter ID |

## State files (local mirror + server mirror)

- **`pipeline-journal.json`** — single-writer per project; phase-level status (`pending | running | iterating | done | failed`). Mirrored server-side in `workflows.pipeline_journal`.
- **`analysis.json`** — append-only running diary of cross-phase reasoning + decisions. Every phase appends; downstream phases read. Mirrored in `workflows.iteration_state`.
- **`change-log.md`** — append-only audit trail of grader modifications (author, rationale, diff).
- **`plan.md`** — human review artifact produced by `plan`.
- **`monitor-report-{N}.md`** — per-round training summary from `training_monitor`.

Key invariant: all history files are **append-only**. API exposes only `append` / `augment` — no overwrite primitive. Downstream phases see a stable history.

## Three-axis architecture

- **Local vLLora** — `vllora` Rust binary driving pipeline verbs + workers via subprocess; stores local state at `~/.vllora/` and `finetune-project/`.
- **Cloud Gateway** — REST service (`localhost:9090` in dev) holding workflows, records, graders, eval runs, training jobs. SQLite at `~/.vllora/vllora.db`.
- **Skill** — Claude Code plugin: 1 orchestrator + 9 thin verb commands + 5 reference skills (this is one of them).

Data flows top-to-bottom: skill → CLI → gateway → SQLite. Every write returns a durable ID the skill can later read.

## Invariants worth remembering

1. **Gateway is a side-effect store, not the orchestrator.** The skill / CLI drives; the gateway persists.
2. **Plugin has no pipeline logic.** Every pipeline action is `Bash("vllora finetune <verb>")`. No plugin file reads records, writes state, or calls gateway directly.
3. **No credential file reads.** Claude auth flows through `claude -p` subprocess inheritance. `~/.claude/.credentials.json` is never touched by vllora.
4. **Evaluation gates training.** `train` refuses to start without `eval` done + `readiness=pass` (parent §9 — non-negotiable).
5. **System prompt is per-topic, source context is per-record.** Never put per-record data in system prompt; never embed source context as ground truth.
6. **Records carry `origin_uri` + `origin_source_id`.** Traceability is a first-class field, not a comment.
7. **Graders carry `change_reason`.** Every grader version explains why it exists.

## Gotchas

- **Terminology split**: Layer A verb is `import-dataset` (user-facing — user brings a dataset). Layer B primitive + the DB table are `records`.
- **Topic IDs are slugs locally, UUIDs on the gateway.** The CLI maps at the upload boundary.
- **`auto` and `jobs` verbs are terminal-only** — the plugin deliberately doesn't expose them.
- **Idempotency via journal**: Re-running a verb reads `pipeline-journal.json` and skips `done` phases. `--force` resets.

## Related skills (contextual loads)

- `topic-hierarchy` — loads during `plan` / `generate`.
- `grader-writing` — loads whenever grader authoring or diagnosis comes up.
- `readiness-gate` — loads when `eval` fails or the user asks about the verdict.
- `nemo-guide` — loads during `generate` when the user reaches for NeMo-based data generation.

## Where to dig deeper

- Parent design: `vllora/ui/docs/workflow-skill-first-approach/finetune-skill-command-redesign.md`.
- Stream-JSON event taxonomy: `finetune-workflow-speckit/specs/003-cli-pipeline-verbs/contracts/stream-json.schema.json`.
- State schemas: `vllora/finetune/src/state/schemas/`.
