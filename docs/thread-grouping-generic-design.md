# Thread Grouping: Generic Design Approach

## Overview

Based on your excellent suggestion, we've redesigned the thread grouping feature to use a **generic, extensible response structure** instead of creating separate response types for each grouping mode. This design makes it trivial to add new grouping types in the future (by model, by user, etc.).

---

## Key Design Principles

1. **Single Response Structure**: One `GenericGroupResponse` handles all grouping types
2. **Discriminated Union**: Uses `GroupByKey` enum with serde tagging
3. **Unified Backend Service**: Single `list_groups()` method for all grouping types
4. **Type-Safe**: Rust enum ensures compile-time safety
5. **Extensible**: Adding new grouping types requires minimal code changes

---

## Backend Design

### 1. GroupByKey Enum (Discriminated Union)

```rust
/// Enum representing the grouping key
/// Serializes with "group_by" tag and "group_key" content
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "group_by", content = "group_key")]
pub enum GroupByKey {
    #[serde(rename = "time")]
    Time { time_bucket: i64 },

    #[serde(rename = "thread")]
    Thread { thread_id: String },

    // Future grouping types:
    // #[serde(rename = "model")]
    // Model { model_name: String },
    //
    // #[serde(rename = "user")]
    // User { user_id: String },
}
```

**Serialization Examples:**

Time grouping:
```json
{
  "group_by": "time",
  "group_key": { "time_bucket": 1737100800000000 }
}
```

Thread grouping:
```json
{
  "group_by": "thread",
  "group_key": { "thread_id": "thread-123" }
}
```

---

### 2. Generic Group Response

```rust
/// Generic response struct for all grouping types
#[derive(Debug, Serialize)]
pub struct GenericGroupResponse {
    #[serde(flatten)]
    pub key: GroupByKey,                // Flattens into parent object
    pub thread_ids: Vec<String>,
    pub trace_ids: Vec<String>,
    pub run_ids: Vec<String>,
    pub root_span_ids: Vec<String>,
    pub request_models: Vec<String>,
    pub used_models: Vec<String>,
    pub llm_calls: i64,
    pub cost: f64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub start_time_us: i64,
    pub finish_time_us: i64,
    pub errors: Vec<String>,
}
```

**Complete JSON Output:**

```json
{
  "group_by": "thread",
  "group_key": {
    "thread_id": "thread-123"
  },
  "thread_ids": ["thread-123"],
  "trace_ids": ["trace-1", "trace-2"],
  "run_ids": ["run-1"],
  "root_span_ids": ["span-1", "span-2"],
  "request_models": ["openai/gpt-4"],
  "used_models": ["openai/gpt-4"],
  "llm_calls": 2,
  "cost": 0.05,
  "input_tokens": 1000,
  "output_tokens": 500,
  "start_time_us": 1737100800000000,
  "finish_time_us": 1737104400000000,
  "errors": []
}
```

---

### 3. Unified Database Model

**Update `GroupUsageInformation` to support multiple grouping keys:**

```rust
#[derive(Debug, Queryable)]
pub struct GroupUsageInformation {
    // Grouping key fields - only one will be populated
    pub time_bucket: Option<i64>,       // Populated when group_by=time
    pub thread_id: Option<String>,      // Populated when group_by=thread

    // Aggregated data (same for all grouping types)
    pub thread_ids_json: String,
    pub trace_ids_json: String,
    pub run_ids_json: String,
    pub root_span_ids_json: String,
    pub request_models_json: String,
    pub used_models_json: String,
    pub llm_calls: i64,
    pub cost: f64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub start_time_us: i64,
    pub finish_time_us: i64,
    pub errors_json: String,
}
```

**Benefits:**
- ✅ No need for separate structs per grouping type
- ✅ SQL query determines which key field to populate
- ✅ Easy to add new grouping key columns

---

### 4. GroupBy Enum (Service Layer)

```rust
#[derive(Debug, Clone)]
pub enum GroupBy {
    Time,
    Thread,
    // Future: Model, User, etc.
}

#[derive(Debug, Clone)]
pub struct ListGroupQuery {
    pub project_id: Option<String>,
    pub thread_ids: Option<Vec<String>>,
    pub trace_ids: Option<Vec<String>>,
    pub model_name: Option<String>,
    pub type_filter: Option<TypeFilter>,
    pub start_time_min: Option<i64>,
    pub start_time_max: Option<i64>,
    pub bucket_size_seconds: i64,
    pub group_by: GroupBy,              // NEW: Determines grouping type
    pub limit: i64,
    pub offset: i64,
}
```

---

### 5. Unified Service Interface

