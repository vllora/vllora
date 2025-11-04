# Thread ID Grouping Implementation Plan

## 📋 Overview

This document outlines the detailed execution plan for implementing thread ID grouping functionality in vLLora. The feature will allow users to group traces by `thread_id`, similar to the existing "bucket by time" functionality.

### Goal
Add a new grouping mode that groups traces by `thread_id`, accessible via:
- **Backend**: Same `/group` endpoint with `group_by=thread` parameter
- **Frontend**: New "Thread" tab in View options UI
- **UI Logic**: Reuse existing grouping patterns from bucket mode

---

## 🏗️ Architecture Analysis

### Current Implementation
- **Endpoint:** `GET /group?bucketSize=3600` returns time-bucketed groups
- **Frontend:** Toggles between "Run" and "Bucket" modes
- **Grouping Key:** `time_bucket` (calculated from `start_time_us / bucket_size_us`)

### New Implementation
- **Endpoint:** `GET /group?group_by=thread` returns thread-grouped data
- **Frontend:** Adds "Thread" option → "Run | Bucket | Thread"
- **Grouping Key:** `thread_id` (directly from database)

---

## 🔧 Detailed Implementation Steps

## Phase 1: Backend Changes

### Step 1.1: Update Query Parameters
**File:** `gateway/src/handlers/group.rs` (lines 58-100)

Add `group_by` parameter to `ListGroupQueryParams`:

```rust
pub struct ListGroupQueryParams {
    pub thread_ids: Option<String>,
    pub trace_ids: Option<String>,
    pub model_name: Option<String>,
    pub type_filter: Option<TypeFilter>,
    pub start_time_min: Option<i64>,
    pub start_time_max: Option<i64>,
    pub bucket_size: Option<i64>,     // Used when group_by=time
    pub group_by: Option<String>,      // NEW: "time" or "thread" (default: "time")
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
```

**Changes:**
- Add `group_by: Option<String>` field
- Default value should be "time" to maintain backward compatibility

---

### Step 1.2: Create Generic Group Response Structure
**File:** `gateway/src/handlers/group.rs`

**Replace** the existing `GroupResponse` with a generic structure that supports multiple grouping types:

```rust
/// Enum representing the grouping key
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "group_by", content = "group_key")]
pub enum GroupByKey {
    #[serde(rename = "time")]
    Time { time_bucket: i64 },

    #[serde(rename = "thread")]
    Thread { thread_id: String },

    // Future grouping types can be added here:
    // #[serde(rename = "model")]
    // Model { model_name: String },
    //
    // #[serde(rename = "user")]
    // User { user_id: String },
}

/// Generic response struct for all grouping types
#[derive(Debug, Serialize)]
pub struct GenericGroupResponse {
    #[serde(flatten)]
    pub key: GroupByKey,                // Flattens the enum fields into the response
    pub thread_ids: Vec<String>,        // Parsed from JSON
    pub trace_ids: Vec<String>,         // Parsed from JSON
    pub run_ids: Vec<String>,           // Parsed from JSON
    pub root_span_ids: Vec<String>,     // Parsed from JSON
    pub request_models: Vec<String>,    // Parsed from JSON
    pub used_models: Vec<String>,       // Parsed from JSON
    pub llm_calls: i64,
    pub cost: f64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub start_time_us: i64,
    pub finish_time_us: i64,
    pub errors: Vec<String>,            // Parsed from JSON
}
```

**JSON Output Examples:**

For time grouping:
```json
{
  "group_by": "time",
  "group_key": {
    "time_bucket": 1737100800000000
  },
  "thread_ids": ["thread-1", "thread-2"],
  "trace_ids": ["trace-1"],
  "cost": 0.05,
  ...
}
```

For thread grouping:
```json
{
  "group_by": "thread",
  "group_key": {
    "thread_id": "thread-123"
  },
  "trace_ids": ["trace-1", "trace-2"],
  "cost": 0.10,
  ...
}
```

**Implementation Details:**
- Uses `#[serde(flatten)]` to embed the grouping key fields directly in the response
- The `group_by` field indicates the grouping type
- Easy to add new grouping types by extending the `GroupByKey` enum
- Frontend can discriminate based on `group_by` field

---

### Step 1.3: Update Database Models
**File:** `core/src/metadata/models/group.rs`

The existing `GroupUsageInformation` can be reused! We just need to make `time_bucket` and `thread_id` both optional:

```rust
#[derive(Debug, Queryable)]
pub struct GroupUsageInformation {
    // Grouping key fields - one will be populated depending on group_by
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

**Why this works:**
- SQL query determines which key field to populate
- For time grouping: `time_bucket` will have value, `thread_id` will be NULL
- For thread grouping: `thread_id` will have value, `time_bucket` will be NULL
- No need for separate structs!
- Easy to add more grouping key columns in the future

---

### Step 1.4: Update Grouping Service (Unified Approach)
**File:** `core/src/metadata/services/group.rs`

**Refactor** to use a unified approach. Update the `ListGroupQuery` to include `group_by`:

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

**Refactor existing methods to be generic:**

```rust
pub trait GroupService {
    // Rename and make generic
    fn list_groups(&self, query: ListGroupQuery) -> Result<Vec<GroupUsageInformation>, DatabaseError>;
    fn count_groups(&self, query: ListGroupQuery) -> Result<i64, DatabaseError>;

    // Existing specific methods (can be kept for backward compatibility)
    fn get_by_time_bucket(...) -> Result<Vec<DbTrace>, DatabaseError>;
    fn count_by_time_bucket(...) -> Result<i64, DatabaseError>;

