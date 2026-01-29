# Topic Hierarchy Generation API - cURL Examples

## Endpoint
`POST /finetune/topic-hierarchy/generate`

## Basic Example

```bash
curl -X POST http://localhost:8080/finetune/topic-hierarchy/generate \
  -H "Content-Type: application/json" \
  -d '{
    "goals": "Customer support dataset for a tech company",
    "depth": 3,
    "records": [
      {
        "data": {
          "input": "How do I reset my password?",
          "output": "You can reset your password by visiting the account settings page..."
        }
      }
    ]
  }'
```

## Complete Example with Multiple Records

```bash
curl -X POST http://localhost:8080/finetune/topic-hierarchy/generate \
  -H "Content-Type: application/json" \
  -d '{
    "goals": "Customer support dataset for a SaaS platform covering technical issues, billing questions, and account management",
    "depth": 3,
    "records": [
      {
        "data": {
          "input": "How do I reset my password?",
          "output": "You can reset your password by visiting the account settings page and clicking 'Reset Password'."
        }
      },
      {
        "data": {
          "input": "My subscription was charged twice this month",
          "output": "I apologize for the duplicate charge. Let me check your billing history and process a refund."
        }
      },
      {
        "data": {
          "input": "The API is returning 500 errors",
          "output": "I see you're experiencing API issues. Let me check our system status and help troubleshoot."
        }
      },
      {
        "data": {
          "messages": [
            {
              "role": "user",
              "content": "How do I integrate your API?"
            },
            {
              "role": "assistant",
              "content": "To integrate our API, you'll need to obtain an API key from your dashboard..."
            }
          ]
        }
      }
    ]
  }'
```

## Example with Different Record Formats

The `data` field can contain various JSON structures. Here are examples:

### Using `input`/`output` fields:
```bash
curl -X POST http://localhost:8080/finetune/topic-hierarchy/generate \
  -H "Content-Type: application/json" \
  -d '{
    "goals": "E-commerce customer support",
    "depth": 2,
    "records": [
      {
        "data": {
          "input": "Where is my order?",
          "output": "Your order #12345 is currently in transit and expected to arrive tomorrow."
        }
      }
    ]
  }'
```

### Using `messages` array:
```bash
curl -X POST http://localhost:8080/finetune/topic-hierarchy/generate \
  -H "Content-Type: application/json" \
  -d '{
    "goals": "Technical documentation assistant",
    "depth": 4,
    "records": [
      {
        "data": {
          "messages": [
            {"role": "user", "content": "How do I use the authentication API?"},
            {"role": "assistant", "content": "To authenticate, send a POST request to /auth with your credentials..."}
          ]
        }
      }
    ]
  }'
```

### Using `prompt`/`completion` fields:
```bash
curl -X POST http://localhost:8080/finetune/topic-hierarchy/generate \
  -H "Content-Type: application/json" \
  -d '{
    "goals": "Code generation assistant",
    "depth": 3,
    "records": [
      {
        "data": {
          "prompt": "Write a function to calculate fibonacci numbers",
          "completion": "def fibonacci(n):\n    if n <= 1:\n        return n\n    return fibonacci(n-1) + fibonacci(n-2)"
        }
      }
    ]
  }'
```

## Success Response (200 OK)

```json
{
  "success": true,
  "hierarchy": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "name": "Technical Support",
      "children": [
        {
          "id": "550e8400-e29b-41d4-a716-446655440001",
          "name": "Hardware Issues",
          "children": [
            {
              "id": "550e8400-e29b-41d4-a716-446655440002",
              "name": "Device Malfunction",
              "children": null
            },
            {
              "id": "550e8400-e29b-41d4-a716-446655440003",
              "name": "Connectivity Problems",
              "children": null
            }
          ]
        },
        {
          "id": "550e8400-e29b-41d4-a716-446655440004",
          "name": "Software Issues",
          "children": [
            {
              "id": "550e8400-e29b-41d4-a716-446655440005",
              "name": "Installation Errors",
              "children": null
            },
            {
              "id": "550e8400-e29b-41d4-a716-446655440006",
              "name": "Configuration Problems",
              "children": null
            }
          ]
        }
      ]
    },
    {
      "id": "550e8400-e29b-41d4-a716-446655440007",
      "name": "Account Management",
      "children": [
        {
          "id": "550e8400-e29b-41d4-a716-446655440008",
          "name": "Billing Inquiries",
          "children": null
        },
        {
          "id": "550e8400-e29b-41d4-a716-446655440009",
          "name": "Password Reset",
          "children": null
        }
      ]
    }
  ]
}
```

## Error Response (400 Bad Request)

```json
{
  "error": "Depth must be between 1 and 5"
}
```

## Request Parameters

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `goals` | string | Yes | User's description of dataset goals (used as context for LLM) |
| `depth` | integer | Yes | Hierarchy depth (1-5 levels, where 1 = root only) |
| `records` | array | Yes | Sample records from the dataset for context (up to 20 will be used) |

### Record Structure

Each record in the `records` array should have:
- `data`: A JSON object containing the record data. The system will automatically extract:
  - `input`/`output` fields
  - `messages` array (uses first message as input, last as output)
  - `prompt`/`completion` fields
  - Falls back to serializing the entire object if none of the above are found

## Notes

- The `depth` parameter controls the maximum depth of the generated hierarchy (1-5 levels)
- Only the first 20 records are used for context
- The LLM call is currently a placeholder and will return mock data until implemented
- All nodes in the hierarchy are assigned unique UUIDs
- Leaf nodes have `children: null` (or omitted in JSON)
