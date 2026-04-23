---
name: finetune-generate
description: |
  Generate training records from the topic hierarchy + finalize the grader.
  Spawns record_generator workers per leaf topic, then grader_drafter(finalize)
  to lock in the grader for eval + training. Runs a quality gate before exit.
  Triggers: "generate records", "build training data", "finalize grader".
allowed-tools: Bash, Read, Edit
---

# /finetune-generate

Thin narrator for `vllora finetune generate`. The second LLM-heavy phase: turns the topic hierarchy into concrete training records and produces the finalized grader. Enforces the data-quality gate before handing off to eval.

## When to use

After `/finetune-plan` succeeds and the user has confirmed the topics in `plan.md` look right.

## Preconditions

- `plan` phase must be `done`.
- `plan.md` exists and reflects the user's latest edits (if any).

## What this command does

1. Shell out via `Bash`:
   ```bash
   vllora finetune generate
   ```
2. Stream events. Expect many `worker_start` / `worker_done` pairs (one per leaf topic for `record_generator`, plus one for `grader_drafter` mode `finalize`), then a terminal `phase_done`.
3. If the quality gate fails inside the CLI, the `phase_done` event carries `status: "done"` but `summary` flags the failure. Relay the details — don't treat it as success just because exit was 0.
4. Surface the `next` field (should be `"/finetune-eval"`).

## The quality gate

The CLI runs `data_quality_gate.py` before marking `generate` done. The gate checks:
- Total record count above the minimum (per-topic min × topic count).
- Duplicate fraction under 15% (enforced via `deduplicate_records.py`).
- No empty topics (zero-relation guard).
- Source traceability populated on every record (`source_parts` with `[1], [2]…` pointers).

If the gate fails, the phase is marked `done` with `quality_gate: fail` in the journal, and the next verb (`eval`) will refuse to start until the user re-runs `/finetune-plan` + `/finetune-generate`.

## Usage shape

```bash
vllora finetune generate
# If the gate fails, adjust plan.md and retry:
vllora finetune plan --force
vllora finetune generate
```

## Duration expectations

- Small workflow (10 topics × 5 records each): ~3–5 min.
- Medium workflow (30 topics × 10 records each): ~8–12 min.
- Per-topic LLM retries (`MAX_LLM_RETRIES=2`) can add a minute under load.

## Artifacts produced

- Records inserted into the gateway's `records` table with `origin_uri` + numbered `source_parts` for every row.
- Grader finalized in the `graders` table; `change-log.md` gets an entry noting the finalize-mode rationale.
- `pipeline-journal.json` → `phases.generate.status: "done"` (+ `quality_gate: pass|fail`).

## On failure

- **Exit 2** (precondition): `plan` not done. Tell user to run `/finetune-plan` first.
- **Exit 1** (runtime): worker crashed, gateway upload failed, or the quality gate hard-failed (distinct from a soft fail — the CLI distinguishes). Relay stderr.

If a lot of per-topic workers report `status: incomplete`, that usually means the grader drafter couldn't settle on a stable spec for that topic. Suggest the user revisit `plan.md` and narrow the topic description.

## What happens next

After `phase_done` with a passing quality gate, suggest `/finetune-eval`. If the quality gate failed, explain which criterion tripped and suggest the user revisit `/finetune-plan` first.

## Related skills (auto-loaded when relevant)

- `grader-writing` — the finalize-mode grader this phase locks in.
- `topic-hierarchy` — ongoing reference while records are generated per-topic.
- `pipeline-context` — full pipeline map.
