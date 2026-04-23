---
name: finetune-train
description: |
  Submit GRPO training on the gateway and monitor the run. Spawns a long-running
  training_monitor worker that reports metrics, detects anomalies, and writes a
  monitor-report-{N}.md summary. Duration: 30 min to 3 hours.
  Triggers: "train the model", "start training", "run grpo", "finetune it".
allowed-tools: Bash, Read
---

# /finetune-train

Thin narrator for `vllora finetune train`. Submits a GRPO training job + spawns the training monitor. The longest-running phase of the pipeline — don't block the user while it runs. Outputs an adapter ID when done.

## When to use

After `/finetune-eval` succeeds with `readiness=pass`. Confirm with the user explicitly before invoking — training is expensive in both time (hours) and compute.

## Preconditions

- `eval` phase `done` with `readiness: "pass"`. Refuse to proceed otherwise; training requires the readiness gate (parent §9 invariant — non-negotiable).

## What this command does

1. Confirm with the user: "This will run GRPO training on the 4B model for an expected 30 min to 3 hours. Continue?" Stop if they decline.
2. Shell out via `Bash`:
   ```bash
   vllora finetune train [--config <path.yaml>]
   ```
   `--config` lets the user override GRPO hyperparameters (lr, β, group size K, etc.). Default config is a good starting point — the legacy reference at `finetune-skill/reference/training-metrics-guide.md` has the full rubric for when to override.
3. Stream events. Expect many `progress` events (step counters during training) and a terminal `phase_done` carrying the trained adapter ID in `summary`.
4. While training runs, offer to summarise the monitor report every ~5 min via `Read` on `finetune-project/monitor-report-{N}.md` (updated by the `training_monitor` worker).

## Monitoring during the run

The CLI spawns a long-running `training_monitor` worker that polls gateway metrics and writes `monitor-report-{N}.md` incrementally. Good moments to `Read` it:
- Every ~5 min for status.
- When the user asks "how's training going?".
- When an anomaly fires (the monitor reports reward-saturation, KL blow-up, clipping spikes).

If the monitor flags a real anomaly (not a spurious β=0 warning), explain it to the user and ask whether to cancel.

## Usage shape

```bash
vllora finetune train
# Override hyperparameters if the user has a reason:
vllora finetune train --config ./custom-grpo.yaml
```

## Duration expectations

- Small record set (< 500 records, 0.8B base): ~30 min.
- Medium (500–2000 records, 2B base): ~1–2 hr.
- Large (2000+ records, 4B base): ~2–3 hr.

Training is auto-cancelled by the CLI if clipping exceeds safety thresholds — the CLI will emit a clear error event explaining the cause.

## Artifacts produced

- GRPO job row in the gateway's `training_jobs` table.
- Metrics streamed to `training_metrics`.
- `monitor-report-1.md` (and subsequent rounds on iteration).
- Adapter weights uploaded by the gateway at completion; the ID is in the terminal event's `summary`.
- `pipeline-journal.json` → `phases.train.status: "done"` with the adapter ID in `fields.adapter_id`.

## On failure

- **Exit 2** (precondition): `eval` not `done` or `readiness != pass`. Refuse and explain.
- **Exit 1** (runtime): job submission failed, monitor crashed, or auto-cancel fired. Relay stderr.
- **User cancellation** (Ctrl-C): CLI sends SIGTERM to the monitor; some metrics may not flush. The training job on the gateway may continue unless the user also calls `vllora finetune train stop --job-id <id>`.

## Auto-cancel conditions

The CLI's `training_monitor` auto-cancels the run on:
- Clipping fraction > threshold for N consecutive steps (see `diagnose-clipping` doc).
- Reward saturation (all prompts scoring identical) — usually a grader bug.
- KL blow-up (β≠0 runs only).

On auto-cancel, explain the diagnostic to the user and suggest the next action (usually back to `/finetune-eval` with a refined grader).

## What happens next

After success, the pipeline is complete for this round. Suggest the user:
- Deploy the adapter via `vllora finetune` commands or the cloud UI.
- OR iterate: tune hyperparameters and re-run `/finetune-train` with a new config.

## Related skills (auto-loaded when relevant)

- `readiness-gate` — contextualises the pass that got us here + interprets any auto-cancel root cause.
- `pipeline-context` — full pipeline map.
- Long-form reference for GRPO config tuning: `finetune-skill/reference/training-metrics-guide.md`.
