# Finetune Job Creation API - cURL Examples

This document provides cURL examples for the ai-gateway finetune job creation API. This is a wrapper API that accepts `DatasetWithRecords`, generates JSONL from span data, and forwards the request to the cloud finetuning API.

## Base URL and Authentication

```bash
# Base URL (adjust for your environment)
BASE_URL="http://localhost:3000"  # Local development
# BASE_URL="https://api.langdb.ai"  # Production

# Authentication
API_KEY="your-api-key-here"
PROJECT_ID="550e8400-e29b-41d4-a716-446655440000"
```

## API Endpoint

### Create Finetuning Job

**Endpoint:** `POST /projects/{project_id}/finetune/jobs`

**Content-Type:** `application/json`

This endpoint accepts a `DatasetWithRecords` object containing dataset metadata and records with span data. The API will:
1. Extract messages from span data in each record
2. Generate JSONL format suitable for OpenAI finetuning
3. Forward the request to the cloud finetuning API

**Request Body Structure:**

```json
{
  "dataset": {
    "id": "dataset-uuid",
    "name": "Dataset Name",
    "createdAt": 1234567890,
    "updatedAt": 1234567890,
    "records": [
      {
        "id": "record-id",
        "datasetId": "dataset-uuid",
        "data": { /* Span data with attribute.request and attribute.output */ },
        "spanId": "optional-span-id",
        "topic": "optional-topic",
        "evaluation": {
          "score": 0.95,
          "feedback": "High quality example",
          "evaluatedAt": 1234567890
        },
        "createdAt": 1234567890
      }
    ]
  },
  "base_model": "gpt-4o-mini",
  "provider": "openai",
  "hyperparameters": {
    "batch_size": 4,
    "learning_rate_multiplier": 0.1,
    "n_epochs": 3
  },
  "suffix": "optional-model-suffix"
}
```

## Example 1: Basic Request with OpenAI-style Span Data

```bash
curl -X POST "${BASE_URL}/projects/${PROJECT_ID}/finetune/jobs" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "Content-Type: application/json" \
  -H "X-Project-Id: ${PROJECT_ID}" \
  -d '{
    "dataset": {
      "id": "550e8400-e29b-41d4-a716-446655440001",
      "name": "Customer Support Dataset",
      "createdAt": 1704067200,
      "updatedAt": 1704067200,
      "records": [
        {
          "id": "record-1",
          "datasetId": "550e8400-e29b-41d4-a716-446655440001",
          "data": {
            "span_id": "123456789",
            "trace_id": "550e8400-e29b-41d4-a716-446655440010",
            "operation_name": "openai.chat.completions",
            "attribute": {
              "request": "{\"model\":\"gpt-4o-mini\",\"messages\":[{\"role\":\"system\",\"content\":\"You are a helpful customer support assistant.\"},{\"role\":\"user\",\"content\":\"I need help with my order\"}]}",
              "output": "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"I would be happy to help you with your order. Could you please provide your order number?\"}}]}"
            }
          },
          "spanId": "123456789",
          "topic": "Order Support",
          "evaluation": {
            "score": 0.95,
            "feedback": "High quality customer support interaction",
            "evaluatedAt": 1704067200
          },
          "createdAt": 1704067200
        },
        {
          "id": "record-2",
          "datasetId": "550e8400-e29b-41d4-a716-446655440001",
          "data": {
            "span_id": "123456790",
            "trace_id": "550e8400-e29b-41d4-a716-446655440011",
            "operation_name": "openai.chat.completions",
            "attribute": {
              "request": "{\"model\":\"gpt-4o-mini\",\"messages\":[{\"role\":\"system\",\"content\":\"You are a helpful customer support assistant.\"},{\"role\":\"user\",\"content\":\"When will my order arrive?\"}]}",
              "output": "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"To check your order status, I will need your order number. Once you provide it, I can give you an estimated delivery date.\"}}]}"
            }
          },
          "spanId": "123456790",
          "topic": "Order Support",
          "createdAt": 1704067200
        }
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
  }'
```

## Example 2: Using a JSON File

Create a file `finetune_request.json`:

```json
{
  "dataset": {
    "id": "550e8400-e29b-41d4-a716-446655440001",
    "name": "Customer Support Dataset",
    "createdAt": 1704067200,
    "updatedAt": 1704067200,
    "records": [
      {
        "id": "record-1",
        "datasetId": "550e8400-e29b-41d4-a716-446655440001",
        "data": {
          "span_id": "123456789",
          "trace_id": "550e8400-e29b-41d4-a716-446655440010",
          "operation_name": "openai.chat.completions",
          "attribute": {
            "request": "{\"model\":\"gpt-4o-mini\",\"messages\":[{\"role\":\"system\",\"content\":\"You are a helpful assistant.\"},{\"role\":\"user\",\"content\":\"Hello\"}]}",
            "output": "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"Hi! How can I help you today?\"}}]}"
          }
        },
        "spanId": "123456789",
        "createdAt": 1704067200
      }
    ]
  },
  "base_model": "gpt-4o-mini",
  "provider": "openai",
  "hyperparameters": {
    "batch_size": 4,
    "learning_rate_multiplier": 0.1,
    "n_epochs": 3
  }
}
```

