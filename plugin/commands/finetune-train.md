---
description: |
  GRPO training; long-running training_monitor; analyze on completion.
  Triggers: "train the model", "start training".
  Preconditions: eval readiness=pass.
allowed-tools: Bash
---

# /finetune-train

Track: C | Feature: 004-claude-code-plugin | Parent design: §7.3.2 thin-command template

<!-- TODO [C]: flesh out per parent §7.3.2 — sections below are stubs. -->

## When to use
GRPO training; long-running training_monitor; analyze on completion. Preconditions: eval readiness=pass.

## Steps
1. Verify preconditions via `vllora finetune status`.
2. Shell out: `vllora finetune train`
3. Stream stdout to user (worker progress events).
4. Interpret final output:
   - Success → echo "Next: /finetune-<next-verb>" from CLI.
   - Failure → relay stderr, suggest `/finetune-status` to diagnose.
5. Do NOT auto-invoke the next command — user drives (parent §2.3.2).

## Error handling
- CLI exit 2 (precondition) → tell user the required prior phase.
- CLI exit 3 (gate fail) → explain + suggest fix.
- CLI exit 1 (generic) → relay stderr + suggest `/finetune-status`.

## Related skills
- `pipeline-context` (auto-loaded)
- Others per phase — add as needed.
