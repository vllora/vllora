---
name: nemo-guide
description: |
  Use NeMo Data Designer as an alternative record-generation path during
  the `generate` phase — a synthetic-data pipeline that enriches seed rows
  via retrieval + LLM column generators. Covers recipes, the two-stage
  question pattern, rag-retrieval plumbing, preview health gates, and
  common conversion failures.
  Triggers: "NeMo recipe", "NeMo preview failing", "rag-retrieval empty",
  "generating synthetic data with NeMo", "curated seed", "convert NeMo output",
  "NeMo job", "judge columns".
---

# NeMo Data Designer

## Overview

NeMo is a synthetic data generation system: it takes a curated seed (one row per leaf topic, optionally enriched with knowledge-part links), retrieves grounding chunks via the gateway, and runs a pipeline of LLM column generators to produce final training records. It's an alternative to `generate_records.py` for workflows that want retrieval-augmented generation — especially useful for large document corpora where per-topic LLM prompting would lose grounding.

## Architecture

```
seed parquet      ──┐
(topics + maybe     │
 relations)         ▼
                  NeMo preview (10 rows) ── health gate ── NeMo full job
                                                              │
                                                              ▼
                                                    dataset pages (JSON)
                                                              │
                                                              ▼
                                               convert_nemo_rows.py → training.jsonl
                                                                        + relations.json
```

## Core patterns

1. **Topics-only seed mode** (recommended). One row per leaf topic; NeMo retrieves at generation time. No `relations.json` dependency.
2. **Relations seed mode** (legacy). Pre-links topics to knowledge parts in the parquet. Requires `relations.json` already materialized.
3. **Two-stage question generation.** `topic_path → rag-retrieval → retrieved_chunks (broad) → raw_question (diverse, drop=true) → rag-retrieval → question_chunks (question-specific, drop=true) → final system_prompt + user_message`. Avoids anchoring bias while keeping answers grounded.
4. **`rag-retrieval` plugin.** Calls the gateway's search API per row. Config keys: `workflow_id`, `top_k: 15`, `query_field: "topic_path"`, `part_ids_field`, `matches_field`. Emits concatenated chunks + metadata columns for traceability.
5. **Judge columns (RAGAS-aligned).** `judge_answerable` (0/1), `judge_groundedness` (0/0.5/1), `judge_specificity` (0/1). These filter weak rows pre-training. **Not** the training objective — that's still `grader.js`.

## Recipe shape (sketch)

```
execution_type: "preview" | "full"
rows: 10 | <target>
seed_file_id: <from upload-curated response>
columns:
  - name: topic_context
    type: expression
    expr: "{{ topic_path }}"
  - name: retrieved_chunks
    type: rag-retrieval
    config: { workflow_id, top_k: 15, query_field: "topic_path", ... }
  - name: raw_question
    type: llm-text
    drop: true
    prompt: "..."
  - name: user_message
    type: llm-text
    ...
```

## Workflow

```bash
# 1. Materialize seed
uv run finetune-skill/scripts/nemo/materialize_seed.py \
  --topics topics.json --output curated-seed.parquet

# 2. Upload
curl -X POST http://localhost:8000/api/data-recipe/seed/upload-curated \
  -F "file=@curated-seed.parquet" -F "block_id=$(date +%s)"
# capture file_id

# 3. Preview (10 rows, health gate)
curl -X POST http://localhost:8000/api/data-recipe/jobs -d '{ "execution_type": "preview", ... }'
# → poll status, GET /dataset and /analysis, fix recipe if broken

# 4. Full job
# → same POST, execution_type: "full", rows: TARGET
# → page dataset with ?limit=200&offset=0

# 5. Convert
python3 finetune-skill/scripts/nemo/convert_nemo_rows.py \
  --input nemo-dataset-page-1.json \
  --output training.jsonl \
  --ground-truth-field reference_answer \
  --workflow-id $WF_ID \
  --relations-output relations.json \
  --min-answerable 1.0 --min-groundedness 0.5
```

## Anti-patterns

- **"Using `topic_path` directly after `drop: true` columns."** Column context resets; downstream columns can't see dropped ones. Use an `expression` column (`topic_context`) as a persistent passthrough.
- **"Running full job without preview."** Skips the health gate. 100 rows of broken `retrieved_chunks` wastes time. **Always preview first.**
- **"Empty `retrieved_chunks` in preview."** Usually `rag-retrieval` couldn't reach the gateway, or knowledge isn't embedded yet. Check NeMo server logs for `[rag-retrieval] ERROR`; test with a gateway-ping first.
- **"Including metadata in `training.jsonl`."** `convert_nemo_rows.py` auto-detects `judge_*` / `score_*` fields and routes them to a sidecar. If `reference_answer`, `chunk_text`, or `topic_path` sneak into `messages`, validation fails. Run `validate_records.py --nemo` to catch this.
- **"Dropping a column you still need in metadata."** `drop: true` hides the column from the final dataset (not just `training.jsonl`). If you need it downstream, use `drop: false` or create a separate metadata column.

## Converter filters

Rows are filtered out of `training.jsonl` when:
- `judge_answerable < min-answerable` (default 1.0).
- `judge_groundedness < min-groundedness` (default 0.5).
- `reference_answer` is empty or the row failed a required judge column.

Filtered rows still appear in the sidecar for debugging. Inspect rejection counts before deciding whether to relax thresholds.

## When to use NeMo vs `generate_records.py`

- **NeMo**: large corpora (>20 knowledge parts per topic), RAG-heavy tasks, when you want per-row retrieval grounding + judge filtering.
- **`generate_records.py`**: smaller corpora, simpler flows, when topic-specific LLM prompting with pre-linked relations is enough.

Both produce `training.jsonl` that passes `validate_records.py`; downstream phases don't know the difference.

## Related

- `topic-hierarchy` — NeMo consumes `topics.json`; design it well before materializing the seed.
- `grader-writing` — judge columns are filters, the real reward still comes from `grader.js`.
- `pipeline-context` — `generate` phase is where this skill loads.
- Long-form: `vllora/ui/finetune-skill/reference/nemo-guide.md` (508 lines) + `nemo-columns-reference.md` (per-column-type reference).