```rust
pub trait GroupService {
    // Unified methods (NEW)
    fn list_groups(&self, query: ListGroupQuery)
        -> Result<Vec<GroupUsageInformation>, DatabaseError>;
    fn count_groups(&self, query: ListGroupQuery)
        -> Result<i64, DatabaseError>;

    // Generic span retrieval
    fn get_spans_by_group_key(&self, group_by: GroupBy, key_value: String, ...)
        -> Result<Vec<DbTrace>, DatabaseError>;
    fn count_spans_by_group_key(&self, group_by: GroupBy, key_value: String, ...)
        -> Result<i64, DatabaseError>;

    // Existing methods (keep for backward compatibility)
    fn get_by_time_bucket(...) -> Result<Vec<DbTrace>, DatabaseError>;
    fn count_by_time_bucket(...) -> Result<i64, DatabaseError>;
}
```

---

### 6. Conversion Logic

```rust
impl From<GroupUsageInformation> for GenericGroupResponse {
    fn from(group: GroupUsageInformation) -> Self {
        // Parse JSON strings to arrays
        let thread_ids: Vec<String> = serde_json::from_str(&group.thread_ids_json)
            .unwrap_or_default();
        let trace_ids: Vec<String> = serde_json::from_str(&group.trace_ids_json)
            .unwrap_or_default();
        // ... parse other fields

        // Determine which grouping key to use
        let key = if let Some(time_bucket) = group.time_bucket {
            GroupByKey::Time { time_bucket }
        } else if let Some(thread_id) = group.thread_id {
            GroupByKey::Thread { thread_id }
        } else {
            panic!("GroupUsageInformation must have either time_bucket or thread_id")
        };

        Self {
            key,
            thread_ids,
            trace_ids,
            run_ids,
            root_span_ids,
            request_models,
            used_models,
            llm_calls: group.llm_calls,
            cost: group.cost,
            input_tokens: group.input_tokens,
            output_tokens: group.output_tokens,
            start_time_us: group.start_time_us,
            finish_time_us: group.finish_time_us,
            errors,
        }
    }
}
```

---

### 7. Simplified Handler

```rust
pub async fn list_root_group(
    req: HttpRequest,
    query: web::Query<ListGroupQueryParams>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let group_service = GroupServiceImpl::new(Arc::new(db_pool.get_ref().clone()));
    let project_id = req.extensions().get::<DbProject>().map(|p| p.slug.clone());

    // Parse group_by parameter
    let group_by = match query.group_by.as_deref().unwrap_or("time") {
        "time" => GroupBy::Time,
        "thread" => GroupBy::Thread,
        other => return Err(Error::BadRequest(
            format!("Invalid group_by: '{}'. Must be 'time' or 'thread'", other)
        )),
    };

    // Build query with GroupBy enum
    let list_query = ListGroupQuery {
        project_id,
        thread_ids: /* parse from query */,
        trace_ids: /* parse from query */,
        model_name: query.model_name.clone(),
        type_filter: query.type_filter.clone(),
        start_time_min: query.start_time_min,
        start_time_max: query.start_time_max,
        bucket_size_seconds: query.bucket_size.unwrap_or(3600),
        group_by,  // Pass the enum
        limit: query.limit.unwrap_or(100),
        offset: query.offset.unwrap_or(0),
    };

    // Single unified call!
    let groups = group_service.list_groups(list_query.clone())?;
    let total = group_service.count_groups(list_query)?;

    // Convert to generic responses
    let generic_responses: Vec<GenericGroupResponse> = groups
        .into_iter()
        .map(|g| g.into())
        .collect();

    Ok(HttpResponse::Ok().json(PaginatedResult {
        data: generic_responses,
        pagination: Pagination {
            offset: query.offset.unwrap_or(0),
            limit: query.limit.unwrap_or(100),
            total,
        },
    }))
}
```

**Key Benefits:**
- ✅ No more match statements on group_by in handler
- ✅ Conversion logic handles everything
- ✅ Adding new grouping types only requires updating the enum

---

## Frontend Design

### 1. Generic Group Type

```typescript
export interface GenericGroupDTO {
  group_by: "time" | "thread";  // Discriminator
  group_key: {
    time_bucket?: number;         // Present when group_by="time"
    thread_id?: string;           // Present when group_by="thread"
  };
  thread_ids: string[];
  trace_ids: string[];
  run_ids: string[];
  root_span_ids: string[];
  request_models: string[];
  used_models: string[];
  llm_calls: number;
  cost: number;
  input_tokens: number | null;
  output_tokens: number | null;
  start_time_us: number;
  finish_time_us: number;
  errors: string[];
}
```

### 2. Type Guards

```typescript
export function isTimeGroup(group: GenericGroupDTO): group is GenericGroupDTO & {
  group_by: "time";
  group_key: { time_bucket: number };
} {
  return group.group_by === "time" && "time_bucket" in group.group_key;
}

export function isThreadGroup(group: GenericGroupDTO): group is GenericGroupDTO & {
  group_by: "thread";
  group_key: { thread_id: string };
} {
  return group.group_by === "thread" && "thread_id" in group.group_key;
}
```

### 3. Usage in Components

