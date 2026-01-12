# Finetune Dataset Management

This package manages finetuning datasets by constructing JSONL files from tracing spans. It provides a wrapper API around the cloud finetuning API, accepting `DatasetWithRecords` and converting it to JSONL format before submitting to the cloud finetuning service.

## Overview

The finetune dataset management system consists of three main components:

1. **Dataset Management** (`ai-gateway/finetune`) - Wrapper API that accepts `DatasetWithRecords`, generates JSONL, and forwards requests to cloud finetuning API
2. **Cloud Finetuning API** (`cloud/finetune`) - Handles the actual finetuning job submission to provider APIs (OpenAI, etc.)
3. **Provider APIs** - External LLM provider APIs (OpenAI, Anthropic, etc.)

## Concepts

### Dataset

A dataset is a collection of training examples derived from tracing spans. Each dataset contains:

- **Original Spans**: Spans captured from actual LLM invocations
- **Generated Spans**: Synthetic or augmented spans that can reference original spans
- **Topic Organization**: Hierarchical topic tree for categorizing examples
- **Evaluation Scores**: Quality metrics for each dataset row
- **Evaluation Prompts**: Topic-specific prompts for assessing data quality

### Dataset Structure

The wrapper API accepts `DatasetWithRecords` which combines dataset metadata with its records:

```typescript
// Dataset metadata (stored separately)
export interface Dataset {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
}

// Individual dataset record with span data
export interface DatasetRecord {
  id: string;
  datasetId: string;           // Foreign key to dataset
  data: Span;                  // Full span data
  spanId?: string;             // For duplicate detection (optional - undefined for generated data)
  topic?: string;              // Topic categorization
  evaluation?: DatasetEvaluation;
  createdAt: number;
}

// Evaluation information for a record
export interface DatasetEvaluation {
  score?: number;              // Quality score (0.0-1.0)
  feedback?: string;           // Evaluation feedback
  evaluatedAt?: number;        // Timestamp of evaluation
}

// Combined view for API (dataset + its records)
export interface DatasetWithRecords extends Dataset {
  records: DatasetRecord[];
}
```

**Note**: The wrapper API currently accepts `DatasetWithRecords` with all records included. In the future, it will accept only `dataset_id` and fetch records internally.

### Topic Tree

Topics are organized hierarchically to allow fine-grained categorization:

```rust
pub struct Topic {
    pub id: Uuid,
    pub dataset_id: Uuid,
    pub parent_id: Option<Uuid>,              // For hierarchical organization
    pub name: String,
    pub description: Option<String>,
    pub evaluation_prompt: Option<String>,     // Topic-specific evaluation prompt
    pub created_at: DateTime<Utc>,
}
```

## Implementation Details

### Span Data Structure

Spans are stored in the `langdb.traces` table with the following schema:

```sql
CREATE TABLE traces (
    trace_id UUID NOT NULL,
    span_id BIGINT NOT NULL,
    parent_span_id BIGINT,
    operation_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    start_time_us BIGINT NOT NULL,
    finish_time_us BIGINT NOT NULL,
    finish_date DATE NOT NULL,
    attribute JSONB DEFAULT '{}',            -- Contains span data (messages, model info, etc.)
    tenant_id TEXT,
    project_id TEXT NOT NULL,
    thread_id TEXT,
    tags JSONB DEFAULT '{}',                  -- User-defined tags/labels
    parent_trace_id UUID,
    run_id UUID,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
    PRIMARY KEY (trace_id, span_id)
);
```

The `attribute` field contains structured JSON data including:
- Model invocation details (`model_call` operation)
- Input/output messages
- Tool calls and results
- Error information
- Performance metrics

### Span Types

Spans are categorized by `operation_name`:
- `run` - Top-level execution context
- `model_call` - LLM model invocations
- `agent` - Agent execution spans
- `task` - Task execution spans
- `tools` - Tool call spans
- `openai`, `anthropic`, `bedrock`, `gemini` - Provider-specific spans
- `cloud_api_invoke`, `api_invoke` - API call spans

### JSONL Generation

