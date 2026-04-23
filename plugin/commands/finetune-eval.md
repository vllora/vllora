---
name: finetune-eval
description: |
  Run the readiness-gate evaluation on the 4B and 0.8B base models. Iterates
  automatically when the grader fails — spawns grader_drafter(refine) and
  retries. The readiness verdict (pass/fail) controls whether training can start.
  Triggers: "evaluate", "readiness check", "dry run", "check the grader".
allowed-tools: Bash, Read, Edit
---

# /finetune-eval

Thin narrator for `vllora finetune eval`. Runs the pre-training readiness gate on the base models and iterates the grader when it detects a grader-root-cause failure. Outputs a pass/fail readiness verdict — training cannot start without a pass.

## When to use

After `/finetune-generate` succeeds with a passing quality gate — OR after `/finetune-import-dataset` succeeds, if the user is on the import path.

The user should plan for multiple iterations. First-run failures are normal and useful: the eval phase is where most grader bugs surface.

## Preconditions

- Records exist (generated or imported).
- Grader is `finalized` (done by `/finetune-generate` or, on the import path, `vllora finetune grader generate` as a prerequisite — surface this requirement if missing).
- `generate` phase `done` with `quality_gate: pass`, OR `import-dataset` phase `done`.

## What this command does

1. Shell out via `Bash`:
   ```bash
   vllora finetune eval [--max-iterations <N>]
   ```
   `--max-iterations` defaults to 5. Override when the user wants tighter budgets.
2. Stream events. Expect `worker_iteration` events (one per eval round) with `outcome: pass|fail|inconclusive` and metrics (`readiness_score`, `avg_score`), plus a terminal `phase_done` carrying the final readiness verdict.
3. On `readiness=pass`, surface `/finetune-train` as the next step.
4. On `readiness=fail`, do NOT advance — parse the `root_cause` from the event, explain it to the user, and suggest what to do (usually: read the refined `change-log.md` entry the CLI just wrote, then re-eval or revisit `plan.md`).

## The iteration loop (happens inside the CLI)

For each iteration, up to `--max-iterations`:
1. Run eval on the 4B model (required) and 0.8B (sanity check).
2. Poll until terminal.
3. If `readiness=fail` AND root cause is grader-related, spawn `grader_drafter(refine)` with the failure signals (score distribution, reason patterns).
4. Write the new grader version + a `change-log.md` entry.
5. Loop.

If the loop exits without a pass, the phase ends with `readiness=fail` + a summarised root cause.

## Usage shape

```bash
vllora finetune eval
# If readiness fails with a DATA root cause, revisit the prior phases:
vllora finetune plan --force
vllora finetune generate
vllora finetune eval
```

## Duration expectations

- Per iteration: ~5–15 min (dominated by model inference on the 4B).
- 5-iteration budget: worst-case 60–75 min.

## Artifacts produced

- Eval run rows in the gateway (one per iteration).
- New grader versions in `graders` table + per-refinement entries in `change-log.md`.
- `pipeline-journal.json` → `phases.eval.status: "done"`; `phases.eval.iteration: N`; `phases.eval.fields.readiness: "pass"|"fail"`.

## On failure (distinct from `readiness=fail`)

- **Exit 2** (precondition): records or grader missing. Relay stderr.
- **Exit 1** (runtime): eval worker crashed, gateway down, or the CLI auto-cancelled the run for safety (e.g., excessive output-token clipping — the CLI provides a remediation message; run `vllora doctor` via Bash if gateway itself looks sick).

`readiness=fail` is NOT a CLI failure — it's a successful eval run with a negative verdict. Don't conflate the two.

## What happens next

- On `readiness=pass`, suggest `/finetune-train` with explicit user confirmation (training is expensive: 30 min – 3 hr).
- On `readiness=fail` with GRADER root cause: the CLI has already refined the grader. Surface the `change-log.md` tail, then suggest re-running eval.
- On `readiness=fail` with DATA root cause: stop and require user action. Suggest revisiting `/finetune-plan` or `/finetune-generate` depending on the signal.

## Related skills (auto-loaded when relevant)

- `readiness-gate` — the 4 hard + 8 soft checks, thresholds, root-cause taxonomy.
- `grader-writing` — patterns and anti-patterns, referenced when the refinement loop needs narration.
- `pipeline-context` — full pipeline map.
