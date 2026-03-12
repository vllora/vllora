# Finetune API Reference

Complete reference for the vLLora Gateway API endpoints used by the finetune pipeline.

**Base URL**: `http://localhost:9090` (gateway default)

All endpoints accept `Authorization: Bearer <token>` header when auth is enabled.

---

## Endpoint Overview

| # | Method | Endpoint | Description |
|---|--------|----------|-------------|
| **Workflows** | | | |
| 1 | GET | `/finetune/workflows` | List all workflows |
| 2 | POST | `/finetune/workflows` | Create workflow |
| 3 | GET | `/finetune/workflows/{workflow_id}` | Get workflow |
| 4 | PUT | `/finetune/workflows/{workflow_id}` | Update workflow |
| 5 | DELETE | `/finetune/workflows/{workflow_id}` | Soft delete workflow |
| **Records** | | | |
| 6 | GET | `/finetune/workflows/{workflow_id}/records` | List records |
| 7 | POST | `/finetune/workflows/{workflow_id}/records` | Add records |
| 8 | PUT | `/finetune/workflows/{workflow_id}/records` | Replace all records |
| 9 | DELETE | `/finetune/workflows/{workflow_id}/records` | Delete all records |
| 10 | PATCH | `/finetune/workflows/{workflow_id}/records/{record_id}` | Update record topic |
| 11 | DELETE | `/finetune/workflows/{workflow_id}/records/{record_id}` | Delete record |
| 12 | PATCH | `/finetune/workflows/{workflow_id}/records/{record_id}/data` | Update record data |
| 13 | PATCH | `/finetune/workflows/{workflow_id}/records/{record_id}/scores` | Update record scores |
| 14 | PATCH | `/finetune/workflows/{workflow_id}/records/topics` | Batch update topics |
| 15 | DELETE | `/finetune/workflows/{workflow_id}/records/topics` | Clear all topics |
| 16 | DELETE | `/finetune/workflows/{workflow_id}/records/topics/{topic_name}` | Clear specific topic |
| 17 | PATCH | `/finetune/workflows/{workflow_id}/records/rename-topic` | Rename topic |
| **Topics** | | | |
| 18 | GET | `/finetune/workflows/{workflow_id}/topics` | List topics |
| 19 | POST | `/finetune/workflows/{workflow_id}/topics` | Create topics |
| 20 | DELETE | `/finetune/workflows/{workflow_id}/topics` | Delete all topics |
| 21 | POST | `/finetune/workflows/{workflow_id}/topics/generate` | Generate topics (AI) |
| **Knowledge Sources** | | | |
| 22 | GET | `/finetune/workflows/{workflow_id}/knowledge` | List knowledge sources |
| 23 | POST | `/finetune/workflows/{workflow_id}/knowledge` | Create knowledge source |
| 24 | DELETE | `/finetune/workflows/{workflow_id}/knowledge` | Delete all knowledge sources |
| 25 | GET | `/finetune/workflows/{workflow_id}/knowledge/count` | Count knowledge sources |
| 26 | GET | `/finetune/workflows/{workflow_id}/knowledge/{ks_id}` | Get knowledge source |
| 27 | DELETE | `/finetune/workflows/{workflow_id}/knowledge/{ks_id}` | Delete knowledge source |
| 28 | PATCH | `/finetune/workflows/{workflow_id}/knowledge/{ks_id}/status` | Update status |
| 29 | PATCH | `/finetune/workflows/{workflow_id}/knowledge/{ks_id}/chunks` | Update chunks |
| 30 | POST | `/finetune/workflows/{workflow_id}/knowledge/chunk` | Chunk knowledge (AI) |
| 31 | POST | `/finetune/workflows/{workflow_id}/knowledge/trace` | Create knowledge trace |
| 32 | DELETE | `/finetune/workflows/{workflow_id}/knowledge/trace/{trace_id}` | Delete knowledge trace |
| **Eval Jobs** | | | |
| 33 | GET | `/finetune/workflows/{workflow_id}/eval-jobs` | List eval jobs (workflow) |
| 34 | POST | `/finetune/workflows/{workflow_id}/eval-jobs` | Create eval job |
| 35 | DELETE | `/finetune/workflows/{workflow_id}/eval-jobs` | Delete all eval jobs |
| 36 | GET | `/finetune/workflows/{workflow_id}/eval-jobs/{job_id}` | Get eval job |
| 37 | PATCH | `/finetune/workflows/{workflow_id}/eval-jobs/{job_id}` | Update eval job |
| 38 | DELETE | `/finetune/workflows/{workflow_id}/eval-jobs/{job_id}` | Delete eval job |
| 39 | GET | `/finetune/eval-jobs?status=running` | List eval jobs by status (cross-workflow) |
| **Dataset & Evaluator** | | | |
| 40 | POST | `/finetune/datasets` | Upload dataset (multipart) |
| 41 | POST | `/finetune/datasets/analytics/dry-run` | Dataset analytics dry run |
| 42 | GET | `/finetune/datasets/{dataset_id}/analytics` | Get dataset analytics |
| 43 | PATCH | `/finetune/workflows/{workflow_id}/evaluator` | Update evaluator |
| 44 | GET | `/finetune/workflows/{workflow_id}/evaluator/versions` | Evaluator version history |
| 45 | POST | `/finetune/workflows/{workflow_id}/evaluator/run` | Run evaluator (placeholder) |
| 46 | GET | `/finetune/workflows/{workflow_id}/evaluator/run/status` | Evaluator run status (placeholder) |
| **Dataset Generation** | | | |
| 47 | POST | `/finetune/workflows/{workflow_id}/dataset/generate` | Generate dataset (placeholder) |
| 48 | POST | `/finetune/workflows/{workflow_id}/dataset/generate/status` | Generation status (placeholder) |
| **Evaluation Runs** | | | |
| 49 | POST | `/finetune/evaluations` | Create evaluation run |
| 50 | GET | `/finetune/evaluations/{evaluation_run_id}` | Get evaluation results |
| **Training Jobs** | | | |
| 51 | POST | `/finetune/workflows/{workflow_id}/jobs` | Create training job |
| 52 | GET | `/finetune/workflows/{workflow_id}/jobs` | List training jobs |
| 53 | GET | `/finetune/workflows/{workflow_id}/jobs/{job_id}/status` | Get job status |
| 54 | GET | `/finetune/workflows/{workflow_id}/jobs/{job_id}/metrics` | Get training metrics |
| 55 | POST | `/finetune/workflows/{workflow_id}/jobs/{job_id}/cancel` | Cancel job |
| 56 | POST | `/finetune/workflows/{workflow_id}/jobs/{job_id}/resume` | Resume job |
| 57 | GET | `/finetune/workflows/{workflow_id}/jobs/{job_id}/weights/url` | Download weights URL |
| **Finetune Evaluations** | | | |
| 58 | GET | `/finetune/datasets/{dataset_id}/finetune-evaluations` | Per-epoch evaluations |
| **Deployments** | | | |
| 59 | POST | `/finetune/deployments` | Deploy model |
| 60 | DELETE | `/finetune/deployments/{deployment_id}` | Delete deployment |
| **Topic Hierarchy** | | | |
| 61 | POST | `/finetune/topic-hierarchy/generate` | Generate topic hierarchy (AI) |
| 62 | POST | `/finetune/topic-hierarchy/adjust` | Adjust topic hierarchy (AI) |

