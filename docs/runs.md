### Runs API

- GET /runs: list runs usage information derived from traces
  - Output struct: `RunUsageInformation` in `cloud/src/server/handler/runs.rs`

Notes:
- Supports time filters and project scoping.
- Schemas TBD.

### Schema

Source type: `cloud/src/server/handler/runs.rs::RunUsageInformation`

Fields:
- run_id (string, nullable)
- thread_ids (array<string>)
- trace_ids (array<string>)
- request_models (array<string>)
- used_models (array<string>)
- used_tools (array<string>)
- mcp_template_definition_ids (array<string>)
- llm_calls (number)
- cost (number)
- input_tokens (number)
- output_tokens (number)
- start_time_us (number)
- finish_time_us (number)
- errors (array<string>)

Example:
```json
{
  "run_id": "1f2e3d4c-5678-90ab-cdef-0123456789ab",
  "thread_ids": ["t1", "t2"],
  "trace_ids": ["uuid-1", "uuid-2"],
  "request_models": [["gpt-4o"]],
  "used_models": [["gpt-4o", "reranker-1"]],
  "used_tools": [["search", "db.query"]],
  "mcp_template_definition_ids": [["tmpl-123"]],
  "llm_calls": 12,
  "cost": 0.42,
  "input_tokens": 2048,
  "output_tokens": 1024,
  "start_time_us": 1710000000000,
  "finish_time_us": 1710000123456,
  "errors": [[]]
}
```
