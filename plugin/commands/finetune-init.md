---
name: finetune-init
description: |
  Scaffold a finetune-project/ workspace and register a workflow on the gateway.
  This is the first command in every new fine-tune — it creates nothing destructive,
  just a project directory and a DB row, and returns a workflow ID.
  Triggers: "init finetune", "start a fine-tune", "scaffold workflow", "begin pipeline".
allowed-tools: Bash, Read
---

# /finetune-init

Thin narrator for `vllora finetune init`. Scaffolds the local `finetune-project/` directory and registers a workflow on the gateway. Always the first step of a new fine-tune. Idempotent on re-run with the same objective — returns the existing workflow ID.

## When to use

Start here whenever the user wants to begin a new fine-tune project. If the user hasn't told you their objective yet, ask for one before invoking this command — `init` requires a non-empty objective.

If a `finetune-project/` directory already exists in the cwd, inspect its `pipeline-journal.json` first via `Read` to decide whether this is a resume (existing workflow) or a fresh start. If unsure, run `/finetune-status` instead.

## Preconditions

None. This is the entry-point command.

## What this command does

1. Ask the user for their training objective (one sentence; stored on the workflow row for traceability).
2. Shell out via `Bash`:
   ```bash
   vllora finetune init "<objective>" [--base-model <name>] [--name <slug>]
   ```
   `--base-model` defaults to `qwen-3.5-2b`. Pass `--base-model qwen-3.5-4b` only when the user has confirmed tool-calling is required (smaller models can't emit native `tool_calls` reliably — see the `nemo-guide` skill for the full rubric).
3. Stream the stdout stream-JSON events to the user as they arrive. The command is fast (<10s) — typically one `progress` event and one terminal `phase_done`.
4. From the terminal `phase_done` event, read:
   - `status` — should be `"done"`; otherwise relay the error.
   - `summary` — usually `"workflow created"` plus the workflow ID.
   - `next` — should be `"/finetune-sources"`. Surface it to the user.

## Usage shape

```bash
vllora finetune init "build a customer-support agent for shoe returns"
# Next: /finetune-sources
```

With non-default flags:

```bash
vllora finetune init "tool-calling agent for retail operations" \
  --base-model qwen-3.5-4b \
  --name tau-retail
```

## Artifacts this command produces

- `finetune-project/` (directory, created idempotently).
- `finetune-project/pipeline-journal.json` (initial state: `current_phase: null`, workflow_id populated).
- One new row in the gateway's `workflows` table.

Use `Read` on `pipeline-journal.json` afterwards if the user wants to see the workflow ID.

## On failure

- **Exit 2** (precondition unmet): the CLI refused because something prerequisite is missing (e.g., `$HOME` unwritable). Relay the stderr message and suggest `/finetune-doctor` (run `vllora doctor` via Bash if the plugin doesn't expose a doctor command yet).
- **Exit 1** (runtime): the CLI hit a transient failure (gateway unreachable, DB I/O). Relay stderr, suggest the user check `vllora doctor`, don't auto-retry.
- Any other non-zero code: treat as runtime, relay stderr verbatim.

Do NOT swallow errors or fall back to writing `pipeline-journal.json` yourself — the plugin has no pipeline logic (parent §9 invariant).

## What happens next

After a successful `phase_done`, the `next` field points at `/finetune-sources`. Ask the user what source material they have (PDFs, OTel traces, pre-built records), then prompt them to run the suggested next command.

If they already have pre-built records in JSONL or Parquet form, suggest `/finetune-import-dataset` instead of `/finetune-sources` — it skips the extraction phase entirely.

## Related skills (auto-loaded when relevant)

- `pipeline-context` — full pipeline map; loaded on any `/finetune-*` turn.
