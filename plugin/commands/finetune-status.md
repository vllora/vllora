---
name: finetune-status
description: |
  Pure read — show current pipeline state and the recommended next command.
  Safe to call at any point; never mutates. Useful when resuming, when
  troubleshooting, or when the user asks "where are we?".
  Triggers: "status", "where are we", "what's next", "pipeline state".
allowed-tools: Bash, Read
---

# /finetune-status

Thin narrator for `vllora finetune status`. Pure read operation — prints the current phase, per-phase status, and the recommended next command, all from `pipeline-journal.json`.

## When to use

- At the start of any `/finetune-*` session to orient.
- After a crash, cancellation, or long gap to recover context.
- When the user asks what phase they're in or what to run next.
- Whenever another thin verb exits 2 (precondition unmet) and you're not sure what's missing.

## Preconditions

- `init` phase has been run at least once in the cwd (otherwise there's no `finetune-project/` to describe). If absent, suggest `/finetune-init`.

## What this command does

1. Shell out via `Bash`:
   ```bash
   vllora finetune status
   ```
2. Stream the single `status` event from stdout. Unlike other verbs, this one does NOT emit `phase_done` — the terminal event type is literally `status`, carrying:
   - `current_phase` (string or null): what's running, or null if idle.
   - `phases` (map): each known phase's `status` + iteration count.
   - `next_command`: the suggested slash command the user should run next.
3. Summarise for the user in one sentence. Surface `next_command` verbatim.

## Usage shape

```bash
vllora finetune status
```

## Example output (paraphrased)

```
Workflow: <uuid>
Init:        done
Sources:     done (14 documents)
Plan:        done
Generate:    done (quality_gate: pass, 240 records)
Eval:        iteration 3/5, readiness: fail (grader root cause)
Train:       pending

Next: /finetune-eval
```

## Artifacts consulted (read-only)

- `finetune-project/pipeline-journal.json` (local mirror).
- `workflows.pipeline_journal` on the gateway (server-side mirror for cross-machine resume).

This command does NOT write anywhere — it's safe to call as often as needed.

## On failure

- **Exit 2** (precondition): `init` not run. The CLI's error message says so; suggest `/finetune-init`.
- **Exit 1** (runtime): journal file corrupt, or gateway unreachable when trying to sync from the server mirror. Relay stderr. Journal corruption usually means a crash mid-write; the atomic-write invariant means the file should always be valid, so this is rare — if it happens, suggest `vllora doctor`.

## What happens next

After every call, surface the `next_command` field to the user. If the user asks to advance, they should run that command; don't auto-invoke it.

If they're confused by the state (e.g., "why is eval still iterating?"), offer to `Read` the relevant artifact:
- `change-log.md` to see the grader refinement trail.
- `plan.md` to inspect topics.
- `analysis.json` (via gateway UI or raw file) to see cross-phase reasoning.

## Related skills (auto-loaded when relevant)

- `pipeline-context` — the phase ordering + what each phase is supposed to produce.