    // NEW: Generic method for getting spans by group key
    fn get_spans_by_group_key(&self, group_by: GroupBy, key_value: String, ...) -> Result<Vec<DbTrace>, DatabaseError>;
    fn count_spans_by_group_key(&self, group_by: GroupBy, key_value: String, ...) -> Result<i64, DatabaseError>;
}
```

**Implementation for `list_groups` (unified):**

```rust
pub fn list_groups(&self, query: ListGroupQuery) -> Result<Vec<GroupUsageInformation>, DatabaseError> {
    let mut conn = self.pool.get()?;

    // Build WHERE clause filters
    let filters = self.build_filters(&query);

    // Determine GROUP BY clause based on grouping type
    let (group_by_field, order_by_field) = match query.group_by {
        GroupBy::Time => {
            let bucket_size_us = query.bucket_size_seconds * 1_000_000;
            (
                format!("(start_time_us / {}) * {} as time_bucket", bucket_size_us, bucket_size_us),
                "time_bucket DESC".to_string()
            )
        },
        GroupBy::Thread => {
            ("thread_id".to_string(), "start_time_us DESC".to_string())
        },
    };

    // Build SELECT clause - return appropriate grouping key
    let select_key = match query.group_by {
        GroupBy::Time => "time_bucket, NULL as thread_id",
        GroupBy::Thread => "NULL as time_bucket, thread_id",
    };

    let sql = format!(
        r#"
        SELECT
            {select_key},
            json_group_array(DISTINCT thread_id) as thread_ids_json,
            json_group_array(DISTINCT trace_id) as trace_ids_json,
            json_group_array(DISTINCT run_id) as run_ids_json,
            json_group_array(DISTINCT CASE WHEN parent_span_id IS NULL THEN span_id END) as root_span_ids_json,
            json_group_array(DISTINCT json_extract(attribute, '$.model')) as request_models_json,
            json_group_array(DISTINCT json_extract(attribute, '$.model_name')) as used_models_json,
            COUNT(CASE WHEN operation_name = 'model_call' THEN 1 END) as llm_calls,
            SUM(CAST(json_extract(attribute, '$.cost') AS REAL)) as cost,
            SUM(CAST(json_extract(attribute, '$.input_tokens') AS INTEGER)) as input_tokens,
            SUM(CAST(json_extract(attribute, '$.output_tokens') AS INTEGER)) as output_tokens,
            MIN(start_time_us) as start_time_us,
            MAX(finish_time_us) as finish_time_us,
            json_group_array(DISTINCT json_extract(attribute, '$.error')) as errors_json
        FROM ({subquery})
        {filters}
        GROUP BY {group_by_field}
        ORDER BY {order_by_field}
        LIMIT ? OFFSET ?
        "#,
        subquery = group_by_field,  // Either time_bucket calculation or thread_id
        filters = if filters.is_empty() { String::new() } else { format!("WHERE {}", filters) },
        group_by_field = match query.group_by {
            GroupBy::Time => "time_bucket",
            GroupBy::Thread => "thread_id",
        },
        order_by_field = order_by_field,
        select_key = select_key,
    );

    diesel::sql_query(sql)
        .bind::<BigInt, _>(query.limit)
        .bind::<BigInt, _>(query.offset)
        .load::<GroupUsageInformation>(&mut conn)
}
```

**Implementation for `count_groups` (unified):**

```rust
pub fn count_groups(&self, query: ListGroupQuery) -> Result<i64, DatabaseError> {
    let mut conn = self.pool.get()?;
    let filters = self.build_filters(&query);

    let group_by_field = match query.group_by {
        GroupBy::Time => {
            let bucket_size_us = query.bucket_size_seconds * 1_000_000;
            format!("(start_time_us / {}) * {}", bucket_size_us, bucket_size_us)
        },
        GroupBy::Thread => "thread_id".to_string(),
    };

    let sql = format!(
        r#"
        SELECT COUNT(DISTINCT {group_by_field})
        FROM traces
        WHERE (run_id IS NOT NULL OR thread_id IS NOT NULL)
        {filters}
        "#,
        group_by_field = group_by_field,
        filters = if filters.is_empty() { String::new() } else { format!("AND {}", filters) }
    );

    diesel::sql_query(sql)
        .get_result::<i64>(&mut conn)
}
```

**Implementation for `get_spans_by_group_key` (generic):**

```rust
pub fn get_spans_by_group_key(
    &self,
    group_by: GroupBy,
    key_value: String,
    project_id: Option<&str>,
    bucket_size_seconds: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<Vec<DbTrace>, DatabaseError> {
    let mut conn = self.pool.get()?;

    let mut query = traces::table.into_boxed();

    // Apply filter based on group type
    match group_by {
        GroupBy::Time => {
            let bucket_size_us = bucket_size_seconds.unwrap_or(3600) * 1_000_000;
            let time_bucket: i64 = key_value.parse()
                .map_err(|_| DatabaseError::QueryError("Invalid time_bucket".to_string()))?;
            let bucket_start = time_bucket;
            let bucket_end = time_bucket + bucket_size_us;

            query = query
                .filter(traces::start_time_us.ge(bucket_start))
                .filter(traces::start_time_us.lt(bucket_end));
        },
        GroupBy::Thread => {
            query = query.filter(traces::thread_id.eq(key_value));
        },
    }

    // Apply project filter
    if let Some(project_id) = project_id {
        query = query.filter(traces::project_id.eq(project_id));
    }

    query
        .order(traces::start_time_us.asc())
        .limit(limit)
        .offset(offset)
        .load::<DbTrace>(&mut conn)
}
```

**Implementation for `count_spans_by_group_key` (generic):**

```rust
pub fn count_spans_by_group_key(
    &self,
    group_by: GroupBy,
    key_value: String,
    project_id: Option<&str>,
    bucket_size_seconds: Option<i64>,
) -> Result<i64, DatabaseError> {
    let mut conn = self.pool.get()?;

    let mut query = traces::table.into_boxed();

    // Apply filter based on group type
    match group_by {
        GroupBy::Time => {
            let bucket_size_us = bucket_size_seconds.unwrap_or(3600) * 1_000_000;
            let time_bucket: i64 = key_value.parse()
                .map_err(|_| DatabaseError::QueryError("Invalid time_bucket".to_string()))?;
            let bucket_start = time_bucket;
            let bucket_end = time_bucket + bucket_size_us;

            query = query
                .filter(traces::start_time_us.ge(bucket_start))
                .filter(traces::start_time_us.lt(bucket_end));
        },
        GroupBy::Thread => {
            query = query.filter(traces::thread_id.eq(key_value));
        },
    }

    // Apply project filter
    if let Some(project_id) = project_id {
        query = query.filter(traces::project_id.eq(project_id));
    }

    query.count().get_result(&mut conn)
}
```

---

### Step 1.5: Update Handler with Generic Conversion
**File:** `gateway/src/handlers/group.rs` (lines 58-100)

**Simplify** the handler - no need to match on group_by! The conversion handles it:

```rust
pub async fn list_root_group(
    req: HttpRequest,
    query: web::Query<ListGroupQueryParams>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let group_service: GroupServiceImpl =
        GroupServiceImpl::new(Arc::new(db_pool.get_ref().clone()));

    // Extract project_id from extensions
    let project_id = req.extensions().get::<DbProject>().map(|p| p.slug.clone());

    // Parse group_by parameter (default: "time")
    let group_by = match query.group_by.as_deref().unwrap_or("time") {
        "time" => GroupBy::Time,
        "thread" => GroupBy::Thread,
        other => return Err(Error::BadRequest(
            format!("Invalid group_by parameter: '{}'. Must be 'time' or 'thread'", other)
        )),
    };

    // Build query
    let list_query = ListGroupQuery {
        project_id,
        thread_ids: query.thread_ids.as_ref()
            .map(|s| s.split(',').map(|id| id.trim().to_string()).collect()),
        trace_ids: query.trace_ids.as_ref()
            .map(|s| s.split(',').map(|id| id.trim().to_string()).collect()),
        model_name: query.model_name.clone(),
        type_filter: query.type_filter.clone(),
        start_time_min: query.start_time_min,
        start_time_max: query.start_time_max,
        bucket_size_seconds: query.bucket_size.unwrap_or(3600),
        group_by,  // Pass the GroupBy enum
        limit: query.limit.unwrap_or(100),
        offset: query.offset.unwrap_or(0),
    };

    // Single unified call!
    let groups = group_service.list_groups(list_query.clone())?;
    let total = group_service.count_groups(list_query)?;

    // Convert to generic responses
    let generic_responses: Vec<GenericGroupResponse> = groups
        .into_iter()
        .map(|g| g.into())  // Uses From<GroupUsageInformation> trait
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

**Implement the `From` trait for `GenericGroupResponse`:**

```rust
impl From<GroupUsageInformation> for GenericGroupResponse {
    fn from(group: GroupUsageInformation) -> Self {
        // Parse JSON strings to arrays
        let thread_ids: Vec<String> = serde_json::from_str(&group.thread_ids_json).unwrap_or_default();
        let trace_ids: Vec<String> = serde_json::from_str(&group.trace_ids_json).unwrap_or_default();
        let run_ids: Vec<String> = serde_json::from_str(&group.run_ids_json).unwrap_or_default();
        let root_span_ids: Vec<String> = serde_json::from_str(&group.root_span_ids_json).unwrap_or_default();
        let request_models: Vec<String> = serde_json::from_str(&group.request_models_json).unwrap_or_default();
        let used_models: Vec<String> = serde_json::from_str(&group.used_models_json).unwrap_or_default();
        let errors: Vec<String> = serde_json::from_str(&group.errors_json).unwrap_or_default();

        // Determine which grouping key to use
        let key = if let Some(time_bucket) = group.time_bucket {
            GroupByKey::Time { time_bucket }
        } else if let Some(thread_id) = group.thread_id {
            GroupByKey::Thread { thread_id }
        } else {
            // This shouldn't happen if SQL is correct, but provide a fallback
            panic!("GroupUsageInformation must have either time_bucket or thread_id set")
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
        let trace_ids: Vec<String> = serde_json::from_str(&thread_group.trace_ids_json)
            .unwrap_or_default();
        let run_ids: Vec<String> = serde_json::from_str(&thread_group.run_ids_json)
            .unwrap_or_default();
        let root_span_ids: Vec<String> = serde_json::from_str(&thread_group.root_span_ids_json)
            .unwrap_or_default();
        let request_models: Vec<String> = serde_json::from_str(&thread_group.request_models_json)
            .unwrap_or_default();
        let used_models: Vec<String> = serde_json::from_str(&thread_group.used_models_json)
            .unwrap_or_default();
        let errors: Vec<String> = serde_json::from_str(&thread_group.errors_json)
            .unwrap_or_default();

        Self {
            thread_id: thread_group.thread_id,
            trace_ids,
            run_ids,
            root_span_ids,
            request_models,
            used_models,
            llm_calls: thread_group.llm_calls,
            cost: thread_group.cost,
            input_tokens: thread_group.input_tokens,
            output_tokens: thread_group.output_tokens,
            start_time_us: thread_group.start_time_us,
            finish_time_us: thread_group.finish_time_us,
            errors,
        }
    }
}
```

---

### Step 1.6: Add Thread-Specific Span Endpoint
**File:** `gateway/src/handlers/group.rs`

Add new endpoint handler:

```rust
#[derive(Debug, Deserialize)]
pub struct GetSpansByThreadParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn get_spans_by_thread(
    thread_id: web::Path<String>,
    query: web::Query<GetSpansByThreadParams>,
    project: Project,
) -> Result<HttpResponse, Error> {
    let group_service = /* get service */;

    // Fetch spans for this thread
    let spans = group_service.get_spans_by_thread(
        &thread_id,
        project.id.as_deref(),
        query.limit.unwrap_or(100),
        query.offset.unwrap_or(0),
    )?;

    // Get total count
    let total = group_service.count_spans_by_thread(
        &thread_id,
        project.id.as_deref(),
    )?;

    Ok(HttpResponse::Ok().json(PaginatedResponse {
        data: spans,
        pagination: PaginationInfo {
            offset: query.offset.unwrap_or(0),
            limit: query.limit.unwrap_or(100),
            total,
        },
    }))
}
```

---

### Step 1.7: Register New Route
**File:** `gateway/src/http.rs` (lines 305-308)

Update route registration to include new thread endpoint:

```rust
.service(
    web::scope("/group")
        .route("", web::get().to(group::list_root_group))
        .route("/{time_bucket}", web::get().to(group::get_spans_by_group))
        .route("/thread/{thread_id}", web::get().to(group::get_spans_by_thread))  // NEW
)
```

**Important:** The order matters! More specific routes (`/thread/{thread_id}`) should come before generic ones (`/{time_bucket}`) to avoid routing conflicts.

---

## Phase 2: Frontend Changes

### Step 2.1: Update Type Definitions
**File:** `ui/src/contexts/TracesPageContext.tsx`

Update `GroupByMode` type to include "thread":

```typescript
export type GroupByMode = "run" | "bucket" | "thread";  // Add "thread"
```

---

### Step 2.2: Add Thread Group Types
**File:** `ui/src/services/groups-api.ts`

Add new TypeScript types for thread groups:

```typescript
export interface ThreadGroupDTO {
  thread_id: string;           // Grouping key (instead of time_bucket)
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

export interface ListThreadGroupsParams {
  projectId: string;
  limit?: number;
  offset?: number;
  threadId?: string;           // Optional filter
  modelName?: string;
  typeFilter?: string;
  startTimeMin?: number;
  startTimeMax?: number;
}

export interface FetchThreadGroupSpansParams {
  threadId: string;
  projectId: string;
  limit?: number;
  offset?: number;
}
```

---

### Step 2.3: Add API Functions
**File:** `ui/src/services/groups-api.ts`

Add API functions for thread grouping:

```typescript
export async function listThreadGroups(
  params: ListThreadGroupsParams
): Promise<PaginatedResponse<ThreadGroupDTO>> {
  const queryParams = new URLSearchParams({
    group_by: 'thread',  // Critical parameter!
    limit: String(params.limit || 20),
    offset: String(params.offset || 0),
  });

  // Add optional filters
  if (params.threadId) {
    queryParams.append('thread_ids', params.threadId);
  }
  if (params.modelName) {
    queryParams.append('model_name', params.modelName);
  }
  if (params.typeFilter) {
    queryParams.append('type_filter', params.typeFilter);
  }
  if (params.startTimeMin) {
    queryParams.append('start_time_min', String(params.startTimeMin));
  }
  if (params.startTimeMax) {
    queryParams.append('start_time_max', String(params.startTimeMax));
  }

  const response = await fetch(
    `${API_BASE_URL}/group?${queryParams}`,
    {
      headers: {
        'x-project-id': params.projectId,
      },
    }
  );

  if (!response.ok) {
    throw new Error(`Failed to fetch thread groups: ${response.statusText}`);
  }

  return response.json();
}

export async function fetchSpansByThreadGroup(
  params: FetchThreadGroupSpansParams
): Promise<PaginatedResponse<Span>> {
  const queryParams = new URLSearchParams({
    limit: String(params.limit || 100),
    offset: String(params.offset || 0),
  });

  const response = await fetch(
    `${API_BASE_URL}/group/thread/${params.threadId}?${queryParams}`,
    {
      headers: {
        'x-project-id': params.projectId,
      },
    }
  );

  if (!response.ok) {
    throw new Error(`Failed to fetch thread group spans: ${response.statusText}`);
  }

  return response.json();
}
```

---

### Step 2.4: Update GroupingSelector UI
**File:** `ui/src/components/traces/GroupingSelector.tsx`

Add "Thread" option to the toggle group:

```typescript
export function GroupingSelector({
  groupByMode,
  onGroupByModeChange,
  bucketSize,
  onBucketSizeChange,
}: GroupingSelectorProps) {
  return (
    <div className="flex items-center gap-6">
      {/* View mode toggle */}
      <div className="inline-flex items-center gap-3">
        <span className="text-sm font-medium text-muted-foreground">View:</span>
        <ToggleGroup
          type="single"
          value={groupByMode}
          onValueChange={(value) => {
            if (value) onGroupByModeChange(value as GroupByMode);
          }}
        >
          <ToggleGroupItem value="run">Run</ToggleGroupItem>
          <ToggleGroupItem value="bucket">Bucket</ToggleGroupItem>
          <ToggleGroupItem value="thread">Thread</ToggleGroupItem>  {/* NEW */}
        </ToggleGroup>
      </div>

      {/* Bucket size selector (only shown when "bucket" mode is selected) */}
      {groupByMode === 'bucket' && (
        <div className="inline-flex items-center gap-3">
          <span className="text-sm font-medium text-muted-foreground">Bucket size:</span>
          <ToggleGroup
            type="single"
            value={String(bucketSize)}
            onValueChange={(value) => {
              if (value) onBucketSizeChange(Number(value) as BucketSize);
            }}
          >
            {BUCKET_OPTIONS.map((option) => (
              <ToggleGroupItem key={option.value} value={String(option.value)}>
                {option.label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </div>
      )}
    </div>
  );
}
```

**Key points:**
- Add "Thread" as a third toggle option
- Bucket size selector remains hidden when thread mode is active (only shows for bucket mode)

---

### Step 2.5: Create Thread Grouping Hook
**File:** `ui/src/hooks/useThreadGroupsPagination.ts` (NEW FILE)

Create pagination hook for thread groups:

```typescript
import { useState, useEffect, useCallback } from 'react';
import { useQuery } from '@tanstack/react-query';
import { listThreadGroups, ThreadGroupDTO } from '@/services/groups-api';

export interface UseThreadGroupsPaginationParams {
  projectId: string;
  enabled?: boolean;
  onThreadGroupsLoaded?: (groups: ThreadGroupDTO[]) => void;
}

export function useThreadGroupsPagination({
  projectId,
  enabled = true,
  onThreadGroupsLoaded,
}: UseThreadGroupsPaginationParams) {
  const [page, setPage] = useState(1);
  const [allThreadGroups, setAllThreadGroups] = useState<ThreadGroupDTO[]>([]);

  const ITEMS_PER_PAGE = 20;

  // Fetch thread groups with pagination
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['threadGroups', projectId, page],
    queryFn: () => listThreadGroups({
      projectId,
      limit: ITEMS_PER_PAGE,
      offset: (page - 1) * ITEMS_PER_PAGE,
    }),
    enabled: enabled && !!projectId,
  });

  // Accumulate thread groups as pages load
  useEffect(() => {
    if (data?.data) {
      setAllThreadGroups(prev =>
        page === 1 ? data.data : [...prev, ...data.data]
      );
      onThreadGroupsLoaded?.(data.data);
    }
  }, [data, page, onThreadGroupsLoaded]);

  // Load more handler
  const loadMore = useCallback(() => {
    if (data && allThreadGroups.length < data.pagination.total) {
      setPage(p => p + 1);
    }
  }, [data, allThreadGroups.length]);

  // Check if there are more items to load
  const hasMore = data
    ? allThreadGroups.length < data.pagination.total
    : false;

  // Reset when projectId changes
  useEffect(() => {
    setPage(1);
    setAllThreadGroups([]);
  }, [projectId]);

  return {
    threadGroups: allThreadGroups,
    isLoading,
    error,
    hasMore,
    loadingMore: isLoading && page > 1,
    loadMore,
    refetch: () => {
      setPage(1);
      setAllThreadGroups([]);
      refetch();
    },
    pagination: data?.pagination,
  };
}
```

---

### Step 2.6: Update TracesPageContext
**File:** `ui/src/contexts/TracesPageContext.tsx`

Add thread groups state and logic:

```typescript
// Add thread groups state
const [threadGroups, setThreadGroups] = useState<ThreadGroupDTO[]>([]);
const [openThreadGroups, setOpenThreadGroups] = useState<string[]>([]);
const [threadGroupSpansMap, setThreadGroupSpansMap] = useState<Record<string, Span[]>>({});
const [loadingThreadGroupsByThreadId, setLoadingThreadGroupsByThreadId] = useState<Set<string>>(new Set());

// Use thread groups pagination hook
const {
  threadGroups: fetchedThreadGroups,
  isLoading: threadGroupsLoading,
  hasMore: hasMoreThreadGroups,
  loadingMore: loadingMoreThreadGroups,
  loadMore: loadMoreThreadGroups,
  refetch: refetchThreadGroups,
} = useThreadGroupsPagination({
  projectId,
  enabled: groupByMode === 'thread',
  onThreadGroupsLoaded: (groups) => {
    setThreadGroups(prev => {
      // Merge new groups with existing ones
      const existingIds = new Set(prev.map(g => g.thread_id));
      const newGroups = groups.filter(g => !existingIds.has(g.thread_id));
      return [...prev, ...newGroups];
    });

    // Auto-open first group
    if (groups.length > 0 && openThreadGroups.length === 0) {
      setOpenThreadGroups([groups[0].thread_id]);
    }
  },
});

// Function to load spans for a specific thread
const loadSpansByThreadGroup = useCallback(async (threadId: string) => {
  // Prevent duplicate loading
  if (loadingThreadGroupsByThreadId.has(threadId)) {
    return;
  }

  setLoadingThreadGroupsByThreadId(prev => new Set(prev).add(threadId));

  try {
    const response = await fetchSpansByThreadGroup({
      threadId,
      projectId,
      limit: 100,
      offset: 0,
    });

    // Cache spans in threadGroupSpansMap
    setThreadGroupSpansMap(prev => ({
      ...prev,
      [threadId]: response.data,
    }));

    // Also add to flattenSpans for global access
    updateBySpansArray(response.data);
  } catch (error: any) {
    toast.error("Failed to load thread group spans", {
      description: error.message || "An error occurred while loading thread group spans",
    });
  } finally {
    setLoadingThreadGroupsByThreadId(prev => {
      const newSet = new Set(prev);
      newSet.delete(threadId);
      return newSet;
    });
  }
}, [projectId, updateBySpansArray]);

// Function to refresh a single thread group
const refreshSingleThreadGroup = useCallback(async (threadId: string) => {
  try {
    const updatedGroups = await listThreadGroups({
      projectId,
      threadId,  // Filter to just this thread
      limit: 1,
      offset: 0,
    });

    if (updatedGroups.data.length > 0) {
      setThreadGroups(prev =>
        prev.map(g => g.thread_id === threadId ? updatedGroups.data[0] : g)
      );
      console.log('Refreshed thread group stats for:', threadId);
    }
  } catch (error) {
    console.error('Failed to refresh thread group stats:', error);
  }
}, [projectId]);

// Add to context return value
return {
  // ... existing values
  threadGroups,
  threadGroupsLoading,
  hasMoreThreadGroups,
  loadingMoreThreadGroups,
  loadMoreThreadGroups,
  openThreadGroups,
  setOpenThreadGroups,
  loadSpansByThreadGroup,
  threadGroupSpansMap,
  loadingThreadGroupsByThreadId,
  refreshSingleThreadGroup,
};
```

**Update event handling for thread mode:**

```typescript
const handleEvent = useCallback((event: ProjectEventUnion) => {
  if (event.run_id) {
    // Existing run mode logic...
    if (groupByMode === 'run') {
      // ... existing code
    }
    // Existing bucket mode logic...
    else if (groupByMode === 'bucket') {
      // ... existing code
    }
    // NEW: Thread mode logic
    else if (groupByMode === 'thread') {
      const currentSpans = flattenSpans;
      const updatedSpans = processEvent(currentSpans, event);
      const newSpan = updatedSpans.find(s => !currentSpans.find(cs => cs.span_id === s.span_id));

      if (newSpan && newSpan.thread_id) {
        // Check if this thread group exists
        const threadExists = threadGroups.some(g => g.thread_id === newSpan.thread_id);

        // If thread doesn't exist, refresh thread groups list
        if (!threadExists) {
          console.log('New thread detected, refreshing thread groups list:', newSpan.thread_id);
          setTimeout(() => {
            refetchThreadGroups();
          }, 50);
        }

        // Check if this thread group is currently opened
        const isOpen = openThreadGroups.includes(newSpan.thread_id);

        if (isOpen) {
          // Add or update span in the opened thread group
          setThreadGroupSpansMap(prev => {
            const existingSpans = prev[newSpan.thread_id] || [];
            // Check if span already exists (update it)
            if (existingSpans.some(s => s.span_id === newSpan.span_id)) {
              return {
                ...prev,
                [newSpan.thread_id]: existingSpans.map(s =>
                  s.span_id === newSpan.span_id ? newSpan : s
                ),
              };
            }
            // Add new span
            return {
              ...prev,
              [newSpan.thread_id]: [...existingSpans, newSpan],
            };
          });
        }

        // Update flattenSpans for compatibility
        setFlattenSpans(updatedSpans);
      }

      // Refresh thread group stats when run finishes
      if (event.type === 'RunFinished' || event.type === 'RunError') {
        if (newSpan?.thread_id) {
          setTimeout(() => {
            refreshSingleThreadGroup(newSpan.thread_id);
          }, 100);
        }
      }
    }
  }
}, [groupByMode, flattenSpans, threadGroups, openThreadGroups, refetchThreadGroups, refreshSingleThreadGroup]);
```

---

### Step 2.7: Create ThreadCard Component
**File:** `ui/src/pages/chat/traces/thread-card.tsx` (NEW FILE)

Create component to display individual thread groups:

```typescript
import React, { useEffect } from 'react';
import { motion } from 'framer-motion';
import { ChevronRight } from 'lucide-react';
import { ThreadGroupDTO } from '@/services/groups-api';
import { TracesPageConsumer } from '@/contexts/TracesPageContext';
import { TimelineContent } from '@/components/chat/traces/components/TimelineContent';
import { formatCost, formatNumber, formatDuration } from '@/lib/utils';
import { Badge } from '@/components/ui/badge';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';

const CARD_STATS_GRID = 'auto 100px 100px 100px 100px 80px';
// Provider | Cost | Input | Output | Duration | Status

interface ThreadCardProps {
  threadGroup: ThreadGroupDTO;
}

export const ThreadCard: React.FC<ThreadCardProps> = ({ threadGroup }) => {
  const {
    openThreadGroups,
    setOpenThreadGroups,
    loadSpansByThreadGroup,
    threadGroupSpansMap,
    loadingThreadGroupsByThreadId,
  } = TracesPageConsumer();

  const threadId = threadGroup.thread_id;
  const isOpen = openThreadGroups.includes(threadId);
  const allSpans = threadGroupSpansMap[threadId] || [];
  const isLoadingSpans = loadingThreadGroupsByThreadId.has(threadId);

  // Auto-load spans when card is expanded
  useEffect(() => {
    if (isOpen && allSpans.length === 0 && !isLoadingSpans) {
      loadSpansByThreadGroup(threadId);
    }
  }, [isOpen, threadId, allSpans.length, isLoadingSpans, loadSpansByThreadGroup]);

  const toggleAccordion = () => {
    setOpenThreadGroups(prev =>
      prev.includes(threadId)
        ? prev.filter(id => id !== threadId)
        : [...prev, threadId]
    );
  };

  // Calculate duration
  const duration = threadGroup.finish_time_us - threadGroup.start_time_us;

  // Determine status
  const hasErrors = threadGroup.errors.length > 0;

  return (
    <motion.div
      className="rounded-lg border border-border bg-[#0a0a0a]"
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
    >
      {/* Card header */}
      <div
        onClick={toggleAccordion}
        className="cursor-pointer p-4 hover:bg-muted/30 transition-colors"
      >
        <div className="flex items-center justify-between gap-6">
          {/* Left: Thread ID */}
          <div className="flex items-center gap-3 flex-1 min-w-0">
            <ChevronRight
              className={`h-4 w-4 transition-transform flex-shrink-0 ${
                isOpen ? 'rotate-90' : ''
              }`}
            />
            <div className="min-w-0 flex-1">
              <h3 className="font-mono text-sm font-medium truncate">
                {threadId}
              </h3>
              <span className="text-xs text-muted-foreground">
                {threadGroup.trace_ids.length} trace{threadGroup.trace_ids.length !== 1 ? 's' : ''}
              </span>
            </div>
          </div>

          {/* Right: Stats grid */}
          <div
            className="grid items-center gap-4"
            style={{ gridTemplateColumns: CARD_STATS_GRID }}
          >
            {/* Provider */}
            <div className="text-xs">
              <span className="text-muted-foreground">Provider: </span>
              <span className="font-medium">
                {threadGroup.used_models.length > 0
                  ? threadGroup.used_models[0].split('/')[0]
                  : 'N/A'
                }
              </span>
            </div>

            {/* Cost */}
            <div className="text-xs">
              <span className="text-muted-foreground">Cost: </span>
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className="font-medium cursor-help">
                    {formatCost(threadGroup.cost)}
                  </span>
                </TooltipTrigger>
                <TooltipContent>
                  <div className="text-xs">
                    <div>Total: {formatCost(threadGroup.cost)}</div>
                    <div>Calls: {threadGroup.llm_calls}</div>
                  </div>
                </TooltipContent>
              </Tooltip>
            </div>

            {/* Input Tokens */}
            <div className="text-xs">
              <span className="text-muted-foreground">Input: </span>
              <span className="font-medium">
                {threadGroup.input_tokens
                  ? formatNumber(threadGroup.input_tokens)
                  : '0'
                }
              </span>
            </div>

            {/* Output Tokens */}
            <div className="text-xs">
              <span className="text-muted-foreground">Output: </span>
              <span className="font-medium">
                {threadGroup.output_tokens
                  ? formatNumber(threadGroup.output_tokens)
                  : '0'
                }
              </span>
            </div>

            {/* Duration */}
            <div className="text-xs">
              <span className="text-muted-foreground">Duration: </span>
              <span className="font-medium">
                {formatDuration(duration)}
              </span>
            </div>

            {/* Status */}
            <div className="text-xs">
              {hasErrors ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Badge variant="destructive" className="cursor-help">
                      ⚠️ {threadGroup.errors.length}
                    </Badge>
                  </TooltipTrigger>
                  <TooltipContent>
                    <div className="text-xs max-w-xs">
                      {threadGroup.errors.map((error, idx) => (
                        <div key={idx} className="mb-1">{error}</div>
                      ))}
                    </div>
                  </TooltipContent>
                </Tooltip>
              ) : (
                <Badge variant="default" className="bg-green-500/20 text-green-400">
                  ✓ OK
                </Badge>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Expanded content */}
      {isOpen && (
        <motion.div
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: 'auto', opacity: 1 }}
          exit={{ height: 0, opacity: 0 }}
          transition={{ duration: 0.2 }}
          className="border-t border-border"
        >
          {isLoadingSpans ? (
            <div className="flex items-center justify-center py-8">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
            </div>
          ) : (
            <TimelineContent
              spansByRunId={allSpans}
              projectId={projectId}
              // Pass other required props
            />
          )}
        </motion.div>
      )}
    </motion.div>
  );
};
```

---

### Step 2.8: Create ThreadCardGrid Component
**File:** `ui/src/pages/chat/traces/thread-card-grid.tsx` (NEW FILE)

Create grid container for thread cards:

```typescript
import React from 'react';
import { ThreadGroupDTO } from '@/services/groups-api';
import { ThreadCard } from './thread-card';
import { Button } from '@/components/ui/button';

interface ThreadCardGridProps {
  threadGroups: ThreadGroupDTO[];
  hasMore: boolean;
  loadingMore: boolean;
  onLoadMore: () => void;
  observerRef?: React.RefObject<HTMLDivElement>;
}

export function ThreadCardGrid({
  threadGroups,
  hasMore,
  loadingMore,
  onLoadMore,
  observerRef,
}: ThreadCardGridProps) {
  return (
    <div className="px-6 py-4">
      <div className="grid grid-cols-1 gap-4">
        {threadGroups.map((threadGroup) => (
          <ThreadCard
            key={threadGroup.thread_id}
            threadGroup={threadGroup}
          />
        ))}

        {/* Load More section */}
        {hasMore && (
          <div
            ref={observerRef}
            className="flex justify-center py-4"
          >
            {loadingMore ? (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-primary"></div>
                Loading more...
              </div>
            ) : (
              <Button
                variant="outline"
                onClick={onLoadMore}
                className="min-w-[120px]"
              >
                Load More
              </Button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
```

---

### Step 2.9: Update RunTable Component
**File:** `ui/src/pages/chat/traces/run-table.tsx`

Add routing for thread mode:

```typescript
import { ThreadCardGrid } from './thread-card-grid';

export function RunTable() {
  const {
    groupByMode,
    runs,
    groups,
    threadGroups,  // NEW
    runsLoading,
    groupsLoading,
    threadGroupsLoading,  // NEW
    hasMoreRuns,
    hasMoreGroups,
    hasMoreThreadGroups,  // NEW
    loadingMoreRuns,
    loadingMoreGroups,
    loadingMoreThreadGroups,  // NEW
    loadMoreRuns,
    loadMoreGroups,
    loadMoreThreadGroups,  // NEW
  } = TracesPageConsumer();

  const observerTarget = useRef<HTMLDivElement>(null);

  // Determine which mode we're in
  const isRunMode = groupByMode === 'run';
  const isBucketMode = groupByMode === 'bucket';
  const isThreadMode = groupByMode === 'thread';  // NEW

  // Loading states
  const isLoading = isRunMode
    ? runsLoading
    : isBucketMode
      ? groupsLoading
      : threadGroupsLoading;  // NEW

  const hasMore = isRunMode
    ? hasMoreRuns
    : isBucketMode
      ? hasMoreGroups
      : hasMoreThreadGroups;  // NEW

  const loadingMore = isRunMode
    ? loadingMoreRuns
    : isBucketMode
      ? loadingMoreGroups
      : loadingMoreThreadGroups;  // NEW

  const onLoadMore = isRunMode
    ? loadMoreRuns
    : isBucketMode
      ? loadMoreGroups
      : loadMoreThreadGroups;  // NEW

  // Show loading state
  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-primary"></div>
      </div>
    );
  }

  // Show empty state
  const isEmpty = isRunMode
    ? runs.length === 0
    : isBucketMode
      ? groups.length === 0
      : threadGroups.length === 0;  // NEW

  if (isEmpty) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        No {isRunMode ? 'runs' : isBucketMode ? 'groups' : 'threads'} found
      </div>
    );
  }

  return (
    <div className="flex-1 w-full h-full overflow-auto">
      {isRunMode ? (
        <RunTableView
          runs={runs}
          hasMore={hasMore}
          loadingMore={loadingMore}
          onLoadMore={onLoadMore}
          observerRef={observerTarget}
        />
      ) : isBucketMode ? (
        <GroupCardGrid
          groups={groups}
          hasMore={hasMore}
          loadingMore={loadingMore}
          onLoadMore={onLoadMore}
          observerRef={observerTarget}
        />
      ) : (
        <ThreadCardGrid
          threadGroups={threadGroups}
          hasMore={hasMore}
          loadingMore={loadingMore}
          onLoadMore={onLoadMore}
          observerRef={observerTarget}
        />
      )}
    </div>
  );
}
```

---

## Phase 3: Testing & Integration

### Step 3.1: Backend Testing

**Test thread grouping endpoint:**

```bash
# List thread groups
curl "http://localhost:8080/group?group_by=thread&limit=10" \
  -H "x-project-id: your-project-id"

# Expected response:
{
  "data": [
    {
      "thread_id": "thread-123",
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
  ],
  "pagination": {
    "offset": 0,
    "limit": 10,
    "total": 5
  }
}
```

**Test thread-specific span retrieval:**

```bash
# Get spans for specific thread
curl "http://localhost:8080/group/thread/thread-123?limit=100" \
  -H "x-project-id: your-project-id"

# Expected response: Array of spans with pagination
{
  "data": [ /* array of DbTrace objects */ ],
  "pagination": {
    "offset": 0,
    "limit": 100,
    "total": 15
  }
}
```

**Test backward compatibility:**

```bash
# Ensure existing time grouping still works
curl "http://localhost:8080/group?bucketSize=3600&limit=10" \
  -H "x-project-id: your-project-id"

# Should return time-bucketed groups (default behavior)
```

**Test error handling:**

```bash
# Test invalid group_by parameter
curl "http://localhost:8080/group?group_by=invalid&limit=10" \
  -H "x-project-id: your-project-id"

# Expected: 400 Bad Request with error message
```

---

### Step 3.2: Frontend Testing

**Manual Testing Checklist:**

1. **Mode Switching**
   - [ ] Click "Thread" in View toggle
   - [ ] Verify thread groups load correctly
   - [ ] Verify bucket size selector disappears
   - [ ] Switch back to "Run" mode - verify it works
   - [ ] Switch to "Bucket" mode - verify it works
   - [ ] Switch back to "Thread" mode - verify state is preserved

2. **Thread Group Display**
   - [ ] Verify thread IDs are displayed correctly
   - [ ] Verify stats (cost, tokens, duration) are shown
   - [ ] Verify provider names are extracted correctly
   - [ ] Verify error badges appear when errors exist
   - [ ] Verify trace count is accurate

3. **Thread Group Expansion**
   - [ ] Click on a thread group card
   - [ ] Verify it expands with animation
   - [ ] Verify loading spinner shows while fetching spans
   - [ ] Verify spans appear in timeline after loading
   - [ ] Verify multiple thread groups can be open simultaneously

4. **Pagination**
   - [ ] Scroll to bottom of thread list
   - [ ] Click "Load More" button
   - [ ] Verify next page of threads loads
   - [ ] Verify "Load More" disappears when all threads loaded

5. **Real-time Updates** (if applicable)
   - [ ] Start a new trace with a thread_id
   - [ ] Verify new thread group appears in list
   - [ ] Verify stats update when trace completes
   - [ ] Verify spans appear in opened thread group

6. **Empty States**
   - [ ] Test with project that has no threads
   - [ ] Verify empty state message appears
   - [ ] Test with thread that has no spans
   - [ ] Verify appropriate message

7. **State Persistence**
   - [ ] Select "Thread" mode
   - [ ] Refresh the page
   - [ ] Verify "Thread" mode is still selected (via URL param or localStorage)
   - [ ] Open a thread group
   - [ ] Refresh page
   - [ ] Verify thread group opens automatically (if stored in URL)

---

### Step 3.3: Edge Cases & Error Handling

**Edge Cases to Test:**

1. **Null/Missing thread_id**
   - Traces with `thread_id = null` should be filtered out
   - Backend: WHERE clause includes `thread_id IS NOT NULL`
   - Frontend: Should never display threads with empty/null IDs

2. **Thread with Many Spans**
   - Test thread with 100+ spans
   - Verify pagination works on span fetch
   - Verify UI doesn't freeze/lag

3. **Thread with No Root Spans**
   - Test thread where all spans have parent_span_id
   - Verify `root_span_ids` array is empty
   - Verify UI handles empty array gracefully

4. **Duplicate Thread IDs Across Projects**
   - Ensure `project_id` filtering works correctly
   - Verify threads from other projects don't appear

5. **Very Long Thread IDs**
   - Test thread with extremely long ID (100+ chars)
   - Verify UI truncates with ellipsis
   - Verify full ID visible in tooltip/detail view

6. **Concurrent Updates**
   - Open a thread group
   - Trigger real-time event for same thread
   - Verify no duplicate spans appear
   - Verify stats update correctly

**Error Scenarios:**

1. **API Failure**
   - Simulate 500 error from backend
   - Verify toast error message appears
   - Verify UI doesn't crash
   - Verify retry mechanism (if implemented)

2. **Network Timeout**
   - Simulate slow/timeout network
   - Verify loading state shows
   - Verify timeout handled gracefully

3. **Invalid Response Data**
   - Backend returns malformed JSON
   - Verify error caught and logged
   - Verify UI shows error state

---

## Phase 4: Documentation & Cleanup

### Step 4.1: Update Documentation

**Update existing docs:**

1. **File:** `docs/trace-grouping.md`
   - Add section: "Grouping by Thread ID"
   - Document new `group_by=thread` parameter
   - Add example API requests/responses
   - Add screenshots of thread mode UI

2. **File:** `docs/api-reference.md` (if exists)
   - Document `GET /group?group_by=thread`
   - Document `GET /group/thread/{thread_id}`
   - Add parameter descriptions
   - Add response schema

3. **Create:** `docs/thread-grouping-feature.md` (optional)
   - Detailed feature documentation
   - Use cases for thread grouping
   - Comparison with time bucketing
   - Best practices

**Update code comments:**

1. Add JSDoc comments to new functions
2. Update inline comments explaining thread grouping logic
3. Add TODO comments for future enhancements

---

### Step 4.2: Code Cleanup

**Backend:**

1. Remove any debug `println!` statements
2. Add proper error logging with context
3. Ensure consistent error messages
4. Run `cargo fmt` and `cargo clippy`
5. Add unit tests for new service methods

**Frontend:**

1. Remove console.log statements (or convert to proper logging)
2. Ensure consistent naming conventions
3. Remove unused imports
4. Run linter (`npm run lint`)
5. Format code (`npm run format`)

---

### Step 4.3: Performance Optimization

**Backend:**

1. Add database index on `thread_id` column (if not exists)
   ```sql
   CREATE INDEX idx_traces_thread_id ON traces(thread_id);
   ```

2. Consider adding composite index for common queries
   ```sql
   CREATE INDEX idx_traces_thread_project ON traces(thread_id, project_id);
   ```

3. Profile SQL query performance
   - Use `EXPLAIN QUERY PLAN` to analyze
   - Optimize if needed

**Frontend:**

1. Implement virtual scrolling for large thread lists (optional)
2. Add debouncing to search/filter inputs (if added)
3. Optimize re-renders with `React.memo` (if needed)
4. Lazy load timeline component

---

## 📊 Implementation Checklist

### Backend
- [ ] Step 1.1: Update `ListGroupQueryParams` with `group_by` field
- [ ] Step 1.2: Create `ThreadGroupResponse` struct
- [ ] Step 1.3: Create `ThreadGroupInformation` model
- [ ] Step 1.4: Implement `list_by_thread` service method
- [ ] Step 1.4: Implement `count_by_thread` service method
- [ ] Step 1.4: Implement `get_spans_by_thread` service method
- [ ] Step 1.4: Implement `count_spans_by_thread` service method
- [ ] Step 1.5: Update `list_root_group` handler to route by `group_by`
- [ ] Step 1.5: Implement `From<ThreadGroupInformation>` trait
- [ ] Step 1.6: Create `get_spans_by_thread` handler
- [ ] Step 1.7: Register `/group/thread/{thread_id}` route
- [ ] Test all backend endpoints
- [ ] Add error handling
- [ ] Run `cargo fmt` and `cargo clippy`

### Frontend
- [ ] Step 2.1: Update `GroupByMode` type to include "thread"
- [ ] Step 2.2: Create `ThreadGroupDTO` interface
- [ ] Step 2.2: Create `ListThreadGroupsParams` interface
- [ ] Step 2.2: Create `FetchThreadGroupSpansParams` interface
- [ ] Step 2.3: Implement `listThreadGroups` API function
- [ ] Step 2.3: Implement `fetchSpansByThreadGroup` API function
- [ ] Step 2.4: Add "Thread" option to `GroupingSelector`
- [ ] Step 2.5: Create `useThreadGroupsPagination` hook
- [ ] Step 2.6: Add thread groups state to `TracesPageContext`
- [ ] Step 2.6: Implement `loadSpansByThreadGroup` function
- [ ] Step 2.6: Implement `refreshSingleThreadGroup` function
- [ ] Step 2.6: Update event handling for thread mode
- [ ] Step 2.7: Create `ThreadCard` component
- [ ] Step 2.8: Create `ThreadCardGrid` component
- [ ] Step 2.9: Update `RunTable` to route thread mode
- [ ] Test all frontend functionality
- [ ] Run linter and formatter

### Testing
- [ ] Backend: Test `/group?group_by=thread` endpoint
- [ ] Backend: Test `/group/thread/{thread_id}` endpoint
- [ ] Backend: Test backward compatibility
- [ ] Backend: Test error cases
- [ ] Frontend: Test mode switching
- [ ] Frontend: Test thread group display
- [ ] Frontend: Test expansion/collapse
- [ ] Frontend: Test pagination
- [ ] Frontend: Test real-time updates
- [ ] Frontend: Test empty states
- [ ] Frontend: Test state persistence
- [ ] Test all edge cases
- [ ] Test error scenarios

### Documentation & Cleanup
- [ ] Update `docs/trace-grouping.md`
- [ ] Update API documentation
- [ ] Add code comments
- [ ] Remove debug statements
- [ ] Code cleanup and formatting
- [ ] Performance optimization
- [ ] Add database indexes

---

## 🎯 Key Design Decisions

1. **Reuse Existing Endpoint**
   - Use `/group` with `group_by` parameter instead of creating `/thread-groups`
   - Maintains API consistency
   - Easier to add more grouping modes in future

2. **Separate Route for Spans**
   - `/group/thread/{thread_id}` mirrors `/group/{time_bucket}` pattern
   - Clear semantic meaning
   - Easy to understand and use

3. **UI Component Reuse**
   - ThreadCard/ThreadCardGrid reuse logic from GroupCard/GroupCardGrid
   - Reduces code duplication
   - Consistent user experience

4. **State Management**
   - Thread groups have separate state from time bucket groups
   - Prevents conflicts between modes
   - Clean separation of concerns

5. **No Bucket Size for Thread Mode**
   - Thread mode doesn't need bucket size selector
   - Hide it when thread mode is active
   - Simpler, cleaner UI

6. **Backward Compatibility**
   - Default `group_by` to "time" if not specified
   - Existing clients continue to work
   - Gradual migration path

---

## 🚀 Estimated Timeline

| Phase | Tasks | Estimated Time |
|-------|-------|----------------|
| **Phase 1: Backend** | 7 major steps | 3-4 hours |
| **Phase 2: Frontend** | 9 major steps | 4-5 hours |
| **Phase 3: Testing** | Comprehensive testing | 2-3 hours |
| **Phase 4: Documentation** | Docs, cleanup, optimization | 1-2 hours |
| **Total** | All phases | **10-14 hours** |

**Note:** Timeline assumes developer is familiar with the codebase and has development environment set up.

---

## ⚠️ Important Considerations

1. **Thread ID Uniqueness**
   - Ensure `thread_id` is sufficiently unique across traces
   - Consider adding project-level uniqueness constraints

2. **Performance**
   - Thread grouping may create many groups if thread IDs are highly fragmented
   - Consider adding pagination/filtering options
   - Monitor database query performance

3. **Sorting**
   - Thread groups sorted by `start_time_us DESC` (earliest span in thread)
   - Consider adding sort options (alphabetical, cost, tokens, etc.)

4. **Filtering**
   - Consider adding thread ID search/filter in UI
   - Add ability to filter by model, cost range, date range

5. **Real-time Events**
   - Ensure event handlers support thread grouping mode
   - Test with high-frequency event streams

6. **Database Indexes**
   - Add index on `thread_id` for performance
   - Consider composite indexes for common query patterns

7. **Null Handling**
   - Traces with `thread_id = null` are filtered out
   - Ensure this is documented and expected behavior

---

## 🔮 Future Enhancements

1. **Thread Search/Filter**
   - Add search box to filter threads by ID or content
   - Add filter by date range, cost, model

2. **Custom Sorting**
   - Allow sorting by: time, cost, tokens, trace count, errors
   - Add ascending/descending toggle

3. **Thread Comparison**
   - Select multiple threads and compare stats
   - Visual comparison charts

4. **Thread Export**
   - Export thread group data to CSV/JSON
   - Include all spans and aggregated stats

5. **Thread Visualization**
   - Timeline view showing thread activity over time
   - Gantt chart for concurrent threads

6. **Multiple Grouping**
   - Group by thread + time (nested grouping)
   - Group by thread + model

---

## 📝 Notes

- This implementation plan follows the existing patterns established in the bucket-by-time feature
- The code structure mirrors the time-bucketing implementation for consistency
- All new code should follow existing coding standards and conventions
- Tests should be added for all new functionality
- Documentation should be updated to reflect new features

---

**Document Version:** 1.0
**Last Updated:** 2025-11-04
**Author:** Implementation Plan Generator
**Status:** Ready for Implementation