The wrapper API converts `DatasetWithRecords` into JSONL format compatible with OpenAI's finetuning API:

```json
{"messages": [{"role": "system", "content": "..."}, {"role": "user", "content": "..."}, {"role": "assistant", "content": "..."}]}
{"messages": [{"role": "user", "content": "..."}, {"role": "assistant", "content": "..."}]}
```

**Conversion Process (Wrapper API):**

1. **Record Processing**: Iterate through `DatasetWithRecords.records`:
   - Each `DatasetRecord` contains full `Span` data in the `data` field
   - Filter records based on evaluation scores (if threshold is set)
   - Respect topic assignments for organization

2. **Message Extraction**: Extract conversation messages from span data:
   - Parse span `attribute` field (contains model call data)
   - Extract `messages` array from model call spans
   - Filter and validate message structure
   - Handle tool calls and function results
   - Convert to OpenAI chat completion format

3. **Data Enrichment**: Add metadata and context:
   - Preserve span ID references (`spanId` field)
   - Include topic information
   - Apply evaluation scores for filtering
   - Maintain thread context

4. **JSONL Serialization**: Convert to JSONL format:
   - One JSON object per line (one per record)
   - Validate against OpenAI format requirements
   - Handle special characters and encoding
   - Generate multipart form data for cloud API

5. **Cloud API Submission**: Forward to cloud finetuning API:
   - Create multipart request with:
     - `config`: Job configuration (JSON)
     - `file`: Generated JSONL file
     - `validation_file`: Optional validation JSONL (if provided)
   - Handle response and error propagation

### Database Schema

**Datasets Table:**
```sql
CREATE TABLE finetune_datasets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'draft',  -- draft, ready, training, completed, failed
    jsonl_file_url TEXT,                    -- S3/storage URL for generated JSONL
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
    FOREIGN KEY (project_id, tenant_id) REFERENCES projects(project_id, tenant_id)
);
```

**Dataset Rows Table:**
```sql
CREATE TABLE finetune_dataset_rows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dataset_id UUID NOT NULL REFERENCES finetune_datasets(id),
    original_span_id TEXT,                  -- References traces.span_id
    generated_span_id TEXT,
    topic_id UUID REFERENCES finetune_topics(id),
    evaluation_score FLOAT,
    messages JSONB NOT NULL,                -- Training messages in OpenAI format
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL
);
```

**Topics Table:**
```sql
CREATE TABLE finetune_topics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dataset_id UUID NOT NULL REFERENCES finetune_datasets(id),
    parent_id UUID REFERENCES finetune_topics(id),
    name TEXT NOT NULL,
    description TEXT,
    evaluation_prompt TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL
);
```

## API Endpoints

### Dataset Management

**Create Dataset**
```
POST /projects/{project_id}/finetune/datasets
Content-Type: application/json

{
  "name": "Customer Support Dataset",
  "description": "Dataset for fine-tuning customer support model"
}
```

**List Datasets**
```
GET /projects/{project_id}/finetune/datasets
```

**Get Dataset**
```
GET /projects/{project_id}/finetune/datasets/{dataset_id}
```

**Add Spans to Dataset**
```
POST /projects/{project_id}/finetune/datasets/{dataset_id}/spans
Content-Type: application/json

{
  "span_ids": ["span_id_1", "span_id_2"],
  "topic_id": "optional_topic_uuid"
}
```

**Generate JSONL**
```
POST /projects/{project_id}/finetune/datasets/{dataset_id}/generate-jsonl
```

**Download JSONL**
```
GET /projects/{project_id}/finetune/datasets/{dataset_id}/jsonl
```

### Topic Management

**Create Topic**
```
POST /projects/{project_id}/finetune/datasets/{dataset_id}/topics
Content-Type: application/json

{
  "name": "Customer Complaints",
  "parent_id": null,
  "description": "Handles customer complaint scenarios",
  "evaluation_prompt": "Evaluate if this conversation..."
}
```

**List Topics**
```
GET /projects/{project_id}/finetune/datasets/{dataset_id}/topics
```

### Finetuning Jobs (Wrapper API)

