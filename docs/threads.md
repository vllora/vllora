### Threads API

- GET /threads: list threads for a single project ordered by last_message_date
  - Query params: limit, offset (optional)
  - Returns: List of threads for the current project
- PUT /threads/{id}: update thread title
  - Body: { "title": "New Title" }
  - Returns: Updated thread object

### Thread Messages

- GET /threads/messages: get messages for a thread
  - Query params: threadId (required), limit, offset (optional)
  - Returns: List of messages for the specified thread

Notes:
- Auth and project context required (via X-Project-Id header or default project).
- All endpoints require valid project context from middleware.
- Thread IDs must be valid UUIDs.

### Schema

Thread source type: `ai-gateway/core/src/types/threads.rs::MessageThread`

Fields:
- id (string)
- model_name (string)
- user_id (string)
- project_id (string)
- is_public (boolean)
- title (string, nullable)
- description (string, nullable)
- keywords (array<string>, nullable)

Example:
```json
{
  "id": "f8b9c1d2-3456-7890-abcd-ef0123456789",
  "model_name": "gpt-4o",
  "user_id": "user-123",
  "project_id": "proj-456",
  "is_public": false,
  "title": "Product ideas",
  "description": null,
  "keywords": ["brainstorm", "q1"]
}
```
