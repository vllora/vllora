# Spans API Documentation

## Overview

The Spans API provides endpoints to retrieve and filter spans (traces) in the AI Gateway. Spans represent individual operations or events in your application's execution flow, containing timing information, attributes, and hierarchical relationships.

## Endpoint

### GET /spans

Retrieve a paginated list of spans with optional filtering.

#### Base URL
```
GET http://localhost:3000/spans
```

#### Authentication
- Requires project context (set via `ProjectMiddleware`)
- Project ID is automatically extracted from request headers/context

#### Query Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `threadIds` | string | No | - | Filter by thread IDs (comma-separated). Special: "null" = no thread, "!null" = has thread |
| `runIds` | string | No | - | Filter by run IDs (comma-separated). Special: "null" = no run, "!null" = has run |
| `operationNames` | string | No | - | Filter by operation names (comma-separated). Special: "null" = no operation, "!null" = has operation |
| `parentSpanIds` | string | No | - | Filter by parent span IDs (comma-separated). Special: "null" = root spans, "!null" = child spans |
| `startTime` | integer | No | - | Filter spans that started after this timestamp (microseconds since epoch) |
| `endTime` | integer | No | - | Filter spans that started before this timestamp (microseconds since epoch) |
| `limit` | integer | No | 100 | Maximum number of results to return (pagination) |
| `offset` | integer | No | 0 | Number of results to skip (pagination) |

**Note:** Parameter names support both camelCase and snake_case (e.g., `threadIds` or `thread_ids`).

**Special Filter Values (ALL filters support these):**
- `null` - Returns only spans where the field IS NULL (e.g., `threadIds=null` → no thread_id)
- `!null` - Returns only spans where the field IS NOT NULL (e.g., `threadIds=!null` → has thread_id)

Examples:
- `threadIds=null` → Spans without a thread
- `threadIds=!null` → Spans with a thread
- `runIds=null` → Spans without a run
- `runIds=!null` → Spans with a run
- `operationNames=null` → Spans without operation name
- `operationNames=!null` → Spans with operation name
- `parentSpanIds=null` → Root spans (no parent)
- `parentSpanIds=!null` → Child spans (has parent)

#### Response Format

The response returns a paginated result with the following structure:

```json
{
  "pagination": {
    "offset": 0,
    "limit": 100,
    "total": 250
  },
  "data": [
    {
      "trace_id": "trace-123",
      "span_id": "span-456",
      "thread_id": "thread-789",
      "parent_span_id": "span-123",
      "operation_name": "model_call",
      "start_time_us": 1697500000000000,
      "finish_time_us": 1697500001000000,
      "attribute": {
        "model": "gpt-4",
        "provider": "openai",
        "temperature": 0.7
      },
      "child_attribute": {
        "response_tokens": 150,
        "prompt_tokens": 50
      },
      "run_id": "run-101"
    }
  ]
}
```

#### Response Fields

##### Pagination Object
- `offset` (integer): The starting position of the results
- `limit` (integer): Maximum number of results returned
- `total` (integer): Total number of spans matching the filters

##### Span Object
- `trace_id` (string): Unique identifier for the trace
- `span_id` (string): Unique identifier for this span
- `thread_id` (string|null): Thread/conversation identifier
- `parent_span_id` (string|null): Parent span ID (for hierarchical relationships)
- `operation_name` (string): Name of the operation (e.g., "model_call", "embedding", "tool_call")
- `start_time_us` (integer): Start timestamp in microseconds
- `finish_time_us` (integer): Finish timestamp in microseconds
- `attribute` (object): JSON object containing span attributes
- `child_attribute` (object|null): Attributes from the first child span (typically for model_call operations)
- `run_id` (string|null): Associated run identifier

## Usage Examples

### Example 1: Basic Request (All Spans)

Retrieve the first 100 spans:

```bash
curl -X GET "http://localhost:3000/spans"
```

### Example 2: Filter by Thread IDs

Get all spans for specific threads:

```bash
# Single thread
curl -X GET "http://localhost:3000/spans?threadIds=thread-abc-123"

# Multiple threads
curl -X GET "http://localhost:3000/spans?threadIds=thread-abc-123,thread-def-456"
```

### Example 3: Filter by Run IDs

Get all spans for specific runs:

```bash
# Single run
curl -X GET "http://localhost:3000/spans?runIds=run-xyz-789"

# Multiple runs
curl -X GET "http://localhost:3000/spans?runIds=run-xyz-789,run-abc-123"
```

### Example 4: Time Range Filtering

Get spans within a specific time range:

```bash
curl -X GET "http://localhost:3000/spans?startTime=1697500000000000&endTime=1697510000000000"
```

### Example 5: Combined Filters with Pagination

Get the second page of spans for a specific thread, with 50 results per page:

```bash
curl -X GET "http://localhost:3000/spans?threadIds=thread-abc-123&limit=50&offset=50"
```

### Example 6: Filter by Run and Time Range

Get spans for specific runs within a time window:

```bash
curl -X GET "http://localhost:3000/spans?runIds=run-123&startTime=1697500000000000&endTime=1697510000000000&limit=25"
```

### Example 7: Filter by Operation Names

Get spans for specific operation types:

```bash
# Single operation
curl -X GET "http://localhost:3000/spans?operationNames=model_call"

# Multiple operations
curl -X GET "http://localhost:3000/spans?operationNames=model_call,embedding,tool_call"
```

### Example 8: Get Child Spans

Get all child spans of specific parent spans:

```bash
# Single parent
curl -X GET "http://localhost:3000/spans?parentSpanIds=span-parent-123"

# Multiple parents
curl -X GET "http://localhost:3000/spans?parentSpanIds=span-parent-123,span-parent-456"
```

### Example 9: Get Root Spans (No Parent)

Get all root-level spans (spans that don't have a parent):

```bash
curl -X GET "http://localhost:3000/spans?parentSpanIds=null"
```

**Note:** To filter for spans where `parent_span_id` is NULL (root spans), pass the string `"null"` as the value.

### Example 10: Complex Filter Combination

Get all model_call operations that are root spans for specific threads:

```bash
curl -X GET "http://localhost:3000/spans?threadIds=thread-abc-123,thread-def-456&operationNames=model_call&parentSpanIds=null&limit=20"
```

### Example 11: Multiple Filters with Arrays

Get spans for multiple runs, multiple operations, within a time range:

```bash
curl -X GET "http://localhost:3000/spans?runIds=run-1,run-2,run-3&operationNames=model_call,tool_call&startTime=1697500000000000&endTime=1697510000000000"
```

### Example 12: Get Spans WITH Thread ID

Get all spans that have a thread_id (not null):

```bash
curl -X GET "http://localhost:3000/spans?threadIds=!null"
```

This is useful for filtering out spans that aren't associated with a conversation thread.

### Example 13: Combine Thread Filter with Other Filters

Get all model_call spans that have a thread_id:

```bash
curl -X GET "http://localhost:3000/spans?threadIds=!null&operationNames=model_call"
```

### Example 14: Get Spans WITHOUT Thread ID

Get all spans that don't have a thread_id:

```bash
curl -X GET "http://localhost:3000/spans?threadIds=null"
```

### Example 15: Get Spans WITHOUT Run ID

Get all spans that aren't associated with a run:

```bash
curl -X GET "http://localhost:3000/spans?runIds=null"
```

### Example 16: Get Child Spans (Has Parent)

Get all spans that have a parent (non-root spans):

```bash
curl -X GET "http://localhost:3000/spans?parentSpanIds=!null"
```

### Example 17: Complex Null Combinations

Get all spans that have a thread_id but no parent (root conversation spans):

```bash
curl -X GET "http://localhost:3000/spans?threadIds=!null&parentSpanIds=null"
```

Get all spans that have both a run_id and a thread_id:

```bash
curl -X GET "http://localhost:3000/spans?runIds=!null&threadIds=!null"
```

## Pagination

The API uses offset-based pagination:

1. **First Page**: `GET /spans?limit=100&offset=0`
2. **Second Page**: `GET /spans?limit=100&offset=100`
3. **Third Page**: `GET /spans?limit=100&offset=200`

The `pagination.total` field in the response indicates the total number of results matching your filters, allowing you to calculate the total number of pages.

### Calculating Total Pages

```javascript
const totalPages = Math.ceil(response.pagination.total / response.pagination.limit);
```

## Implementation Details

### Reused Components

The `/spans` endpoint reuses existing infrastructure from the codebase:

1. **Pagination Pattern**: Uses the same `PaginatedResult<T>` and `Pagination` structs used by `/traces` and `/runs` endpoints
2. **Service Layer**: Leverages `TraceServiceImpl` from `langdb_core::metadata::services::trace`
3. **Query Builder**: Uses `ListTracesQuery` to construct filtered database queries
4. **Project Middleware**: Automatically filters results by project context

### Data Model

Spans are stored in the `traces` table with the following schema:

```sql
CREATE TABLE traces (
    trace_id TEXT NOT NULL,
    span_id TEXT NOT NULL,
    thread_id TEXT,
    parent_span_id TEXT,
    operation_name TEXT NOT NULL,
    start_time_us INTEGER NOT NULL,
    finish_time_us INTEGER NOT NULL,
    attribute TEXT NOT NULL,  -- JSON stored as text
    run_id TEXT,
    project_id TEXT
);
```

### Query Performance

- Results are ordered by `start_time_us` in descending order (most recent first)
- Database indexes should be considered for `thread_id`, `run_id`, and `start_time_us` columns for optimal performance
- The endpoint automatically fetches child attributes for spans in a single additional query

## Error Handling

### Common Error Responses

#### 400 Bad Request
```json
{
  "error": "Invalid query parameter",
  "message": "startTime must be a valid integer"
}
```

#### 500 Internal Server Error
```json
{
  "error": "Database error",
  "message": "Failed to query spans"
}
```

## Related Endpoints

- **GET /traces**: Similar functionality, uses comma-separated `threadIds` for multiple threads
- **GET /runs/{run_id}**: Get spans for a specific run (alternative to filtering by runId)
- **GET /runs**: List all runs with pagination

## Best Practices

1. **Use Pagination**: Always use `limit` and `offset` for large datasets to avoid performance issues
2. **Time Range Filters**: When querying large time ranges, consider breaking them into smaller chunks
3. **Combine Filters**: Use multiple filters together for more precise queries
4. **Monitor Total Count**: Check `pagination.total` to understand the full dataset size

## Code Files

- Handler: [`gateway/src/handlers/spans.rs`](../gateway/src/handlers/spans.rs)
- Route Registration: [`gateway/src/http.rs`](../gateway/src/http.rs) (line 325)
- Service Layer: [`core/src/metadata/services/trace.rs`](../core/src/metadata/services/trace.rs)
- Data Model: [`core/src/metadata/models/trace.rs`](../core/src/metadata/models/trace.rs)
