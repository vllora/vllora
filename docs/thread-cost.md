### Thread Cost API

- GET /threads/{thread_id}/cost: aggregate usage and performance metrics for a single thread
  - Path params: thread_id (UUID)

Authentication/authorization:
- Requires project-scoped request context and `ThreadRead` permission (same as other thread endpoints).

### Response Schema

Source type: `cloud/src/server/handler/threads.rs::ThreadCostResponse`

Fields:
- total_cost (number): Sum of `attribute.cost.cost` across all spans in the thread, expressed in USD.
- total_output_tokens (number): Sum of `attribute.usage.output_tokens`.
- total_input_tokens (number): Sum of `attribute.usage.input_tokens`.
- avg_ttft (number, nullable): Average time-to-first-token in milliseconds for spans that recorded `ttft`.
- avg_duration (number, nullable): Average span duration in milliseconds (`finish_time_us - start_time_us`).
- avg_tps (number, nullable): Average output tokens per second across spans with non-zero output tokens.
- avg_tpot (number, nullable): Average seconds per output token (inverse of TPS) across spans with non-zero output tokens.

Only spans that include both `attribute.cost` and `attribute.usage.output_tokens` are considered.

### Example

```json
{
  "total_cost": 1.37,
  "total_output_tokens": 8250,
  "total_input_tokens": 16200,
  "avg_ttft": 420.5,
  "avg_duration": 1825.3,
  "avg_tps": 5.4,
  "avg_tpot": 0.21
}
```
