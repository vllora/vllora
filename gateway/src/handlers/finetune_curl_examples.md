# Reinforcement Fine-Tuning API - cURL Examples

## Dataset Upload

Upload a JSONL file containing training data for reinforcement fine-tuning.

### Endpoint
```
POST /finetune/datasets
```

### Request

**Headers:**
- `Authorization: Bearer <your-api-key>` - Your API key
- `X-Project-Id: <project-id>` - Your project ID (optional, but recommended)

**Body:**
- Multipart form data with field name `file` containing the JSONL file

### Example JSONL File Format

The JSONL file should contain one JSON object per line, where each object has a `messages` array:

```jsonl
{"messages": [{"role": "system", "content": "You are a helpful assistant."}, {"role": "user", "content": "What is the weather today?"}, {"role": "assistant", "content": "I don't have access to real-time weather data. Please check a weather service or app."}]}
{"messages": [{"role": "system", "content": "You are a helpful assistant."}, {"role": "user", "content": "How do I reset my password?"}, {"role": "assistant", "content": "To reset your password, go to the login page and click 'Forgot Password'. Enter your email address and follow the instructions sent to your email."}]}
{"messages": [{"role": "system", "content": "You are a helpful assistant."}, {"role": "user", "content": "What's the capital of France?"}, {"role": "assistant", "content": "The capital of France is Paris."}]}
```

### cURL Example

```bash
# Set your configuration
BASE_URL="http://localhost:8080"  # Adjust for your environment
API_KEY="your-api-key-here"
PROJECT_ID="550e8400-e29b-41d4-a716-446655440000"

# Create a sample JSONL file
cat > /tmp/training.jsonl << 'EOF'
{"messages": [{"role": "system", "content": "You are a helpful assistant."}, {"role": "user", "content": "What is the weather today?"}, {"role": "assistant", "content": "I don't have access to real-time weather data. Please check a weather service or app."}]}
{"messages": [{"role": "system", "content": "You are a helpful assistant."}, {"role": "user", "content": "How do I reset my password?"}, {"role": "assistant", "content": "To reset your password, go to the login page and click 'Forgot Password'. Enter your email address and follow the instructions sent to your email."}]}
{"messages": [{"role": "system", "content": "You are a helpful assistant."}, {"role": "user", "content": "What's the capital of France?"}, {"role": "assistant", "content": "The capital of France is Paris."}]}
EOF

# Upload the dataset
curl -X POST "${BASE_URL}/finetune/datasets" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "X-Project-Id: ${PROJECT_ID}" \
  -F "file=@/tmp/training.jsonl"
```

### Response

```json
{
  "dataset_id": "accounts/langdb/datasets/dataset-abc123xyz"
}
```

### Complete Example with Response Parsing

```bash
#!/bin/bash

# Configuration
BASE_URL="http://localhost:8080"
API_KEY="your-api-key-here"
PROJECT_ID="550e8400-e29b-41d4-a716-446655440000"

# Create sample JSONL file
cat > /tmp/training.jsonl << 'EOF'
{"messages": [{"role": "system", "content": "You are a helpful assistant."}, {"role": "user", "content": "What is the weather today?"}, {"role": "assistant", "content": "I don't have access to real-time weather data. Please check a weather service or app."}]}
{"messages": [{"role": "system", "content": "You are a helpful assistant."}, {"role": "user", "content": "How do I reset my password?"}, {"role": "assistant", "content": "To reset your password, go to the login page and click 'Forgot Password'. Enter your email address and follow the instructions sent to your email."}]}
EOF

# Upload dataset and capture response
echo "Uploading dataset..."
RESPONSE=$(curl -s -X POST "${BASE_URL}/finetune/datasets" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "X-Project-Id: ${PROJECT_ID}" \
  -F "file=@/tmp/training.jsonl")

# Extract dataset_id (requires jq)
DATASET_ID=$(echo $RESPONSE | jq -r '.dataset_id')

if [ "$DATASET_ID" != "null" ] && [ -n "$DATASET_ID" ]; then
  echo "✓ Dataset uploaded successfully!"
  echo "Dataset ID: ${DATASET_ID}"
else
  echo "✗ Failed to upload dataset"
  echo "Response: ${RESPONSE}"
  exit 1
fi
```

### Notes

1. **File Format**: The file must be in JSONL format (one JSON object per line)
2. **Field Name**: The multipart form field must be named `file`
3. **Content Type**: The file will be automatically detected as `application/x-ndjson`
4. **File Size**: There may be limits on file size depending on your server configuration
5. **Project ID**: While optional, including `X-Project-Id` header is recommended for proper project scoping