The wrapper API accepts `DatasetWithRecords` and forwards requests to the cloud finetuning API.

**Submit Finetuning Job**
```
POST /projects/{project_id}/finetune/jobs
Content-Type: application/json

{
  "dataset": {
    "id": "dataset-uuid",
    "name": "Customer Support Dataset",
    "createdAt": 1234567890,
    "updatedAt": 1234567890,
    "records": [
      {
        "id": "record-1",
        "datasetId": "dataset-uuid",
        "data": { /* Span data */ },
        "spanId": "span-123",
        "topic": "Customer Complaints",
        "evaluation": {
          "score": 0.95,
          "feedback": "High quality example",
          "evaluatedAt": 1234567890
        },
        "createdAt": 1234567890
      }
      // ... more records
    ]
  },
  "base_model": "gpt-4o-mini",
  "provider": "openai",
  "hyperparameters": {
    "batch_size": 4,
    "learning_rate_multiplier": 0.1,
    "n_epochs": 3
  },
  "suffix": "customer-support-v1"
}
```

**Future API (dataset_id only):**
```
POST /projects/{project_id}/finetune/jobs
Content-Type: application/json

{
  "dataset_id": "dataset-uuid",
  "base_model": "gpt-4o-mini",
  "provider": "openai",
  "hyperparameters": {
    "batch_size": 4,
    "learning_rate_multiplier": 0.1,
    "n_epochs": 3
  }
}
```

**List Finetuning Jobs**
```
GET /projects/{project_id}/finetune/jobs
```
(Proxies to cloud finetuning API)

**Get Finetuning Job Status**
```
GET /projects/{project_id}/finetune/jobs/{job_id}
```
(Proxies to cloud finetuning API)

## Integration Architecture

### Wrapper API (ai-gateway/finetune)

The wrapper API in `ai-gateway/finetune` provides a higher-level interface that:
- Accepts `DatasetWithRecords` with full span data
- Extracts training messages from span data
- Generates JSONL format
- Calls the cloud finetuning API

**Wrapper API Flow:**
```rust
// 1. Receive DatasetWithRecords
let dataset_with_records: DatasetWithRecords = request.into_inner();

// 2. Extract messages from span data
let jsonl_data = generate_jsonl_from_records(&dataset_with_records.records)?;

// 3. Call cloud finetuning API
let cloud_response = cloud_client
    .post("/finetune/jobs")
    .multipart(form_data_with_config_and_jsonl)
    .send()
    .await?;
```

### Cloud Finetuning API (cloud/finetune)

The cloud finetuning API handles provider integration and job management:

**Cloud API Endpoints:**
- `POST /finetune/jobs` - Create finetuning job (multipart: config JSON + JSONL file)
- `GET /finetune/jobs` - List finetuning jobs
- `GET /finetune/jobs/{job_id}` - Get job status
- `POST /finetune/jobs/{job_id}/cancel` - Cancel job

Routes are defined in `cloud/src/server/rest.rs`:

```rust
.service(
    web::scope("/finetune")
        .wrap(CloudApiInvokeMiddleware)
        .wrap(TracingContext)
        .wrap(ProjectHeaderExtract) // Project ID from X-Project-Id header
        .service(
            web::scope("/jobs")
                .route("", web::post().to(handler::finetune::create_job))
                .route("", web::get().to(handler::finetune::list_jobs))
                .route("/{job_id}", web::get().to(handler::finetune::get_job))
                .route("/{job_id}/cancel", web::post().to(handler::finetune::cancel_job))
        )
)
```

## Provider Integration

### OpenAI Integration

The system integrates with OpenAI's finetuning API using the `async-openai-compat` library:

**Reference Implementation:**
- OpenAI API: https://platform.openai.com/docs/api-reference/fine-tuning/create
- Library: https://github.com/langdb/async-openai-compat/blob/main/async-openai/src/fine_tuning.rs

**Key Operations:**
1. **Upload File**: Upload JSONL file to OpenAI
2. **Create Fine-tuning Job**: Submit job with dataset file ID
3. **Monitor Job**: Poll job status until completion
4. **Retrieve Model**: Get fine-tuned model identifier

