---
name: finetune-import-dataset
description: |
  Alternative entry path — skip extraction/planning/generation and import
  pre-built training records directly from a JSONL, Parquet, or HuggingFace
  dataset. Use when the user already has a curated record set.
  Triggers: "import dataset", "use existing records", "skip extraction", "hf dataset".
allowed-tools: Bash, Read
---

# /finetune-import-dataset

Thin narrator for `vllora finetune import-dataset`. Skips the `sources → plan → generate` path and ingests pre-built records straight into the workflow. Every record gets `origin_uri` + `origin_source_id` populated for traceability.

## When to use

The user already has:
- A local `.jsonl` / `.parquet` file in `openai-chat` or `tool-calling` format.
- A HuggingFace dataset reference (`hf://org/dataset[@branch]`).
- Records from another bucket (`s3://` / `gs://` / `azblob://` / `https://`).

If the user has raw material (PDFs, traces) instead, use `/finetune-sources` → `/finetune-plan` → `/finetune-generate` instead.

## Preconditions

- `init` phase must be `done`.
- Phases `sources`, `plan`, `generate` must NOT be `done` — this entry path is mutually exclusive with the generation flow in v0. If the user wants to merge imported records with generated ones, stop and explain: that's a deferred feature (design doc §5.9 "Notes on hybrid flow").

## What this command does

1. Confirm the user's source (path or URI) + suggest the appropriate `--schema` flag if obvious from the file extension.
2. Shell out via `Bash`:
   ```bash
   vllora finetune import-dataset <path-or-uri> [--schema openai-chat|tool-calling]
   ```
3. Stream events. Expect one `progress` (validation) and one terminal `phase_done`.
4. Surface the `next` field (should be `"/finetune-eval"`).

## Usage shape

Local JSONL:

```bash
vllora finetune import-dataset ./training.jsonl
```

HuggingFace dataset with explicit schema:

```bash
vllora finetune import-dataset hf://anthropic/hh-rlhf --schema openai-chat
```

Tool-calling records from S3:

```bash
vllora finetune import-dataset s3://my-bucket/records.parquet --schema tool-calling
```

## What gets validated

The CLI runs `validate_records.py` internally. Common validation failures:
- Missing `messages` field or wrong shape.
- `ground_truth` stringified when it should be `{name, arguments}` dict (tool-calling).
- `tools` field stripped from tool-calling records (CLI refuses — training would teach text-only output).

If validation fails, the CLI exits 1 with a specific error; relay it verbatim.

## Artifacts produced

- Records inserted into the gateway's `records` table with `origin_uri` + `origin_source_id`.
- `pipeline-journal.json` marks `phases.import-dataset.status: "done"`. Because generation is skipped, `phases.sources`, `.plan`, `.generate` remain `null` / unset.

## On failure

- **Exit 2** (precondition unmet): `init` not done, or `sources`/`plan`/`generate` already done (mutual exclusion). Relay stderr, suggest the user `rm -rf finetune-project/` and restart if they meant a fresh import.
- **Exit 1** (runtime): validation failed, URI unreachable, gateway down. Relay stderr. For validation failures, the message usually names a specific row / field — surface that.
- Remote URI auth missing: relay the env-var remediation the CLI provides.

## What happens next

After success, `next` points at `/finetune-eval`. Ask the user to confirm they want to evaluate; the eval phase is compute-bound (5–15 min per iteration) and they may want to inspect the imported records first via the UI or the gateway's `/records` endpoint.

## Related skills (auto-loaded when relevant)

- `pipeline-context` — full pipeline map + the records-vs-knowledge-parts distinction.
- `grader-writing` — the next bottleneck after import: you'll need a grader before eval can run.