---

## Table of Contents

1. [Concepts & ID Mapping](#concepts--id-mapping)
2. [Workflows (Datasets)](#workflows-datasets)
3. [Workflow Records](#workflow-records)
4. [Workflow Topics](#workflow-topics)
5. [Knowledge Sources](#knowledge-sources)
6. [Evaluation Jobs (Dry Runs)](#evaluation-jobs-dry-runs)
7. [Dataset Upload & Evaluator](#dataset-upload--evaluator)
8. [Evaluation Runs](#evaluation-runs)
9. [Reinforcement Training Jobs](#reinforcement-training-jobs)
10. [Training Metrics](#training-metrics)
11. [Finetune Evaluations (Per-Epoch)](#finetune-evaluations-per-epoch)
12. [Deployments](#deployments)
13. [Topic Hierarchy Generation](#topic-hierarchy-generation)
14. [Pipeline Flow](#pipeline-flow)

---

## Concepts & ID Mapping

| Frontend Term | Backend Term | Notes |
|---|---|---|
| Dataset | Workflow | A "dataset" in the UI is a "workflow" row in the DB |
| DatasetRecord | WorkflowRecord | Records belong to a workflow |
| DryRunJob / EvalJob | eval_job | Evaluation job tracking |
| datasetId | workflow_id | Same UUID — used interchangeably |
| backendDatasetId | dataset_id (cloud) | ID returned by `POST /finetune/datasets` upload |
| evaluationRunId | cloud_run_id | ID from the cloud evaluation service |

The gateway uses **SQLite** for local persistence. The `state` column on workflows stores a JSON blob containing the full workflow state machine, snapshots, generation history, and eval cache.

---

## Workflows (Datasets)

CRUD for the top-level entity. Each workflow represents one finetune dataset project.

### List Workflows

```
GET /finetune/workflows
```

**Response**: `DbWorkflowResponse[]`

```json
[
  {
    "id": "uuid",
    "name": "Chess Tutor",
    "objective": "Expert chess tutor...",
    "eval_script": "function evaluate(...) { ... }",
    "state": "{...json blob...}",
    "iteration_state": "{...json blob...}",
    "created_at": "2026-03-10T12:00:00Z",
    "updated_at": "2026-03-10T12:00:00Z",
    "deleted_at": null
  }
]
```

### Create Workflow

```
POST /finetune/workflows
Content-Type: application/json

{
  "name": "Chess Tutor",
  "objective": "Expert chess tutor helping students improve"
}
```

**Response**: `DbWorkflowResponse` (the created workflow)

### Get Workflow

```
GET /finetune/workflows/{workflow_id}
```

**Response**: `DbWorkflowResponse`
**404** if not found.

### Update Workflow

```
PUT /finetune/workflows/{workflow_id}
Content-Type: application/json

{
  "name": "Updated Name",
  "objective": "Updated objective",
  "eval_script": "function evaluate(...) { ... }",
  "state": "{...json blob...}"
}
```

All fields are optional — only provided fields are updated.

**Response**: `DbWorkflowResponse`

### Delete Workflow (Soft Delete)

```
DELETE /finetune/workflows/{workflow_id}
```

**Response**: `{ "id": "uuid", "deleted": true }`

---

## Workflow Records

Training data records within a workflow. Each record contains a conversation (messages array) plus optional metadata.

### List Records

```
GET /finetune/workflows/{workflow_id}/records
```

**Response**:
```json
{
  "records": [
    {
      "id": "uuid",
      "workflow_id": "uuid",
      "data": "{\"input\":{\"messages\":[...]},\"output\":{}}",
      "topic": "Openings/Sicilian Defense",
      "span_id": null,
      "is_generated": 0,
      "source_record_id": null,
      "dry_run_score": 0.85,
      "finetune_score": null,
      "metadata": null,
      "created_at": "2026-03-10T12:00:00Z"
    }
  ]
}
```

**Key fields**:
- `data` — JSON string containing the training example (messages format)
- `topic` — Leaf topic from the topic hierarchy
- `is_generated` — `1` if synthetically generated, `0` if human-authored
- `dry_run_score` — Score from evaluation/dry run (0.0–1.0)
- `finetune_score` — Score from training evaluation epochs

### Add Records

```
POST /finetune/workflows/{workflow_id}/records
Content-Type: application/json

{
  "records": [
    {
      "id": "uuid",
      "data": { "input": { "messages": [...] }, "output": {} },
      "topic": "Openings",
      "is_generated": false,
      "source_record_id": null,
      "metadata": null
    }
  ]
}
```

**Response**: `{ "added": 5 }`

### Replace All Records

```
PUT /finetune/workflows/{workflow_id}/records
Content-Type: application/json

{ "records": [...] }
```

Atomically replaces all records for this workflow.

### Delete All Records

```
DELETE /finetune/workflows/{workflow_id}/records
```

**Response**: `{ "deleted": 42 }`

### Update Single Record Topic

```
PATCH /finetune/workflows/{workflow_id}/records/{record_id}
Content-Type: application/json

{ "topic": "Endgames/Rook Endgames" }
```

### Update Record Data

```
PATCH /finetune/workflows/{workflow_id}/records/{record_id}/data
Content-Type: application/json

{ "data": "{\"input\":{\"messages\":[...]},\"output\":{}}" }
```

### Update Record Scores

```
PATCH /finetune/workflows/{workflow_id}/records/{record_id}/scores
Content-Type: application/json

{
  "dry_run_score": 0.92,
  "finetune_score": 0.88
}
```

**Used by**: Evaluation job completion, training epoch evaluation persistence.

### Delete Single Record

```
DELETE /finetune/workflows/{workflow_id}/records/{record_id}
```

### Batch Update Topics

```
PATCH /finetune/workflows/{workflow_id}/records/topics
Content-Type: application/json

{
  "updates": [
    { "record_id": "uuid1", "topic": "Tactics/Pins" },
    { "record_id": "uuid2", "topic": "Tactics/Forks" }
  ]
}
```

### Rename Topic

```
PATCH /finetune/workflows/{workflow_id}/records/rename-topic
Content-Type: application/json

{
  "old_name": "Tactics",
  "new_name": "Tactical Patterns"
}
```

### Clear All Topics

```
DELETE /finetune/workflows/{workflow_id}/records/topics
```

**Response**: `{ "cleared": 42 }`

### Clear Specific Topic

```
DELETE /finetune/workflows/{workflow_id}/records/topics/{topic_name}
```

**Response**: `{ "cleared": 8 }`

---

## Workflow Topics

Manage the topic hierarchy for categorizing records.

### List Topics

```
GET /finetune/workflows/{workflow_id}/topics
```

### Create Topics

```
POST /finetune/workflows/{workflow_id}/topics
Content-Type: application/json

{ "topics": [...] }
```

### Delete All Topics

```
DELETE /finetune/workflows/{workflow_id}/topics
```

### Generate Topics (AI)

```
POST /finetune/workflows/{workflow_id}/topics/generate
Content-Type: application/json

{ ... }
```

Uses AI to generate a topic hierarchy from the dataset content.

---

## Knowledge Sources

External documents (PDFs, URLs, text) attached to a workflow for context-aware data generation.

### List Knowledge Sources

```
GET /finetune/workflows/{workflow_id}/knowledge
```

**Response**:
```json
{
  "knowledge_sources": [
    {
      "id": "uuid",
      "workflow_id": "uuid",
      "name": "chess-fundamentals.pdf",
      "type": "pdf",
      "content": null,
      "extracted_content": "{...chunks...}",
      "status": "ready",
      "progress": null,
      "created_at": "2026-03-10T12:00:00Z",
      "deleted_at": null
    }
  ]
}
```

**Types**: `pdf`, `url`, `text`, `file`
**Statuses**: `pending`, `extracting`, `chunking`, `ready`, `error`

### Create Knowledge Source

```
POST /finetune/workflows/{workflow_id}/knowledge
Content-Type: application/json

{
  "name": "chess-fundamentals.pdf",
  "type": "pdf",
  "content": "base64-encoded-content..."
}
```

### Get Knowledge Source

```
GET /finetune/workflows/{workflow_id}/knowledge/{ks_id}
```

### Count Knowledge Sources

```
GET /finetune/workflows/{workflow_id}/knowledge/count
```

**Response**: `{ "count": 3 }`

### Update Status

```
PATCH /finetune/workflows/{workflow_id}/knowledge/{ks_id}/status
Content-Type: application/json

{ "status": "ready" }
```

### Update Chunks (Extracted Content)

```
PATCH /finetune/workflows/{workflow_id}/knowledge/{ks_id}/chunks
Content-Type: application/json

{
  "extracted_content": {
    "chunks": [...],
    "metadata": {...}
  }
}
```

### Delete Knowledge Source

```
DELETE /finetune/workflows/{workflow_id}/knowledge/{ks_id}
```

### Delete All Knowledge Sources

```
DELETE /finetune/workflows/{workflow_id}/knowledge
```

**Response**: `{ "deleted": 3 }`

### Chunk Knowledge (AI)

```
POST /finetune/workflows/{workflow_id}/knowledge/chunk
```

### Create Knowledge Trace

```
POST /finetune/workflows/{workflow_id}/knowledge/trace
```

### Delete Knowledge Trace

```
DELETE /finetune/workflows/{workflow_id}/knowledge/trace/{trace_id}
```

---

## Evaluation Jobs (Dry Runs)

Track evaluation job lifecycle. These are local tracking records for cloud evaluation runs.

### List Eval Jobs for Workflow

```
GET /finetune/workflows/{workflow_id}/eval-jobs
```

**Response**:
```json
{
  "jobs": [
    {
      "id": "uuid",
      "workflow_id": "uuid",
      "cloud_run_id": "eval-run-xyz",
      "status": "completed",
      "sample_size": 20,
      "rollout_model": "gpt-4o-mini",
      "error": null,
      "created_at": "2026-03-10T12:00:00Z",
      "updated_at": "2026-03-10T12:05:00Z"
    }
  ]
}
```

**Statuses**: `pending`, `running`, `completed`, `failed`, `cancelled`

### List Eval Jobs by Status (Cross-Workflow)

```
GET /finetune/eval-jobs?status=running
```

Returns all eval jobs matching the given status across all workflows. Used by the polling manager to resume jobs after page refresh.

### Create Eval Job

```
POST /finetune/workflows/{workflow_id}/eval-jobs
Content-Type: application/json

{
  "cloud_run_id": "eval-run-xyz",
  "sample_size": 20,
  "rollout_model": "gpt-4o-mini"
}
```

### Get Eval Job

```
GET /finetune/workflows/{workflow_id}/eval-jobs/{job_id}
```

### Update Eval Job

```
PATCH /finetune/workflows/{workflow_id}/eval-jobs/{job_id}
Content-Type: application/json

{
  "status": "completed",
  "error": null
}
```

### Delete Eval Job

```
DELETE /finetune/workflows/{workflow_id}/eval-jobs/{job_id}
```

### Delete All Eval Jobs for Workflow

```
DELETE /finetune/workflows/{workflow_id}/eval-jobs
```

**Response**: `{ "deleted": 5 }`

---

## Dataset Upload & Evaluator

Upload training data to the cloud evaluation/training service and manage the evaluator (grader) script.

### Upload Dataset

```
POST /finetune/datasets
Content-Type: multipart/form-data

file: training.jsonl          (required — JSONL training data)
dataset_id: <workflow_id>     (required — links upload to local workflow)
topic_hierarchy: {...}         (optional — JSON topic tree)
eval_script: function(...)...  (optional — JS evaluator script)
evaluator: {...}               (optional — evaluator config JSON)
```

**Response**:
```json
{
  "dataset_id": "ds-cloud-abc123",
  ...
}
```

The returned `dataset_id` is the **cloud/backend dataset ID** — different from the local workflow ID. This ID is used for all subsequent cloud operations (evaluations, training jobs).

### Update Evaluator Script

```
PATCH /finetune/workflows/{workflow_id}/evaluator
Content-Type: application/json

{
  "evaluator": {
    "type": "js",
    "config": {
      "script": "function evaluate(completion, expected) { ... }"
    }
  }
}
```

Also updates the cloud dataset's evaluator. Automatically creates a new evaluator version.

### Get Evaluator Version History

```
GET /finetune/workflows/{workflow_id}/evaluator/versions
```

**Response**:
```json
[
  {
    "id": "uuid",
    "dataset_id": "ds-cloud-abc123",
    "version": 2,
    "config": {
      "type": "js",
      "config": { "script": "..." }
    },
    "diff": "- old line\n+ new line",
    "created_at": "2026-03-10T12:00:00Z"
  }
]
```

Shows git-style diffs between consecutive evaluator versions.

### Run Evaluator (Placeholder)

```
POST /finetune/workflows/{workflow_id}/evaluator/run
```

Not yet implemented.

### Get Evaluator Run Status (Placeholder)

```
GET /finetune/workflows/{workflow_id}/evaluator/run/status
```

Not yet implemented.

### Generate Dataset (Placeholder)

```
POST /finetune/workflows/{workflow_id}/dataset/generate
```

Not yet implemented.

### Get Dataset Generation Status (Placeholder)

```
POST /finetune/workflows/{workflow_id}/dataset/generate/status
```

Not yet implemented.

### Dataset Analytics (Dry Run)

```
POST /finetune/datasets/analytics/dry-run
Content-Type: application/json

{ "rows": [...] }
```

Runs quality analytics on dataset rows without uploading. Returns quality metrics.

### Get Dataset Analytics

```
GET /finetune/datasets/{dataset_id}/analytics
```

---

## Evaluation Runs

Create and poll cloud evaluation runs. An evaluation runs the configured grader against a subset of records using a rollout model.

### Create Evaluation Run

```
POST /finetune/evaluations
Content-Type: application/json

{
  "dataset_id": "ds-cloud-abc123",
  "rollout_model_params": {
    "model": "gpt-4o-mini",
    "temperature": 0.0
  },
  "offset": 0,
  "limit": 20
}
```

**Response**:
```json
{
  "evaluation_run_id": "eval-run-xyz",
  "status": "running",
  "total_rows": 20
}
```

### Get Evaluation Results

```
GET /finetune/evaluations/{evaluation_run_id}
```

**Response**:
```json
{
  "evaluation_run_id": "eval-run-xyz",
  "status": "completed",
  "total_rows": 20,
  "completed_rows": 20,
  "failed_rows": 0,
  "results": [
    {
      "row_index": 0,
      "row": { "id": "record-uuid", "messages": [...] },
      "epochs": {
        "0": [
          {
            "dataset_row_id": "record-uuid",
            "status": "completed",
            "score": 0.85,
            "reason": "Good response quality",
            "logs": []
          }
        ]
      }
    }
  ],
  "summary": {
    "average_score": 0.72,
    "passed_count": 16,
    "failed_count": 4
  }
}
```

**Polling**: The frontend polls this endpoint every 6 seconds until `status` is `completed` or `failed`. The `DryRunPollingManager` singleton handles this automatically.

---

## Reinforcement Training Jobs

Create and manage reinforcement fine-tuning (GRPO/GSPO) jobs. All training job endpoints are scoped under a workflow.

### Create Training Job

```
POST /finetune/workflows/{workflow_id}/jobs
Content-Type: application/json

{
  "dataset": "ds-cloud-abc123",
  "base_model": "unsloth/Qwen3.5-4B",
  "output_model": "chess-tutor-1710000000",
  "display_name": "Chess Tutor Fine-tune",
  "training_config": {
    "learning_rate": 0.00001,
    "lora_rank": 8,
    "gradient_accumulation_steps": 5,
    "epochs": 2.0,
    "batch_size": 5
  },
  "inference_parameters": {
    "max_output_tokens": 1000,
    "temperature": 1.0,
    "top_p": 1.0,
    "response_candidates_count": 2
  },
  "chunk_size": 100,
  "node_count": 1,
  "evaluator_version": 2
}
```

**Response**: `ReinforcementJob` object:
```json
{
  "id": "uuid",
  "provider_job_id": "ft-abc123",
  "dataset_id": "ds-cloud-abc123",
  "status": "pending",
  "base_model": "unsloth/Qwen3.5-4B",
  "fine_tuned_model": null,
  "provider": "vllora",
  "training_config": {...},
  "created_at": "2026-03-10T12:00:00Z",
  "updated_at": "2026-03-10T12:00:00Z"
}
```

### List Training Jobs

```
GET /finetune/workflows/{workflow_id}/jobs?limit=10&dataset_id=ds-cloud-abc123
```

Query params: `limit`, `after` (pagination cursor), `dataset_id` (filter).

**Response**: `ReinforcementJob[]`

### Get Training Job Status

```
GET /finetune/workflows/{workflow_id}/jobs/{job_id}/status
```

**Statuses**: `pending`, `running`, `succeeded`, `failed`, `cancelled`

### Cancel Training Job

```
POST /finetune/workflows/{workflow_id}/jobs/{job_id}/cancel
```

### Resume Training Job

```
POST /finetune/workflows/{workflow_id}/jobs/{job_id}/resume
```

### Download Trained Weights

```
GET /finetune/workflows/{workflow_id}/jobs/{job_id}/weights/url
```

**Response**:
```json
{
  "download_url": "https://storage.example.com/weights/...",
  "expires_at": "2026-03-10T13:00:00Z"
}
```

Returns a signed URL to download the trained LoRA adapter weights.

---

## Training Metrics

Real-time training metrics from GRPO/GSPO reinforcement fine-tuning.

### Get Training Metrics

```
GET /finetune/workflows/{workflow_id}/jobs/{job_id}/metrics
```

**Response**:
```json
{
  "provider_job_id": "ft-abc123",
  "metrics": [
    {
      "metrics": {
        "global_step": 100,
        "max_steps": 500,
        "epoch": 0.4,
        "learning_rate": 0.00001,
        "reward": 0.65,
        "reward_std": 0.15,
        "loss": 0.42,
        "grad_norm": 1.2,
        "kl": 0.05,
        "completion_length": 150,
        "rewards/vllora_reward_fn/mean": 0.65,
        "rewards/vllora_reward_fn/std": 0.15,
        "completions/mean_length": 150,
        "completions/clipped_ratio": 0.02
      },
      "created_at": "2026-03-10T12:10:00Z"
    }
  ]
}
```

**Metric categories**:
- **Progress**: `global_step`, `max_steps`, `epoch`, `learning_rate`
- **Reward Quality**: `reward`, `reward_std`, `frac_reward_zero_std`, `rewards/vllora_reward_fn/mean`
- **Optimization**: `loss`, `grad_norm`, `kl`
- **Clipping**: `clip_ratio/low_mean`, `clip_ratio/high_max`, `clip_ratio/region_mean`
- **Generation**: `completion_length`, `completions/mean_length`, `completions/clipped_ratio`

---

## Finetune Evaluations (Per-Epoch)

View how the model performs on each record across training epochs. Different from "evaluation runs" — these show training-time evaluation results.

### Get Finetune Evaluations

```
GET /finetune/datasets/{dataset_id}/finetune-evaluations?finetune_job_id=ft-abc123&epoch=2
```

Query params: `finetune_job_id`, `row_index`, `epoch` (all optional filters).

**Response**:
```json
{
  "results": [
    {
      "row_index": 0,
      "row": { "id": "record-uuid", "messages": [...] },
      "epochs": {
        "0": [{ "score": 0.3, "reason": "...", "status": "completed" }],
        "1": [{ "score": 0.6, "reason": "...", "status": "completed" }],
        "2": [{ "score": 0.85, "reason": "...", "status": "completed" }]
      }
    }
  ]
}
```

Shows score progression across epochs — useful for detecting overfitting, stalling, or learning curves.

---

## Deployments

Deploy a fine-tuned model for inference.

### Deploy Model

```
POST /finetune/deployments
Content-Type: application/json

{ ... }
```

### Delete Deployment

```
DELETE /finetune/deployments/{deployment_id}
```

---

## Topic Hierarchy Generation

AI-powered topic hierarchy generation and adjustment.

### Generate Topic Hierarchy

```
POST /finetune/topic-hierarchy/generate
Content-Type: application/json

{ ... }
```

### Adjust Topic Hierarchy

```
POST /finetune/topic-hierarchy/adjust
Content-Type: application/json

{ ... }
```

---

## Pipeline Flow

The finetune pipeline consists of 7 steps. Here's which APIs are used at each step:

### Step 1: Topics Config

**Purpose**: Define the topic hierarchy for organizing training data.

| Action | API |
|---|---|
| Generate hierarchy (AI) | `POST /finetune/topic-hierarchy/generate` |
| Adjust hierarchy | `POST /finetune/topic-hierarchy/adjust` |
| Save topics | `POST /finetune/workflows/{id}/topics` |

### Step 2: Categorization

**Purpose**: Classify each record into a leaf topic.

| Action | API |
|---|---|
| Read records | `GET /finetune/workflows/{id}/records` |
| Assign topics (batch) | `PATCH /finetune/workflows/{id}/records/topics` |
| Assign single topic | `PATCH /finetune/workflows/{id}/records/{record_id}` |

### Step 3: Coverage & Generation

**Purpose**: Identify gaps in topic coverage, generate synthetic records.

| Action | API |
|---|---|
| Get records + topics | `GET /finetune/workflows/{id}/records` |
| Add generated records | `POST /finetune/workflows/{id}/records` |
| Attach knowledge sources | `POST /finetune/workflows/{id}/knowledge` |
| Generate dataset (AI) | `POST /finetune/workflows/{id}/dataset/generate` |
| Check generation status | `POST /finetune/workflows/{id}/dataset/generate/status` |

### Step 4: Grader Config

**Purpose**: Write and configure the JavaScript evaluator (grader) script.

| Action | API |
|---|---|
| Save eval script locally | `PUT /finetune/workflows/{id}` (with `eval_script`) |
| Update cloud evaluator | `PATCH /finetune/workflows/{id}/evaluator` |
| View version history | `GET /finetune/workflows/{id}/evaluator/versions` |

### Step 5: Evaluation (Dry Run)

**Purpose**: Test the grader on a sample of records to validate data quality before training.

| Action | API |
|---|---|
| Upload dataset to cloud | `POST /finetune/datasets` |
| Start evaluation run | `POST /finetune/evaluations` |
| Poll for results | `GET /finetune/evaluations/{eval_run_id}` (every 6s) |
| Track job locally | `POST /finetune/workflows/{id}/eval-jobs` |
| Update job status | `PATCH /finetune/workflows/{id}/eval-jobs/{job_id}` |
| Persist scores to records | `PATCH /finetune/workflows/{id}/records/{record_id}/scores` |

### Step 6: Training

**Purpose**: Run reinforcement fine-tuning on the dataset.

| Action | API |
|---|---|
| Check existing jobs | `GET /finetune/workflows/{id}/jobs?dataset_id=X` |
| Create training job | `POST /finetune/workflows/{id}/jobs` |
| Poll job status | `GET /finetune/workflows/{id}/jobs/{job_id}/status` |
| Get training metrics | `GET /finetune/workflows/{id}/jobs/{job_id}/metrics` |
| View per-epoch evals | `GET /finetune/datasets/{dataset_id}/finetune-evaluations` |
| Cancel/Resume job | `POST /finetune/workflows/{id}/jobs/{job_id}/cancel` or `/resume` |

### Step 7: Deployment

**Purpose**: Deploy the trained model and download weights.

| Action | API |
|---|---|
| Download weights URL | `GET /finetune/workflows/{id}/jobs/{job_id}/weights/url` |
| Deploy model | `POST /finetune/deployments` |
| Delete deployment | `DELETE /finetune/deployments/{id}` |

---

## Default Configuration Values

### Training Config

```json
{
  "learning_rate": 0.00001,
  "lora_rank": 8,
  "gradient_accumulation_steps": 5,
  "epochs": 2.0,
  "batch_size": 5
}
```

### Inference Parameters

```json
{
  "max_output_tokens": 1000,
  "temperature": 1.0,
  "top_p": 1.0,
  "response_candidates_count": 2
}
```

### Default Base Model

`unsloth/Qwen3.5-4B`

---

## Error Handling

All endpoints return standard HTTP status codes:

| Status | Meaning |
|---|---|
| 200 | Success |
| 201 | Created |
| 204 | No content (empty response) |
| 400 | Bad request (validation error) |
| 401 | Unauthorized |
| 404 | Not found |
| 500 | Internal server error |

Error response format:
```json
{
  "error": "Human-readable error message"
}
```

Or:
```json
{
  "message": "Human-readable error message"
}
```

---

## Authentication

When a token provider is configured (cloud deployment), all requests include:

```
Authorization: Bearer <token>
```

For local development, authentication is typically disabled and no token is needed.
