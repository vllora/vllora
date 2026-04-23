---
name: finetune-quickstart
description: |
  Guided first-run wizard — chains init + sources into a single interactive
  session with sensible defaults. Best for first-time users who want a
  narrated walk-through rather than issuing each command manually.
  Triggers: "quickstart", "walk me through", "first time", "guided setup".
allowed-tools: Bash, Read
---

# /finetune-quickstart

Thin narrator for `vllora finetune quickstart`. A wizard that strings together `init` + `sources` with prompts at each decision point. The friendliest entry path for new users; power users prefer to drive `/finetune-init` then `/finetune-sources` explicitly.

## When to use

- First-time users who haven't seen the pipeline before.
- Users who want a narrated walk-through instead of reading docs.
- Users who aren't sure where to start or what flags to pass.

If the user has already run `/finetune-init` in the cwd, suggest `/finetune-sources` directly instead — `quickstart` will detect the existing workflow and skip re-init, but the narration is redundant at that point.

## Preconditions

None. The wizard handles every case: fresh cwd, partially initialised cwd, and fully set up cwd.

## What this command does

1. Shell out via `Bash`:
   ```bash
   vllora finetune quickstart
   ```
2. The wizard is interactive — the CLI prompts via stdin for:
   - Objective (one sentence).
   - Base model (default `qwen-3.5-2b`; confirmed only if tool-calling is implied).
   - Source type (PDFs, OTel traces, pre-built records, or all).
   - Source paths / URIs.
3. Stream stdout events. The CLI emits `progress` events for each substep (init, then sources). Expect a single terminal `phase_done`.
4. Surface the `next` field (should be `"/finetune-plan"` after a full happy-path run).

## Interactive prompts + plugin narration

Because the CLI prompts via stdin, the plugin's job is to:
- Relay each CLI prompt to the user.
- Pass the user's reply back to the CLI's stdin.
- Summarise and proceed.

In a Claude Code session the main thread handles this stdin bridging naturally — just don't pre-fill answers the user hasn't given you.

If the user pastes a long answer (e.g., a multi-line objective or a URI list), preserve whitespace and newlines when passing to stdin.

## Usage shape

```bash
vllora finetune quickstart
# Follows the wizard prompts; equivalent to running init + sources manually.
```

## Non-interactive mode

For CI or scripted workflows, use `init` + `sources` directly with flags — don't try to script `quickstart`; the wizard assumes interactive stdin.

## Artifacts produced

Everything `init` + `sources` would produce:
- `finetune-project/pipeline-journal.json` with `init` + `sources` marked `done`.
- Workflow row in the gateway.
- Knowledge parts ingested from the chosen sources.

## On failure

- **Exit 130** (user cancelled): wizard exited mid-prompt. Partial artifacts may exist; running `/finetune-status` shows which phase completed.
- **Exit 1** (runtime): one of the substeps failed (usually sources — source resolution or extractor crash). The CLI tells you which substep; relay it. Suggest re-running `/finetune-sources` directly once the cause is fixed.
- **Exit 2** (precondition): shouldn't happen (wizard has no preconditions), but if it does, `vllora doctor` usually tells you why.

## What happens next

After success, the user is ready for `/finetune-plan`. Summarise what `quickstart` accomplished — workflow ID, document count — then prompt to proceed.

If the user chose the pre-built-records path in the wizard, `quickstart` shells to `/finetune-import-dataset` internally and the `next` field will instead be `/finetune-eval`.

## Related skills (auto-loaded when relevant)

- `pipeline-context` — full pipeline map, especially useful after quickstart.
