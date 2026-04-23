---
name: finetune-sources
description: |
  Ingest source material (PDFs, OTel traces, or URIs) into the workflow.
  Spawns one knowledge_extractor worker per document and uploads extracted
  knowledge parts to the gateway. Typically the longest-running phase — plan
  for minutes, not seconds.
  Triggers: "ingest sources", "add pdfs", "extract documents", "process traces".
allowed-tools: Bash, Read
---

# /finetune-sources

Thin narrator for `vllora finetune sources`. Resolves source paths / URIs to a local cache, then spawns extractor workers in parallel to build a corpus of knowledge parts. First LLM-heavy phase of the pipeline.

## When to use

After `/finetune-init` succeeds. The user has PDFs, OTel trace bundles, or remote URIs and wants them ingested into the workflow's knowledge store.

If the user already has pre-built training records (JSONL / Parquet / HF dataset), suggest `/finetune-import-dataset` instead — that path skips extraction entirely.

## Preconditions

- `init` phase must be `done`. Read `finetune-project/pipeline-journal.json` first if unsure; the `phases.init.status` field must be `"done"`.
- Source inputs present: either local file paths or URIs in a supported scheme (`file://`, `hf://`, `s3://`, `gs://`, `azblob://`, `https://`).

## What this command does

1. Confirm with the user which sources they want to ingest if not already stated.
2. Shell out via `Bash`:
   ```bash
   vllora finetune sources <path-or-uri> [<path-or-uri> ...] [--parallel <N>] [--cache-dir <path>]
   ```
   `--parallel` defaults to 12 (one extractor worker per PDF, bounded). `--cache-dir` defaults to `~/.vllora/cache/sources/`.
3. Stream stdout events to the user. Expect `progress`, `worker_start`, `worker_done` (one pair per document), and a terminal `phase_done`.
4. Surface the `next` field from the terminal event (should be `"/finetune-plan"`).

## Usage shape

Local PDFs:

```bash
vllora finetune sources ./pdfs/
```

Mixed local + remote:

```bash
vllora finetune sources ./pdfs/ hf://anthropic/hh-rlhf s3://my-bucket/runbooks/
```

With bounded parallelism (useful on small machines):

```bash
vllora finetune sources ./pdfs/ --parallel 4
```

## Duration expectations

- Local PDFs: ~30–60s per document with default parallelism; a 20-PDF batch takes ~2–5 min.
- Remote URIs: add download time (cache hit on re-run).
- OTel trace bundles: ~10–30s per bundle.

Don't interrupt the user with progress-every-event noise; summarise every 30s or at each `worker_done`.

## Artifacts produced

- Knowledge parts persisted to the gateway's `knowledge_parts` table with `origin_uri` populated.
- Cached source content under `~/.vllora/cache/sources/<scheme>/<hash>/`.
- `pipeline-journal.json` updated: `phases.sources.status: "done"`.

## On failure

- **Exit 2** (precondition): `init` not done, or cwd missing `finetune-project/`. Tell the user the missing prior phase.
- **Exit 1** (runtime): extractor worker crashed, gateway unreachable, or URI adapter auth missing. Relay stderr verbatim. Common cause: provider credentials env var not set for remote URIs — run `vllora doctor` via Bash to confirm.
- **Exit 130** (user cancelled): user sent SIGINT; some workers may have persisted partial results. Don't auto-retry.

Don't fall back to inline extraction — the plugin has no pipeline logic (parent §9).

## What happens next

After `phase_done`, the `next` field points at `/finetune-plan`. Ask whether the user is ready to proceed, or wants to inspect the extracted parts first (they can `cat finetune-project/plan.md` after plan, or query the gateway directly).

## Related skills (auto-loaded when relevant)

- `pipeline-context` — full pipeline map.
- `topic-hierarchy` — auto-loaded mid-`plan` phase, but the concepts introduced here (knowledge parts, source linking) feed directly into topic design.
