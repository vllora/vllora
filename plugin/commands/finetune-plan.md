---
name: finetune-plan
description: |
  Build the topic hierarchy + initial grader draft from the knowledge parts
  ingested in /finetune-sources. Produces plan.md — the human-reviewable
  roadmap the user should read before proceeding to generation.
  Triggers: "make a plan", "build topics", "draft grader", "what should we train".
allowed-tools: Bash, Read, Edit
---

# /finetune-plan

Thin narrator for `vllora finetune plan`. Spawns `relation_builder` + `grader_drafter(init)` workers to produce the topic hierarchy, topic-part relations, and the first-draft grader. Emits `plan.md` — the main human artifact of this phase.

## When to use

After `/finetune-sources` succeeds. The user wants to see what topics the pipeline is going to train on and get an initial sense of the grader shape before committing to record generation.

## Preconditions

- `sources` phase must be `done`. Read `pipeline-journal.json` to verify if unsure.

## What this command does

1. Shell out via `Bash`:
   ```bash
   vllora finetune plan
   ```
2. Stream events. Expect `worker_start` / `worker_done` pairs for `relation_builder` and `grader_drafter` (mode `init`), plus a terminal `phase_done`.
3. After success, offer to `Read` `finetune-project/plan.md` and summarise the topic tree + grader approach for the user.
4. Surface the `next` field (should be `"/finetune-generate"`).

## Human review checkpoint

This phase is **the** natural place to pause and confirm with the user. `plan.md` captures:
- The topic hierarchy (Domain → Skill, 2 levels).
- Topic-to-knowledge-part relations (max 15 parts per leaf topic).
- The initial grader draft (in JavaScript — evaluated in a QuickJS sandbox).

If the topics look off (wrong granularity, mixed-up domains, overlapping skills), stop here. `plan.md` is editable — use `Edit` to adjust the YAML/Markdown sections the user wants changed, then re-run `/finetune-plan --force` to regenerate downstream artifacts from the edits.

Do NOT edit the grader draft inline — that's `/finetune-generate`'s job (mode `finalize`) and then `/finetune-eval`'s (mode `refine`). Use `Edit` on `plan.md` only for topic structure and high-level guidance.

## Usage shape

```bash
vllora finetune plan
# inspect plan.md, iterate if needed:
vllora finetune plan --force   # regenerates after user edits
```

## Artifacts produced

- `finetune-project/plan.md` — human-readable plan summary.
- Topics persisted to the gateway's `topics` table.
- Topic-part relations persisted to `relations` table.
- Initial grader version persisted (marked draft, not yet finalized).
- `pipeline-journal.json` → `phases.plan.status: "done"`.

## On failure

- **Exit 2** (precondition): `sources` not done. Tell user to run `/finetune-sources` first.
- **Exit 1** (runtime): worker crashed (usually `max_turns` hit — surface the error message), or the knowledge corpus is too thin for meaningful topic extraction.

If `relation_builder` consistently fails to find links, it often means the user's sources are too diverse or too sparse. Suggest either ingesting more sources or narrowing the workflow objective.

## What happens next

After `phase_done`, the `next` field is `/finetune-generate`. Prompt the user to review `plan.md` first — if they push back on the topics, offer to edit the file and re-run `plan --force`. Once they're happy, suggest `/finetune-generate`.

## Related skills (auto-loaded when relevant)

- `topic-hierarchy` — design rubric for good topics (2-level, behavioral format, avoid "Specialize in:" language).
- `grader-writing` — the grader-draft this phase produces is going to be iterated; context on the grader patterns lives here.
- `pipeline-context` — full pipeline map.