### Error Responses

**400 Bad Request** - Invalid file format or empty file:
```json
{
  "error": "File is empty"
}
```

**401 Unauthorized** - Missing or invalid API key:
```json
{
  "error": "Unauthorized"
}
```

**500 Internal Server Error** - Server error during upload:
```json
{
  "error": "Failed to upload dataset: <error details>"
}
```

---

## Create Reinforcement Fine-Tuning Job

Create a reinforcement fine-tuning job using an uploaded dataset.

### Endpoint
```
POST /finetune/reinforcement-jobs
```

### Request

**Headers:**
- `Authorization: Bearer <your-api-key>` - Your API key
- `X-Project-Id: <project-id>` - Your project ID (optional, but recommended)
- `Content-Type: application/json`

**Body:**
- JSON object with job configuration

### Request Fields

**Required:**
- `dataset` (string) - Dataset ID from the upload step (e.g., `"dataset-abc123xyz"`)
- `base_model` (string) - Base model identifier (e.g., `"llama-v3-8b-instruct"`)

**Optional:**
- `output_model` (string) - Name for the fine-tuned model
- `evaluation_dataset` (string) - Dataset ID for evaluation
- `display_name` (string) - Human-readable name for the job
- `training_config` (object) - Training configuration:
  - `learning_rate` (number) - Learning rate
  - `max_context_length` (integer) - Maximum context length
  - `lora_rank` (integer) - LoRA rank
  - `epochs` (number) - Number of epochs
  - `batch_size` (integer) - Batch size
  - `gradient_accumulation_steps` (integer) - Gradient accumulation steps
  - `learning_rate_warmup_steps` (integer) - Learning rate warmup steps
  - `batch_size_samples` (integer) - Batch size in samples
- `inference_parameters` (object) - Inference parameters:
  - `max_output_tokens` (integer) - Maximum output tokens
  - `temperature` (number) - Temperature for sampling
  - `top_p` (number) - Top-p sampling parameter
  - `top_k` (integer) - Top-k sampling parameter
  - `response_candidates_count` (integer) - Number of response candidates
- `chunk_size` (integer) - Chunk size for processing
- `node_count` (integer) - Number of nodes to use

**Note:** The `evaluator` field is automatically set by the backend and does not need to be provided.

### Example 1: Basic Job Creation

```bash
# Set your configuration
BASE_URL="http://localhost:8080"
API_KEY="your-api-key-here"
PROJECT_ID="550e8400-e29b-41d4-a716-446655440000"
DATASET_ID="dataset-abc123xyz"  # From upload step

# Create a basic reinforcement fine-tuning job
curl -X POST "${BASE_URL}/finetune/reinforcement-jobs" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "X-Project-Id: ${PROJECT_ID}" \
  -H "Content-Type: application/json" \
  -d '{
    "dataset": "'"${DATASET_ID}"'",
    "base_model": "llama-v3-8b-instruct"
  }'
```

### Example 2: Job with Output Model Name

```bash
curl -X POST "${BASE_URL}/finetune/reinforcement-jobs" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "X-Project-Id: ${PROJECT_ID}" \
  -H "Content-Type: application/json" \
  -d '{
    "dataset": "'"${DATASET_ID}"'",
    "base_model": "llama-v3-8b-instruct",
    "output_model": "my-custom-model-v1",
    "display_name": "Customer Support Fine-Tuned Model"
  }'
```

### Example 3: Job with Training Configuration

```bash
curl -X POST "${BASE_URL}/finetune/reinforcement-jobs" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "X-Project-Id: ${PROJECT_ID}" \
  -H "Content-Type: application/json" \
  -d '{
    "dataset": "'"${DATASET_ID}"'",
    "base_model": "llama-v3-8b-instruct",
    "output_model": "my-custom-model-v1",
    "display_name": "Customer Support Fine-Tuned Model",
    "training_config": {
      "learning_rate": 0.0001,
      "max_context_length": 32768,
      "lora_rank": 16,
      "epochs": 2.0,
      "batch_size": 65536,
      "gradient_accumulation_steps": 4,
      "learning_rate_warmup_steps": 200,
      "batch_size_samples": 32
    }
  }'
```

### Example 4: Complete Job with All Parameters

