### Traces API

- GET /traces: list traces ordered by start_time_us (DESC)
  - Filters: project_id, thread_id, run_id, date filters
  - Refer to `ListTracesQuery` in `cloud/src/server/handler/traces.rs`
- GET /traces/run/{id}: get single run spans

Notes:
- Response includes span attributes; see `LangdbSpan` and related types.
- Schemas TBD.

### Schema

Span source type: `cloud/src/server/handler/traces.rs::LangdbSpan`

Fields:
- trace_id (string)
- span_id (string)
- thread_id (string, nullable)
- parent_span_id (string, nullable)
- operation_name (string)
- start_time_us (number)
- finish_time_us (number)
- attribute (object)
- child_attribute (object, nullable)
- run_id (string, nullable)

Example:
```json
{
  "trace_id": "8d8b90b5-9e77-4c0d-9b5a-2a3c1e2f4a6b",
  "span_id": "123456789",
  "parent_span_id": null,
  "operation_name": "api_invoke",
  "start_time_us": 1710000000000,
  "finish_time_us": 1710000002345,
  "attribute": {"model": {"name": "gpt-4o"}},
  "thread_id": "f8b9c1d2-3456-7890-abcd-ef0123456789",
  "run_id": "1f2e3d4c-5678-90ab-cdef-0123456789ab"
}
```