Then make the request:

```bash
curl -X POST "${BASE_URL}/projects/${PROJECT_ID}/finetune/jobs" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "Content-Type: application/json" \
  -H "X-Project-Id: ${PROJECT_ID}" \
  -d @finetune_request.json
```

## Example 3: Minimal Request (No Hyperparameters)

```bash
curl -X POST "${BASE_URL}/projects/${PROJECT_ID}/finetune/jobs" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "Content-Type: application/json" \
  -H "X-Project-Id: ${PROJECT_ID}" \
  -d '{
    "dataset": {
      "id": "550e8400-e29b-41d4-a716-446655440001",
      "name": "Simple Dataset",
      "createdAt": 1704067200,
      "updatedAt": 1704067200,
      "records": [
        {
          "id": "record-1",
          "datasetId": "550e8400-e29b-41d4-a716-446655440001",
          "data": {
            "span_id": "123456789",
            "operation_name": "openai.chat.completions",
            "attribute": {
              "request": "{\"messages\":[{\"role\":\"user\",\"content\":\"Hello\"}]}",
              "output": "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"Hi there!\"}}]}"
            }
          },
          "createdAt": 1704067200
        }
      ]
    },
    "base_model": "gpt-4o-mini",
    "provider": "openai"
  }'
```

## Example 4: Anthropic-style Span Data

The API also supports Anthropic-style message formats:

```bash
curl -X POST "${BASE_URL}/projects/${PROJECT_ID}/finetune/jobs" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "Content-Type: application/json" \
  -H "X-Project-Id: ${PROJECT_ID}" \
  -d '{
    "dataset": {
      "id": "550e8400-e29b-41d4-a716-446655440001",
      "name": "Anthropic Dataset",
      "createdAt": 1704067200,
      "updatedAt": 1704067200,
      "records": [
        {
          "id": "record-1",
          "datasetId": "550e8400-e29b-41d4-a716-446655440001",
          "data": {
            "span_id": "123456789",
            "operation_name": "anthropic.messages.create",
            "attribute": {
              "request": "{\"contents\":[{\"role\":\"user\",\"content\":\"Hello\"}]}",
              "output": "{\"content\":[{\"type\":\"text\",\"text\":\"Hi! How can I help?\"}]}"
            }
          },
          "createdAt": 1704067200
        }
      ]
    },
    "base_model": "gpt-4o-mini",
    "provider": "openai",
    "hyperparameters": {
      "batch_size": 4,
      "learning_rate_multiplier": 0.1,
      "n_epochs": 3
    }
  }'
```

## Expected Response

The API forwards the response from the cloud finetuning API. On success (201 Created):

```json
{
  "id": "770e8400-e29b-41d4-a716-446655440002",
  "provider_job_id": "ftjob-abc123xyz",
  "status": "pending",
  "base_model": "gpt-4o-mini",
  "provider": "openai",
  "hyperparameters": {
    "batch_size": 4,
    "learning_rate_multiplier": 0.1,
    "n_epochs": 3
  },
  "suffix": "customer-support-v1",
  "training_file_id": "file-abc123",
  "validation_file_id": null,
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-15T10:30:00Z"
}
```

## Error Responses

**400 Bad Request** - Invalid dataset or missing required fields:

```json
{
  "error": "Bad Request",
  "message": "Failed to generate JSONL: No valid messages found in dataset records"
}
```

**401 Unauthorized** - Missing or invalid API key:

```json
{
  "error": "Unauthorized",
  "message": "Invalid or missing API key"
}
```

**500 Internal Server Error** - Error forwarding to cloud API:

```json
{
  "error": "Internal Server Error",
  "message": "Failed to call cloud API: ..."
}
```

## Span Data Format

The API extracts messages from span `attribute` fields. Supported formats:

### OpenAI Format
```json
{
  "attribute": {
    "request": "{\"messages\":[{\"role\":\"system\",\"content\":\"...\"},{\"role\":\"user\",\"content\":\"...\"}]}",
    "output": "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"...\"}}]}"
  }
}
```

### Anthropic Format
```json
{
  "attribute": {
    "request": "{\"contents\":[{\"role\":\"user\",\"content\":\"...\"}]}",
    "output": "{\"content\":[{\"type\":\"text\",\"text\":\"...\"}]}"
  }
}
```

### Alternative Formats
The API also supports:
- `attribute.input` instead of `attribute.request`
- `attribute.response` for output
- Direct `attribute.content` for assistant messages

## Notes

- The `dataset.records` array should contain at least one record with valid span data
- Each record's `data` field should contain span information with `attribute.request` and `attribute.output` (or equivalent)
- Messages are extracted from the span data and converted to OpenAI JSONL format
- The generated JSONL is then forwarded to the cloud finetuning API
- The `project_id` in the URL path should match the `X-Project-Id` header
- Evaluation scores and topics are optional but can be used for filtering/quality control
- The API automatically handles message extraction from various provider formats (OpenAI, Anthropic, etc.)