```typescript
function GroupCard({ group }: { group: GenericGroupDTO }) {
  // Get the grouping key
  const groupKey = isTimeGroup(group)
    ? group.group_key.time_bucket
    : group.group_key.thread_id;

  // Display based on type
  const displayName = isTimeGroup(group)
    ? formatTime(group.group_key.time_bucket)
    : group.group_key.thread_id;

  return (
    <div>
      <h3>{displayName}</h3>
      <span>{group.trace_ids.length} traces</span>
      {/* Rest of the component */}
    </div>
  );
}
```

---

## Adding New Grouping Types

Want to add "group by model"? Here's all you need to change:

### Backend:

1. **Add to `GroupByKey` enum:**
```rust
#[serde(rename = "model")]
Model { model_name: String },
```

2. **Add to `GroupBy` enum:**
```rust
pub enum GroupBy {
    Time,
    Thread,
    Model,  // NEW
}
```

3. **Update `GroupUsageInformation`:**
```rust
pub struct GroupUsageInformation {
    pub time_bucket: Option<i64>,
    pub thread_id: Option<String>,
    pub model_name: Option<String>,  // NEW
    // ... rest of fields
}
```

4. **Update `list_groups()` SQL:**
```rust
GroupBy::Model => {
    ("model_name".to_string(), "start_time_us DESC".to_string())
},
```

5. **Update conversion:**
```rust
let key = if let Some(time_bucket) = group.time_bucket {
    GroupByKey::Time { time_bucket }
} else if let Some(thread_id) = group.thread_id {
    GroupByKey::Thread { thread_id }
} else if let Some(model_name) = group.model_name {
    GroupByKey::Model { model_name }
} else {
    panic!("No grouping key found")
};
```

### Frontend:

1. **Update type:**
```typescript
export interface GenericGroupDTO {
  group_by: "time" | "thread" | "model";  // Add "model"
  group_key: {
    time_bucket?: number;
    thread_id?: string;
    model_name?: string;  // NEW
  };
  // ... rest of fields
}
```

2. **Add type guard:**
```typescript
export function isModelGroup(group: GenericGroupDTO): group is GenericGroupDTO & {
  group_by: "model";
  group_key: { model_name: string };
} {
  return group.group_by === "model" && "model_name" in group.group_key;
}
```

3. **Update components** to handle the new type.

---

## Comparison: Old vs New Design

### Old Design (Separate Structures)

```rust
// ❌ Need separate structs
pub struct GroupResponse {
    pub time_bucket: i64,
    // ... fields
}

pub struct ThreadGroupResponse {
    pub thread_id: String,
    // ... same fields
}

// ❌ Handler needs match statement
match group_by {
    "time" => {
        let groups = service.list_root_group()?;
        let responses: Vec<GroupResponse> = groups.into_iter().map(...).collect();
        Ok(json(responses))
    }
    "thread" => {
        let groups = service.list_by_thread()?;
        let responses: Vec<ThreadGroupResponse> = groups.into_iter().map(...).collect();
        Ok(json(responses))
    }
}
```

### New Design (Generic Structure)

```rust
// ✅ Single struct with enum
pub struct GenericGroupResponse {
    pub key: GroupByKey,  // Enum handles variants
    // ... fields
}

// ✅ Handler is clean
let groups = service.list_groups(list_query.clone())?;
let responses: Vec<GenericGroupResponse> = groups.into_iter().map(...).collect();
Ok(json(responses))
```

---

## Benefits Summary

| Aspect | Benefit |
|--------|---------|
| **Code Reuse** | Single struct & service method for all grouping types |
| **Maintainability** | Changes to aggregation logic only in one place |
| **Type Safety** | Rust enums ensure compile-time correctness |
| **Extensibility** | Adding new grouping types requires ~10 lines of code |
| **API Clarity** | `group_by` field makes response self-documenting |
| **Frontend** | Type guards provide type-safe discrimination |
| **Testing** | Single code path to test, not N paths |

---

## Migration Path

To migrate from the old `GroupResponse` structure:

1. **Keep old endpoint** for backward compatibility: `GET /group` (defaults to time)
2. **Add `group_by` parameter** with default value "time"
3. **Return `GenericGroupResponse`** with appropriate `GroupByKey`
4. **Frontend can detect** the response type via `group_by` field
5. **No breaking changes** for existing clients

---

## Conclusion

This generic design provides:
- ✅ **Single Source of Truth**: One response structure
- ✅ **Type Safety**: Compile-time guarantees
- ✅ **Extensibility**: Easy to add new grouping types
- ✅ **Clean Code**: No duplication, clear logic
- ✅ **Better DX**: Consistent API patterns

The initial investment of setting up the enum and generic conversion pays off immediately with the first new grouping type, and continues to provide value as the system evolves.

---

**Document Version:** 1.0
**Last Updated:** 2025-11-04
**Status:** Proposed Design