```bash
curl -X POST "${BASE_URL}/finetune/reinforcement-jobs" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "X-Project-Id: ${PROJECT_ID}" \
  -H "Content-Type: application/json" \
  -d '{
    "dataset": "'"${DATASET_ID}"'",
    "base_model": "llama-v3-8b-instruct",
    "output_model": "my-custom-model-v1",
    "evaluation_dataset": "eval-dataset-xyz789",
    "display_name": "Customer Support Fine-Tuned Model",
    "training_config": {
      "learning_rate": 0.0001,
      "max_context_length": 32768,
      "lora_rank": 16,
      "epochs": 2.0,
      "batch_size": 65536,
      "gradient_accumulation_steps": 4,
      "learning_rate_warmup_steps": 200,
      "batch_size_samples": 32
    },
    "inference_parameters": {
      "max_output_tokens": 2048,
      "temperature": 0.7,
      "top_p": 0.9,
      "top_k": 40,
      "response_candidates_count": 4
    },
    "chunk_size": 1000,
    "node_count": 1
  }'
```

### Complete Workflow Example

```bash
#!/bin/bash

# Configuration
BASE_URL="http://localhost:8080"
API_KEY="your-api-key-here"
PROJECT_ID="550e8400-e29b-41d4-a716-446655440000"

# Step 1: Upload dataset
echo "Step 1: Uploading dataset..."
cat > /tmp/training.jsonl << 'EOF'
{"messages": [{"role": "system", "content": "You are a helpful assistant."}, {"role": "user", "content": "What is the weather today?"}, {"role": "assistant", "content": "I don't have access to real-time weather data. Please check a weather service or app."}]}
{"messages": [{"role": "system", "content": "You are a helpful assistant."}, {"role": "user", "content": "How do I reset my password?"}, {"role": "assistant", "content": "To reset your password, go to the login page and click 'Forgot Password'."}]}
EOF

DATASET_RESPONSE=$(curl -s -X POST "${BASE_URL}/finetune/datasets" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "X-Project-Id: ${PROJECT_ID}" \
  -F "file=@/tmp/training.jsonl")

DATASET_ID=$(echo $DATASET_RESPONSE | jq -r '.dataset_id')

if [ "$DATASET_ID" == "null" ] || [ -z "$DATASET_ID" ]; then
  echo "✗ Failed to upload dataset"
  echo "Response: ${DATASET_RESPONSE}"
  exit 1
fi

echo "✓ Dataset uploaded: ${DATASET_ID}"

# Step 2: Create reinforcement fine-tuning job
echo ""
echo "Step 2: Creating reinforcement fine-tuning job..."

JOB_RESPONSE=$(curl -s -X POST "${BASE_URL}/finetune/reinforcement-jobs" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "X-Project-Id: ${PROJECT_ID}" \
  -H "Content-Type: application/json" \
  -d "{
    \"dataset\": \"${DATASET_ID}\",
    \"base_model\": \"llama-v3-8b-instruct\",
    \"output_model\": \"my-custom-model-v1\",
    \"display_name\": \"Customer Support Model\",
    \"training_config\": {
      \"learning_rate\": 0.0001,
      \"lora_rank\": 16,
      \"epochs\": 2.0
    }
  }")

JOB_ID=$(echo $JOB_RESPONSE | jq -r '.provider_job_id // .id')

if [ "$JOB_ID" == "null" ] || [ -z "$JOB_ID" ]; then
  echo "✗ Failed to create job"
  echo "Response: ${JOB_RESPONSE}"
  exit 1
fi

echo "✓ Job created: ${JOB_ID}"
echo "Full response: ${JOB_RESPONSE}"
```

### Response

**Success Response (201 Created):**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "provider_job_id": "accounts/langdb/reinforcementFineTuningJobs/job-12345",
  "status": "pending",
  "base_model": "llama-v3-8b-instruct",
  "fine_tuned_model": "my-custom-model-v1",
  "provider": "fireworks",
  "hyperparameters": null,
  "suffix": "my-custom-model-v1",
  "error_message": null,
  "training_file_id": "dataset-abc123xyz",
  "validation_file_id": null,
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-15T10:30:00Z",
  "completed_at": null
}
```

### Error Responses

**400 Bad Request** - Missing required fields:
```json
{
  "error": "dataset is required"
}
```

**401 Unauthorized** - Missing or invalid API key:
```json
{
  "error": "Unauthorized"
}
```

**500 Internal Server Error** - Server error during job creation:
```json
{
  "error": "Failed to create reinforcement fine-tuning job: <error details>"
}
```