**Example Flow (Cloud API):**
```rust
// 1. Upload JSONL file to provider
let file = client.files().create(CreateFileRequestArgs::default()
    .file(FileInput::from_vec_u8("training.jsonl".to_string(), jsonl_data))
    .purpose(FilePurpose::FineTune)
    .build()?).await?;

// 2. Create fine-tuning job
let mut builder = CreateFineTuningJobRequestArgs::default();
builder.training_file(file.id);
builder.model("gpt-4o-mini".to_string());
builder.method(FineTuneMethod::Supervised {
    supervised: FineTuneSupervisedMethod {
        hyperparameters: FineTuneSupervisedHyperparameters {
            batch_size: BatchSize::BatchSize(4),
            learning_rate_multiplier: LearningRateMultiplier::LearningRateMultiplier(0.1),
            n_epochs: NEpochs::NEpochs(3),
        },
    },
});

let job = client.fine_tuning().create(builder.build()?).await?;

// 3. Poll job status
loop {
    let status = client.fine_tuning().retrieve(&job.id).await?;
    match status.status {
        FineTuningJobStatus::Succeeded => break,
        FineTuningJobStatus::Failed => return Err("Job failed"),
        _ => tokio::time::sleep(Duration::from_secs(10)).await,
    }
}
```

### Multi-Provider Support

The architecture supports multiple providers through a trait-based design:

```rust
pub trait FinetuningProvider {
    async fn upload_dataset(&self, jsonl_data: Vec<u8>) -> Result<String>;
    async fn create_job(&self, request: CreateFinetuningJobRequest) -> Result<String>;
    async fn get_job_status(&self, job_id: &str) -> Result<FinetuningJobStatus>;
    async fn cancel_job(&self, job_id: &str) -> Result<()>;
}
```

Providers implement this trait:
- `OpenAIFinetuningProvider`
- `AnthropicFinetuningProvider` (future)
- `CustomProvider` (future)

## Data Flow

```
┌─────────────────┐
│  Tracing Spans  │
│  (langdb.traces)│
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Span Selection │
│  & Filtering    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  DatasetWithRecords│
│  (dataset + records)│
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Wrapper API    │
│  (ai-gateway/   │
│   finetune)     │
│                 │
│  - Extract      │
│    messages     │
│  - Generate     │
│    JSONL        │
└────────┬────────┘
         │
         │ POST /finetune/jobs
         │ (multipart: config + JSONL)
         ▼
┌─────────────────┐
│  Cloud Finetuning│
│  API            │
│  (cloud/finetune)│
│                 │
│  - Upload file  │
│  - Create job   │
│  - Track status │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Provider API   │
│  (OpenAI, etc.) │
└─────────────────┘
```

**Flow Details:**

1. **Dataset Creation**: User creates `DatasetWithRecords` containing dataset metadata and records with span data
2. **Wrapper API Processing**: The `ai-gateway/finetune` wrapper API:
   - Extracts training messages from span data in records
   - Converts records to JSONL format
   - Validates JSONL structure
3. **Cloud API Submission**: Wrapper API calls `cloud/finetune` API with:
   - Job configuration (model, hyperparameters, etc.)
   - Generated JSONL file (multipart upload)
4. **Provider Integration**: Cloud API handles provider-specific operations (file upload, job creation, status tracking)

## Evaluation System

Each dataset row can have an evaluation score and topic-specific evaluation prompts:

1. **Automatic Evaluation**: Run evaluation prompts against dataset rows
2. **Manual Review**: Allow users to score examples manually
3. **Topic-Based Prompts**: Different evaluation criteria per topic
4. **Score Filtering**: Filter dataset rows by minimum score threshold

## Future Enhancements

- [ ] Support for multiple finetuning providers (Anthropic, etc.)
- [ ] Advanced span filtering and querying
- [ ] Automated data augmentation and generation
- [ ] Evaluation metrics dashboard
- [ ] Dataset versioning and comparison
- [ ] Integration with model registry
- [ ] Cost estimation before job submission
