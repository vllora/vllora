### Events API (Server-Sent Events)

- **GET** `/events`
  - Streams project events as Server-Sent Events (SSE)
  - Content-Type: `text/event-stream`

#### Auth and headers
- **Authorization**: `Bearer <access_token>`
- Recommended: `Accept: text/event-stream`
- Connection is long-lived; the server keeps it open and pushes events.

#### Transport format
- Each event is emitted as an SSE `data:` line with a JSON payload, followed by a blank line.
  - Example frame:
    ```
    data: {"type":"RunStarted","run_context":{"run_id":"r-123","thread_id":"t-456"},"timestamp":1738000000000}

    ```
- Reconnect on network drops to resume streaming. No query params are required.

#### Event schema
All events share:
- **type**: string discriminator
- **run_context**: `{ run_id?: string, thread_id?: string }`
- **timestamp**: number (ms since epoch)

Event variants (source: `cloud/src/events/mod.rs`):

- **RunStarted**
  - Fields: `run_context`, `timestamp`
- **RunFinished**
  - Fields: `run_context`, `timestamp`
- **RunError**
  - Fields: `run_context`, `message` (string), `code` (string|null), `timestamp`
- **StepStarted**
  - Fields: `run_context`, `step_name` (string), `timestamp`
- **StepFinished**
  - Fields: `run_context`, `step_name` (string), `timestamp`
- **TextMessageStart**
  - Fields: `run_context`, `message_id` (string), `role` (string), `timestamp`
- **TextMessageContent**
  - Fields: `run_context`, `delta` (string), `message_id` (string), `timestamp`
- **TextMessageEnd**
  - Fields: `run_context`, `message_id` (string), `timestamp`
- **ToolCallStart**
  - Fields: `run_context`, `tool_call_id` (string), `parent_message_id` (string|null), `tool_call_name` (string), `timestamp`
- **ToolCallArgs**
  - Fields: `run_context`, `delta` (string), `tool_call_id` (string), `timestamp`
- **ToolCallEnd**
  - Fields: `run_context`, `tool_call_id` (string), `timestamp`
- **ToolCallResult**
  - Fields: `run_context`, `message_id` (string|null), `tool_call_id` (string), `content` (string), `role` (string = "tool"), `timestamp`
- **StateSnapshot**
  - Fields: `run_context`, `snapshot` (json), `timestamp`
- **StateDelta**
  - Fields: `run_context`, `delta` (json), `timestamp`
- **MessagesSnapshot**
  - Fields: `run_context`, `messages` (array<json>), `timestamp`
- **Raw**
  - Fields: `run_context`, `event` (json), `source` (string|null), `timestamp`
- **Custom**
  - Fields: `run_context`, `name` (string), `value` (json), `timestamp`

#### Minimal examples

```json
{"type":"RunStarted","run_context":{"run_id":"run_1","thread_id":"thr_1"},"timestamp":1738000000000}
```

```json
{"type":"TextMessageContent","run_context":{"run_id":"run_1","thread_id":"thr_1"},"delta":"hello","message_id":"trace_1","timestamp":1738000000100}
```

```json
{"type":"ToolCallResult","run_context":{"run_id":"run_1","thread_id":"thr_1"},"message_id":"trace_1","tool_call_id":"tool_123","content":"{\"ok\":true}","role":"tool","timestamp":1738000000200}
```

#### Notes
- The stream is project-scoped: path param `{project_id}` selects the project; access is enforced by scopes.
- Server may emit `Custom` events for model metadata (e.g., `model_start`). Clients should ignore unknown `type` values gracefully.

#### Custom events: ThreadEvent and MessageEvent
The server also emits `Custom` events wrapping thread and message domain events. These are useful for real-time UI updates outside the standard LLM lifecycle stream.

- **Custom (thread_event)**
  - Emitted for internal thread lifecycle changes.
  - Shape:
    ```json
    {
      "type": "Custom",
      "run_context": { "run_id": null, "thread_id": "<thread_id>" },
      "name": "thread_event",
      "value": { /* thread event payload (object), opaque */ },
      "timestamp": 1738000000000
    }
    ```

- **Custom (message_event)**
  - Emitted for internal message-related changes.
  - Shape:
    ```json
    {
      "type": "Custom",
      "run_context": { "run_id": "<run_id>", "thread_id": "<thread_id>" },
      "name": "message_event",
      "value": { /* message event payload (object), opaque */ },
      "timestamp": 1738000000000
    }
    ```

Notes:
- The `value` field is an opaque JSON object reflecting internal event structures; treat it as subject to change and guard your clients accordingly.
- `run_context.thread_id` is always set; `run_context.run_id` may be `null` for `thread_event`.

