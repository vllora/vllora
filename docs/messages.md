### Messages API

- GET /threads/messages: fetch messages for a thread
  - Query params: threadId (required), limit, offset (optional)

Notes:
- Responses ordered by creation time unless specified.
- Schemas TBD.

### Schema

Message source types:
- Envelope: `cloud_core/src/thread_entities.rs::MessageWithAllMetrics`
- Inner message: `ai-gateway/core/src/types/threads.rs::Message`

Envelope fields:
- id (string)
- created_at (string)
- message (object: Message)
- metrics (array<MessageMetrics>)

Message fields (core):
- model_name (string)
- thread_id (string, nullable)
- user_id (string)
- content_type (enum: Text | ImageUrl | InputAudio)
- content (string, nullable)
- content_array (array<MessageContentPart>)
- type (enum: Human | AI)
- tool_call_id (string, nullable)
- tool_calls (array<ToolCall>, nullable)

MessageMetrics fields:
- ttft (number, nullable)
- usage (object CompletionModelUsage, nullable)
- duration (number, nullable)
- run_id, trace_id, span_id (string, nullable)
- start_time_us (number, nullable)
- cost (number, nullable)

Example (truncated):
```json
{
  "id": "msg-1",
  "created_at": "2025-01-01T12:00:00Z",
  "message": {
    "model_name": "gpt-4o",
    "thread_id": "f8b9c1d2-3456-7890-abcd-ef0123456789",
    "user_id": "user-123",
    "content_type": "Text",
    "content": "Hello",
    "content_array": [["Text", "Hello", null]],
    "type": "Human"
  },
  "metrics": [{"ttft": 120000, "usage": {"input_tokens": 12}}]
}
```
