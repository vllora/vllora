# Trace Grouping Feature Documentation

## Overview

The trace grouping feature allows users to view traces organized by time buckets instead of individual runs. This is particularly useful for analyzing traces over time periods and understanding activity patterns across different time windows.

**Key Concepts:**
- **Time Bucket**: A time window (e.g., 5 minutes, 1 hour, 24 hours) that groups all spans starting within that period
- **Bucket Size**: Duration in seconds (300 for 5m, 3600 for 1h, 86400 for 24h)
- **Time Bucket Timestamp**: The start of a bucket in microseconds, calculated as `floor(start_time_us / bucket_size_us) * bucket_size_us`
- **Grouping Mode**: UI can display traces by "run" (individual run IDs) or "bucket" (time windows)

**Feature Highlights:**
- Single API call fetches all spans (root + children) for a bucket
- Frontend receives properly typed arrays (not JSON strings)
- Dynamic table header and column widths based on mode
- Real-time capable architecture with centralized state management

## Architecture

### Backend (Rust)

#### API Endpoints

##### 1. List Groups - `GET /group`

Groups root spans (traces without parent spans) into time buckets based on their start time.

**Location:** [gateway/src/handlers/group.rs:58-100](../gateway/src/handlers/group.rs)

**Query Parameters:**
- `bucketSize` (integer, optional): Time bucket size in seconds. Default: 3600 (1 hour)
  - Common values: 3600 (1h), 7200 (2h), 10800 (3h), 21600 (6h), 43200 (12h), 86400 (24h)
- `threadIds` (string, optional): Comma-separated list of thread IDs to filter by
- `traceIds` (string, optional): Comma-separated list of trace IDs to filter by
- `modelName` (string, optional): Filter by model name
- `typeFilter` (string, optional): Filter by type (model or mcp)
- `start_time_min` (integer, optional): Minimum start time in microseconds
- `start_time_max` (integer, optional): Maximum start time in microseconds
- `limit` (integer, optional): Number of results to return. Default: 100
- `offset` (integer, optional): Number of results to skip. Default: 0

**Response Format:**
```json
{
  "pagination": {
    "offset": 0,
    "limit": 100,
    "total": 50
  },
  "data": [
    {
      "time_bucket": 1737100800000000,
      "thread_ids": ["thread-1", "thread-2"],
      "trace_ids": ["trace-1", "trace-2", "trace-3"],
      "run_ids": ["run-1", "run-2"],
      "root_span_ids": ["span-1", "span-2", "span-3"],
      "request_models": ["openai/gpt-4"],
      "used_models": ["openai/gpt-4", "anthropic/claude-3-sonnet"],
      "llm_calls": 5,
      "cost": 0.0234,
      "input_tokens": 1500,
      "output_tokens": 800,
      "start_time_us": 1737100800000000,
      "finish_time_us": 1737104400000000,
      "errors": ["Error message 1"]
    }
  ]
}
```

**Response Fields:**
- `time_bucket`: The start timestamp of the bucket in microseconds
- `thread_ids`: Array of all unique thread IDs in this bucket
- `trace_ids`: Array of all unique trace IDs in this bucket
- `run_ids`: Array of all unique run IDs in this bucket
- `root_span_ids`: Array of all root span IDs in this bucket (count = number of traces)
- `request_models`: Array of models requested via `api_invoke` operations
- `used_models`: Array of models actually used in `model_call` operations
- `llm_calls`: Number of LLM calls (model_call operations) in this bucket
- `cost`: Total cost of all traces in the bucket
- `input_tokens`: Total input tokens across all traces (can be null)
- `output_tokens`: Total output tokens across all traces (can be null)
- `start_time_us`: Earliest trace start time in the bucket (microseconds)
- `finish_time_us`: Latest trace finish time in the bucket (microseconds)
- `errors`: Array of unique error messages from traces in this bucket

**How Bucketing Works:**
1. Each trace's `start_time_us` is divided by `(bucketSize * 1,000,000)` to determine its bucket
2. All traces within the same bucket are aggregated together
3. Statistics (cost, tokens, errors) are summed across all traces in the bucket

**Backend Implementation Details:**

The backend uses SQLite's JSON aggregation functions to group data:

```rust
// In core/src/metadata/services/group.rs
pub fn list_root_group(&self, query: ListGroupQuery) -> Result<Vec<GroupUsageInformation>> {
    let bucket_size_us = query.bucket_size_seconds * 1_000_000;

    // SQL query with JSON aggregation
    let sql = format!(
        r#"
        SELECT
            (start_time_us / {bucket_size_us}) * {bucket_size_us} as time_bucket,
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
        FROM traces
        WHERE (run_id IS NOT NULL OR thread_id IS NOT NULL)
        GROUP BY time_bucket
        ORDER BY time_bucket DESC
        "#,
        bucket_size_us = bucket_size_us
    );
}
```

**Key Implementation Points:**
- Uses Diesel ORM for type-safe queries
- `GroupUsageInformation` struct has `_json` suffixed fields containing JSON strings
- Handler layer transforms these to proper arrays via `GroupResponse` struct
- Time bucket calculation: `(start_time_us / bucket_size_us) * bucket_size_us` truncates to bucket start
- Filter `(run_id IS NOT NULL OR thread_id IS NOT NULL)` excludes orphaned spans

##### 2. Get Spans by Group - `GET /group/{time_bucket}`

Retrieves **all spans** (both root spans and child spans) within a specific time bucket.

**Location:** [gateway/src/handlers/group.rs:136-232](../gateway/src/handlers/group.rs)

**Path Parameters:**
- `time_bucket` (integer): The start timestamp of the bucket in microseconds (from the `time_bucket` field in list groups response)

**Query Parameters:**
- `bucketSize` (integer, optional): Time bucket size in seconds. Default: 3600. Must match the bucket size used when listing groups.
- `limit` (integer, optional): Number of results to return. Default: 100
- `offset` (integer, optional): Number of results to skip. Default: 0

**Filtering:**
- Only returns spans where `run_id IS NOT NULL` **OR** `thread_id IS NOT NULL`
- This excludes orphaned spans that have neither identifier
- Ensures all returned spans are trackable (have at least one identifier)

**Response Format:**
```json
{
  "pagination": {
    "offset": 0,
    "limit": 100,
    "total": 15
  },
  "data": [
    {
      "trace_id": "trace-123",
      "span_id": "span-456",
      "thread_id": "thread-789",
      "parent_span_id": null,
      "operation_name": "chat.completion",
      "start_time_us": 1737100800000000,
      "finish_time_us": 1737100805000000,
      "attribute": {
        "model": "openai/gpt-4",
        "cost": 0.002,
        "input_tokens": 100,
        "output_tokens": 50
      },
      "child_attribute": {
        "children_count": 3,
        "total_children": 5
      },
      "run_id": "run-abc"
    }
  ]
}
```

**Response Fields:**
- `trace_id`: Unique identifier for the trace
- `span_id`: Unique identifier for the span
- `thread_id`: Thread this span belongs to (if any)
- `parent_span_id`: Parent span ID (null for root spans, non-null for child spans)
- `operation_name`: Operation being performed
- `start_time_us`: Start timestamp in microseconds
- `finish_time_us`: Finish timestamp in microseconds
- `attribute`: Span attributes (model, cost, tokens, etc.)
- `child_attribute`: Information about child spans
- `run_id`: Run this span belongs to (if any)

**Important Notes:**
- Returns **ALL spans** within the time bucket (both root and child spans)
- Only includes spans that have at least `run_id` OR `thread_id` (excludes orphaned spans)
- No need for additional API calls to fetch child spans
- The `bucketSize` parameter must match what was used in the list groups call
- Time bucket calculation: `bucket_start = time_bucket`, `bucket_end = time_bucket + (bucketSize * 1,000,000)`
- Backend implementation: [core/src/metadata/services/group.rs:261-301](../core/src/metadata/services/group.rs)
- **Note:** Buckets may appear with data in list but return empty arrays if their spans don't have run_id or thread_id

**Backend Implementation:**

```rust
// In core/src/metadata/services/group.rs
pub fn get_by_time_bucket(
    &self,
    time_bucket: i64,
    bucket_size_seconds: i64,
    project_id: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<DbTrace>> {
    let bucket_size_us = bucket_size_seconds * 1_000_000;
    let bucket_start = time_bucket;
    let bucket_end = time_bucket + bucket_size_us;

    // Fetch ALL spans (not just root spans) in this time bucket
    let mut query = traces::table
        .filter(traces::start_time_us.ge(bucket_start))
        .filter(traces::start_time_us.lt(bucket_end))
        .filter(traces::run_id.is_not_null().or(traces::thread_id.is_not_null()))
        .into_boxed();

    if let Some(project_id) = project_id {
        query = query.filter(traces::project_id.eq(project_id));
    }

    query
        .order(traces::start_time_us.asc())
        .limit(limit)
        .offset(offset)
        .load::<DbTrace>(&mut conn)
}

// Separate count function ensures pagination matches filtering
pub fn count_by_time_bucket(
    &self,
    time_bucket: i64,
    bucket_size_seconds: i64,
    project_id: Option<&str>,
) -> Result<i64> {
    // Same filter logic as get_by_time_bucket
    let bucket_size_us = bucket_size_seconds * 1_000_000;
    let bucket_start = time_bucket;
    let bucket_end = time_bucket + bucket_size_us;

    traces::table
        .filter(traces::start_time_us.ge(bucket_start))
        .filter(traces::start_time_us.lt(bucket_end))
        .filter(traces::run_id.is_not_null().or(traces::thread_id.is_not_null()))
        .count()
        .get_result(&mut conn)
}
```

**Handler Layer Transformation:**

The handler transforms `GroupUsageInformation` (with JSON strings) into `GroupResponse` (with typed arrays):

```rust
// In gateway/src/handlers/group.rs
#[derive(Debug, Serialize)]
pub struct GroupResponse {
    pub time_bucket: i64,
    pub thread_ids: Vec<String>,    // Parsed from thread_ids_json
    pub trace_ids: Vec<String>,     // Parsed from trace_ids_json
    pub run_ids: Vec<String>,       // Parsed from run_ids_json
    pub root_span_ids: Vec<String>, // Parsed from root_span_ids_json
    // ... other fields
}

impl From<GroupUsageInformation> for GroupResponse {
    fn from(group: GroupUsageInformation) -> Self {
        // Parse JSON strings, defaulting to empty arrays on error
        let thread_ids: Vec<String> = serde_json::from_str(&group.thread_ids_json).unwrap_or_default();
        let trace_ids: Vec<String> = serde_json::from_str(&group.trace_ids_json).unwrap_or_default();
        // ... parse other fields

        Self { time_bucket: group.time_bucket, thread_ids, trace_ids, /* ... */ }
    }
}

// In list_root_group handler
let groups = group_service.list_root_group(list_query.clone())?;
let group_responses: Vec<GroupResponse> = groups.into_iter().map(|g| g.into()).collect();
```

**Why This Architecture?**
- SQLite's `json_group_array()` returns JSON strings, not arrays
- Transformation happens once in backend, not repeatedly in frontend
- Type safety: Frontend TypeScript types match backend Rust types
- Error handling: `unwrap_or_default()` prevents parsing errors from breaking API

---

### Frontend (React/TypeScript)

#### Component Architecture

```
TracesPageContext (State Management)
    ├── TabSelectionHeader (UI Controls)
    │   └── GroupingSelector (ToggleGroup: Run | 1h | 2h | 3h | 6h | 12h | 24h)
    │
    └── TracesContent
        ├── (When groupByMode === "run")
        │   └── RunTableRow (Display individual runs)
        │
        └── (When groupByMode === "bucket")
            └── GroupTableRow (Display time-bucketed groups)
                └── TimelineContent (Display all spans in bucket)
```

#### State Management

**Location:** [ui/src/contexts/TracesPageContext.tsx](../ui/src/contexts/TracesPageContext.tsx)

**Core State:**
```typescript
// Grouping mode: "run" or "bucket"
const [groupByMode, setGroupByMode] = useState<"run" | "bucket">("run");

// Bucket size in seconds (3600, 7200, 10800, 21600, 43200, 86400)
const [bucketSize, setBucketSize] = useState<number>(3600);

// Currently opened groups (by time_bucket)
const [openGroups, setOpenGroups] = useState<{ time_bucket: number; tab: string }[]>([]);

// Cached spans for each time bucket
const [groupSpansMap, setGroupSpansMap] = useState<Record<number, Span[]>>({});

// Loading state for each time bucket
const [loadingGroupsByTimeBucket, setLoadingGroupsByTimeBucket] = useState<Set<number>>(new Set());
```

**Key Functions:**

1. **loadGroupSpans(timeBucket: number)** - Critical function with important implementation details

```typescript
// Refs to track latest state values (prevents stale closure reads)
const groupSpansMapRef = useRef<Record<number, Span[]>>({});
const loadingGroupsByTimeBucketRef = useRef<Set<number>>(new Set());

// Sync refs with state
useEffect(() => {
  groupSpansMapRef.current = groupSpansMap;
}, [groupSpansMap]);

useEffect(() => {
  loadingGroupsByTimeBucketRef.current = loadingGroupsByTimeBucket;
}, [loadingGroupsByTimeBucket]);

const loadGroupSpans = useCallback(async (timeBucket: number) => {
  // CRITICAL: Use refs in guard checks to avoid stale closure reads
  // If we used state directly, the callback would capture old values
  if (loadingGroupsByTimeBucketRef.current.has(timeBucket)) {
    console.log('Already loading group:', timeBucket);
    return;
  }

  // CRITICAL: Check existence in object, not array length
  // Empty arrays are valid cached data, checking .length > 0 would retry unnecessarily
  if (timeBucket in groupSpansMapRef.current) {
    console.log('Group already loaded:', timeBucket);
    return;
  }

  console.log('Loading group spans for bucket:', timeBucket);
  setLoadingGroupsByTimeBucket(prev => new Set(prev).add(timeBucket));

  try {
    const response = await fetchGroupSpans({
      timeBucket,
      projectId,
      bucketSize,
      limit: 100,
      offset: 0,
    });

    // Store ALL spans (root + children) directly from the backend
    setGroupSpansMap(prev => ({
      ...prev,
      [timeBucket]: response.data,
    }));
  } catch (error: any) {
    console.error('Failed to load group spans:', error);
    // Don't cache errors - allow retry
  } finally {
    setLoadingGroupsByTimeBucket(prev => {
      const newSet = new Set(prev);
      newSet.delete(timeBucket);
      return newSet;
    });
  }
}, [projectId, bucketSize]); // CRITICAL: Only projectId and bucketSize as dependencies
// DO NOT add groupSpansMap or loadingGroupsByTimeBucket - causes infinite loops
// We use refs for guard checks instead
```

**Critical Implementation Details:**

⚠️ **Common Pitfall #1: Infinite Loop from useCallback Dependencies**
- **Problem**: Adding `groupSpansMap` or `loadingGroupsByTimeBucket` to useCallback deps causes infinite loops
- **Why**: Every state change recreates the callback, which triggers useEffect in GroupTableRow, which calls the new callback, which updates state...
- **Solution**: Use refs for guard checks, only include `projectId` and `bucketSize` in deps

⚠️ **Common Pitfall #2: Empty Array Check**
- **Problem**: Checking `groupSpansMap[timeBucket]?.length > 0` causes retries for empty buckets
- **Why**: Empty bucket (no spans) is valid cached data, but length check treats it as uncached
- **Solution**: Use `timeBucket in groupSpansMapRef.current` to check key existence

⚠️ **Common Pitfall #3: Stale Closure Reads**
- **Problem**: Guard checks read stale state values captured in closure
- **Why**: useCallback captures values at creation time, not call time
- **Solution**: Use refs which always point to current state values

**Why This Matters:**
- Centralized in context enables future real-time event integration
- Real-time events can update `groupSpansMap` directly
- No need for prop drilling or complex state lifting
- Single source of truth for span data

#### API Service Layer

**Location:** [ui/src/services/groups-api.ts](../ui/src/services/groups-api.ts)

**Key Types:**
```typescript
export interface GroupDTO {
  time_bucket: number; // Start timestamp of the bucket in microseconds
  thread_ids: string[]; // All thread IDs in this bucket
  trace_ids: string[]; // All trace IDs in this bucket
  run_ids: string[]; // All run IDs in this bucket
  root_span_ids: string[]; // All root span IDs in this bucket
  request_models: string[]; // Models requested (from api_invoke)
  used_models: string[]; // Models actually used (from model_call)
  llm_calls: number; // Number of LLM calls in this bucket
  cost: number; // Total cost
  input_tokens: number | null; // Total input tokens
  output_tokens: number | null; // Total output tokens
  start_time_us: number; // First span's start time in the bucket
  finish_time_us: number; // Last span's finish time in the bucket
  errors: string[]; // All errors in this bucket
}

export interface ListGroupsParams {
  projectId: string;
  bucketSize: number;  // in seconds
  threadId?: string;
  limit?: number;
  offset?: number;
}

export interface FetchGroupSpansParams {
  timeBucket: number;  // in microseconds
  projectId: string;
  bucketSize: number;  // in seconds
  limit?: number;
  offset?: number;
}
```

**API Functions:**
```typescript
// List all groups with aggregated statistics
export async function listGroups(params: ListGroupsParams): Promise<PaginatedResponse<GroupDTO>>

// Fetch all spans in a specific time bucket
export async function fetchGroupSpans(params: FetchGroupSpansParams): Promise<PaginatedResponse<Span>>
```

#### Pagination Hook

**Location:** [ui/src/hooks/useGroupsPagination.ts](../ui/src/hooks/useGroupsPagination.ts)

Manages pagination and auto-opening of the first group:

```typescript
export function useGroupsPagination({
  projectId,
  bucketSize,
  threadId,
  onGroupsLoaded,
}: UseGroupsPaginationParams) {
  // Fetch groups based on current page and bucket size
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['groups', projectId, bucketSize, threadId, page],
    queryFn: () => listGroups({ projectId, bucketSize, threadId, limit: 20, offset: (page - 1) * 20 }),
    enabled: !!projectId
  });

  // Auto-open first group when data loads
  useEffect(() => {
    if (groups && groups.length > 0) {
      setOpenGroups([{ time_bucket: groups[0].time_bucket, tab: "trace" }]);
      onGroupsLoaded?.(groups);
    }
  }, [groups, onGroupsLoaded]);

  // ... pagination logic
}
```

#### UI Components

##### 1. GroupingSelector

**Location:** [ui/src/components/traces/GroupingSelector.tsx](../ui/src/components/traces/GroupingSelector.tsx)

Flat toggle group for selecting grouping mode:

```typescript
const BUCKET_OPTIONS = [
  { value: '300', label: '5m' },
  { value: '600', label: '10m' },
  { value: '1200', label: '20m' },
  { value: '1800', label: '30m' },
  { value: '3600', label: '1h' },
  { value: '7200', label: '2h' },
  { value: '10800', label: '3h' },
  { value: '21600', label: '6h' },
  { value: '43200', label: '12h' },
  { value: '86400', label: '24h' },
];

<ToggleGroup type="single" value={currentValue} onValueChange={handleValueChange}>
  <ToggleGroupItem value="run">Run</ToggleGroupItem>
  {BUCKET_OPTIONS.map(option => (
    <ToggleGroupItem key={option.value} value={option.value}>
      {option.label}
    </ToggleGroupItem>
  ))}
</ToggleGroup>
```

##### 2. RunTableHeader

**Location:** [ui/src/pages/chat/traces/run-table-header.tsx](../ui/src/pages/chat/traces/run-table-header.tsx)

Displays the table header with columns for trace information. The header adapts based on the current grouping mode:

**Key Features:**
- Dynamically changes first column label based on mode:
  - **Run Mode**: Shows "Run ID"
  - **Bucket Mode**: Shows "Time Bucket"
- Uses different grid layouts for optimal column widths:
  - Run mode: 100px for Run ID column
  - Bucket mode: 180px for Time Bucket column (wider to accommodate timestamps)

**Implementation:**
```typescript
interface RunTableHeaderProps {
  mode?: 'run' | 'bucket';
}

export function RunTableHeader({ mode = 'run' }: RunTableHeaderProps) {
  const gridColumns = mode === 'run' ? RUN_TABLE_GRID_COLUMNS : GROUP_TABLE_GRID_COLUMNS;

  return (
    <div style={{ gridTemplateColumns: gridColumns }}>
      <div>{/* Expand/collapse column */}</div>
      <div>{mode === 'run' ? 'Run ID' : 'Time Bucket'}</div>
      <div>Provider</div>
      {/* ... other columns: Cost, Input, Output, Time, Duration, Errors */}
    </div>
  );
}
```

##### 3. Table Layout Configuration

**Location:** [ui/src/pages/chat/traces/table-layout.ts](../ui/src/pages/chat/traces/table-layout.ts)

Defines consistent grid column widths for the table:

```typescript
// For Run mode - narrower first column (100px)
export const RUN_TABLE_GRID_COLUMNS = '50px 100px 120px 120px 100px 100px 160px 150px 100px';

// For Bucket mode - wider first column (180px) to display full timestamps
export const GROUP_TABLE_GRID_COLUMNS = '50px 180px 120px 120px 100px 100px 160px 150px 100px';
```

**Column Breakdown:**
1. 50px - Expand/collapse button
2. 100px/180px - Run ID or Time Bucket (changes based on mode)
3. 120px - Provider/Models
4. 120px - Cost
5. 100px - Input tokens
6. 100px - Output tokens
7. 160px - Time
8. 150px - Duration
9. 100px - Errors

##### 4. GroupTableRow

**Location:** [ui/src/pages/chat/traces/group-table-row.tsx](../ui/src/pages/chat/traces/group-table-row.tsx)

Displays a single time bucket group and its spans:

**Key Features:**
- Collapsible accordion UI
- Displays timestamp in Time Bucket column (no trace count shown)
- Shows aggregated statistics (cost, tokens, models, errors)
- Auto-loads spans when expanded
- Shows all spans (root + children) in a unified timeline
- Uses `GROUP_TABLE_GRID_COLUMNS` for wider timestamp column

**Implementation:**
```typescript
export const GroupTableRow: React.FC<GroupTableRowProps> = ({ group }) => {
  const {
    openGroups,
    setOpenGroups,
    loadGroupSpans,
    groupSpansMap,
    loadingGroupsByTimeBucket,
    // ... other context values
  } = TracesPageConsumer();

  const timeBucket = group.time_bucket;
  const isOpen = openGroups.some(g => g.time_bucket === timeBucket);

  // All spans (root + children) are fetched directly from backend
  const allSpans = groupSpansMap[timeBucket] || [];
  const isLoadingSpans = loadingGroupsByTimeBucket.has(timeBucket);

  // Auto-load spans when group is expanded
  useEffect(() => {
    if (isOpen && allSpans.length === 0 && !isLoadingSpans) {
      loadGroupSpans(timeBucket);
    }
  }, [isOpen, timeBucket, loadGroupSpans, allSpans.length, isLoadingSpans]);

  // Render with wider grid layout for bucket mode
  return (
    <div style={{ gridTemplateColumns: GROUP_TABLE_GRID_COLUMNS }}>
      {/* Time Bucket display - shows only timestamp, no trace count */}
      <div>
        <span>{formatTimestamp(group.time_bucket)}</span>
      </div>
      {/* ... other columns */}

      {/* Expanded content */}
      {isOpen && <TimelineContent spansByRunId={allSpans} {...otherProps} />}
    </div>
  );
};
```

**Simplified Logic:**
- Uses `GROUP_TABLE_GRID_COLUMNS` for wider timestamp display
- No trace count displayed (cleaner UI)
- No need to merge spans from `runMap`
- No need to loop through `run_ids`
- All spans come directly from `groupSpansMap[timeBucket]`

---

## Data Flow

### 1. User Selects Bucket Mode

```
User clicks "5m" in GroupingSelector (or any bucket: 5m, 10m, 20m, 30m, 1h, 2h, 3h, 6h, 12h, 24h)
    ↓
GroupingSelector.onValueChange('300')  // 5 minutes = 300 seconds
    ↓
TracesPageContext.setGroupByMode('bucket')
TracesPageContext.setBucketSize(300)
    ↓
useGroupsPagination hook triggers refetch
    ↓
API: GET /group?bucketSize=300&limit=20&offset=0
    ↓
Backend returns list of groups with aggregated stats
    ↓
Groups displayed in UI as GroupTableRow components
```

### 2. User Expands a Group

```
User clicks on a GroupTableRow
    ↓
GroupTableRow.toggleAccordion()
    ↓
setOpenGroups([{ time_bucket: 1737100800000000, tab: 'trace' }])
    ↓
GroupTableRow.useEffect detects isOpen = true
    ↓
TracesPageContext.loadGroupSpans(1737100800000000)
    ↓
Check cache and loading state
    ↓
API: GET /group/1737100800000000?bucketSize=3600&limit=100&offset=0
    ↓
Backend returns ALL spans (root + children) in that time bucket
    ↓
Cache all spans in groupSpansMap[1737100800000000]
    ↓
GroupTableRow.allSpans updates (directly from groupSpansMap)
    ↓
TimelineContent renders all spans in unified timeline
```

**Key Improvement:** Only **1 API call** instead of 1 + N calls (where N = number of unique run_ids)

### 3. Switching Between Modes

```typescript
// Run Mode
groupByMode === "run"
  → useRunsPagination hook active
  → Fetches individual runs
  → Displays RunTableRow components

// Bucket Mode
groupByMode === "bucket"
  → useGroupsPagination hook active
  → Fetches time-bucketed groups
  → Displays GroupTableRow components
```

---

## Key Design Decisions

### 1. Backend Returns All Spans

The backend endpoint `/group/{time_bucket}` returns **all spans** (both root and child spans) in a single API call.

**Rationale:**
- **Performance**: Single database query instead of 1 + N queries
- **Simplicity**: No complex merging logic needed in frontend
- **Consistency**: Backend controls span inclusion logic
- **Network efficiency**: One HTTP request instead of multiple
- **Implementation**: [core/src/metadata/services/group.rs:255-293](../core/src/metadata/services/group.rs)

**Code Change:**
```rust
// Before: Only root spans
let mut query = traces::table
    .filter(traces::parent_span_id.is_null())  // ❌ Removed
    .filter(traces::start_time_us.ge(bucket_start))
    .filter(traces::start_time_us.lt(bucket_end))

// After: All trackable spans (with run_id OR thread_id)
let mut query = traces::table
    .filter(traces::start_time_us.ge(bucket_start))
    .filter(traces::start_time_us.lt(bucket_end))
    .filter(traces::run_id.is_not_null().or(traces::thread_id.is_not_null()))  // ✅ Exclude orphaned spans
```

### 2. No Run Grouping in Bucket Mode

When displaying grouped traces, we **do not** further group by `run_id`. All spans in a time bucket are displayed together in a single timeline.

**Rationale:**
- Simpler UI - avoids nested grouping
- Better for time-based analysis - see all activity in that time window
- Consistent with the "bucket by time" mental model

### 3. Centralized Span Loading

The `loadGroupSpans` function is centralized in `TracesPageContext` rather than in the `GroupTableRow` component.

**Rationale:**
- Enables future real-time event integration
- Real-time events can directly update `groupSpansMap`
- Single source of truth for span data
- Easier testing and debugging

### 4. Automatic First Group Expansion

When groups load, the first group automatically opens and loads its spans.

**Rationale:**
- Better UX - user immediately sees data
- Reduces clicks to see traces
- Predictable behavior

### 5. Time Bucket as Microseconds

Backend returns `time_bucket` in microseconds, matching trace timestamp precision.

**Rationale:**
- Consistency with trace data model
- No precision loss
- Easy calculation: `bucket_start = time_bucket`, `bucket_end = time_bucket + (bucketSize * 1_000_000)`

### 6. JSON-to-Array Transformation in Handler

The backend handler transforms SQLite JSON aggregation results into properly typed arrays before returning the response.

**Rationale:**
- **Type Safety**: Frontend receives properly typed arrays instead of JSON strings
- **Developer Experience**: No need for frontend to parse JSON strings manually
- **API Clarity**: Response format is intuitive and self-documenting
- **Reduced Errors**: Eliminates potential JSON parsing errors in frontend

**Implementation:**

SQLite's `json_group_array()` function returns JSON-encoded strings like `'["id1", "id2"]'`. The backend handler parses these into actual arrays:

```rust
// GroupResponse struct with properly typed fields
#[derive(Debug, Serialize)]
pub struct GroupResponse {
    pub thread_ids: Vec<String>,    // Not String with JSON
    pub trace_ids: Vec<String>,
    pub run_ids: Vec<String>,
    pub root_span_ids: Vec<String>,
    pub request_models: Vec<String>,
    pub used_models: Vec<String>,
    pub errors: Vec<String>,
    // ... other fields
}

// Transformation using From trait
impl From<GroupUsageInformation> for GroupResponse {
    fn from(group: GroupUsageInformation) -> Self {
        // Parse JSON strings into arrays
        let thread_ids: Vec<String> = serde_json::from_str(&group.thread_ids_json).unwrap_or_default();
        let trace_ids: Vec<String> = serde_json::from_str(&group.trace_ids_json).unwrap_or_default();
        // ... parse other fields

        Self {
            thread_ids,
            trace_ids,
            // ... other fields
        }
    }
}
```

**Location:** [gateway/src/handlers/group.rs:42-89](../gateway/src/handlers/group.rs)

---

## Future Enhancements

### Real-time Event Integration

With the centralized `loadGroupSpans` architecture, real-time events can update groups:

```typescript
// Hypothetical real-time event handler
socket.on('new_span', (span: Span) => {
  // Calculate which bucket this span belongs to
  const bucketStart = Math.floor(span.start_time_us / (bucketSize * 1_000_000)) * (bucketSize * 1_000_000);

  // Update the groupSpansMap
  setGroupSpansMap(prev => ({
    ...prev,
    [bucketStart]: [...(prev[bucketStart] || []), span]
  }));

  // Trigger re-render of affected GroupTableRow
});
```

### Dynamic Bucket Size

Allow users to input custom bucket sizes beyond the predefined options.

### Time Range Filtering

Add date range picker to filter groups by time period.

### Export Grouped Data

Enable exporting aggregated statistics for each time bucket.

---

## Testing

### Backend Tests

Test both endpoints with various bucket sizes:

```bash
# List groups with 1-hour buckets
curl "http://localhost:8080/group?bucketSize=3600&limit=10"

# Get spans in a specific bucket
curl "http://localhost:8080/group/1737100800000000?bucketSize=3600&limit=100"
```

### Frontend Tests

1. Test mode switching (Run ↔ Bucket modes)
   - Verify table header label changes from "Run ID" to "Time Bucket"
   - Verify column width changes (100px → 180px) when switching modes
2. Test different bucket sizes (5m, 10m, 20m, 30m, 1h, 2h, 3h, 6h, 12h, 24h)
3. Test group expansion and span loading
4. Test pagination with large datasets
5. Test empty states (no groups, no spans)
6. Verify timestamp display in Time Bucket column (no trace count shown)

---

## Troubleshooting

### Issue: Spans not loading when group expands

**Check:**
1. Is `bucketSize` consistent between list and fetch calls?
2. Is `time_bucket` in microseconds?
3. Check browser console for API errors
4. Verify `loadGroupSpans` is being called in useEffect

### Issue: Wrong spans appearing in group

**Check:**
1. Bucket size mismatch between list and fetch calls
2. Time bucket calculation: backend calculates `floor(start_time_us / (bucketSize * 1_000_000)) * (bucketSize * 1_000_000)`
3. Verify `time_bucket` value is correct

### Issue: Groups not refreshing when bucket size changes

**Check:**
1. React Query cache keys include `bucketSize`
2. `useGroupsPagination` dependency array includes `bucketSize`
3. API calls include correct `bucketSize` parameter

### Issue: Table header shows "Run ID" in bucket mode (or vice versa)

**Check:**
1. Verify `RunTableHeader` receives `mode={groupByMode}` prop
2. Verify prop is passed from `RunTable` component
3. Check React DevTools to see actual prop value

### Issue: Time Bucket column too narrow, timestamp truncated

**Check:**
1. Verify `GroupTableRow` uses `GROUP_TABLE_GRID_COLUMNS` (not `RUN_TABLE_GRID_COLUMNS`)
2. Verify `RunTableHeader` selects `GROUP_TABLE_GRID_COLUMNS` when `mode === 'bucket'`
3. Check browser DevTools computed styles for `gridTemplateColumns`

---

## Debugging Guide

### Full Request/Response Flow

**User clicks "1h" bucket option:**
```
1. GroupingSelector.onValueChange('3600')
2. TracesPageContext.setBucketSize(3600)
3. TracesPageContext.setGroupByMode('bucket')
4. useGroupsPagination refetches with new bucketSize
5. API: GET /group?bucketSize=3600&limit=20&offset=0
6. Backend calculates: time_bucket = (start_time_us / 3600000000) * 3600000000
7. Backend returns GroupUsageInformation with JSON strings
8. Handler transforms to GroupResponse with typed arrays
9. Frontend receives PaginatedGroupsResponse
10. RunTable renders GroupTableRow for each group
11. RunTableHeader shows "Time Bucket" with 180px width
```

**User clicks on a group to expand:**
```
1. GroupTableRow.toggleAccordion() called
2. setOpenGroups([{ time_bucket: 1737100800000000, tab: 'trace' }])
3. GroupTableRow.useEffect detects isOpen = true
4. Checks: loadGroupSpans not already loading this bucket
5. Checks: groupSpansMap doesn't have this bucket cached
6. TracesPageContext.loadGroupSpans(1737100800000000) called
7. Guard checks using refs (not stale state)
8. API: GET /group/1737100800000000?bucketSize=3600&limit=100&offset=0
9. Backend queries traces WHERE start_time_us >= 1737100800000000 AND start_time_us < 1737104400000000
10. Backend filters: (run_id IS NOT NULL OR thread_id IS NOT NULL)
11. Backend returns all spans (root + children)
12. Frontend caches in groupSpansMap[1737100800000000]
13. GroupTableRow re-renders with allSpans from cache
14. TimelineContent renders unified timeline
```

### Common Implementation Errors and Fixes

**Error: Infinite API calls to `/group/{time_bucket}`**
- **Symptom**: Network tab shows repeated identical requests
- **Root Cause**: useCallback dependencies include state objects that change on every render
- **Fix**: Only include `projectId` and `bucketSize` in useCallback deps, use refs for guard checks
- **Location**: [ui/src/contexts/TracesPageContext.tsx](../ui/src/contexts/TracesPageContext.tsx)

**Error: Bucket shows in list but returns empty data**
- **Symptom**: Bucket appears with count > 0 but `/group/{time_bucket}` returns `[]`
- **Root Cause**: Spans in bucket don't have `run_id` or `thread_id` (orphaned spans)
- **Fix**: This is expected behavior - spans without identifiers are excluded
- **Location**: [core/src/metadata/services/group.rs:230](../core/src/metadata/services/group.rs)

**Error: Pagination total doesn't match data length**
- **Symptom**: API says `total: 100` but only returns 50 spans
- **Root Cause**: Count query uses different filter than data query
- **Fix**: Ensure `count_by_time_bucket` uses same filter as `get_by_time_bucket`
- **Location**: [core/src/metadata/services/group.rs:245-262](../core/src/metadata/services/group.rs)

**Error: TypeError: Cannot read property 'length' of undefined**
- **Symptom**: Frontend crashes when accessing array fields
- **Root Cause**: Backend returned JSON strings instead of arrays
- **Fix**: Ensure handler uses `GroupResponse` struct and transforms data
- **Location**: [gateway/src/handlers/group.rs:139-143](../gateway/src/handlers/group.rs)

**Error: Spans from different runs mixed in timeline**
- **Symptom**: Timeline shows confusing span relationships
- **Root Cause**: TimelineContent assumes single run, but bucket contains multiple runs
- **Fix**: TimelineContent now handles multiple runs with composite `runId`
- **Location**: [ui/src/components/chat/traces/components/TimelineContent.tsx:34-46](../ui/src/components/chat/traces/components/TimelineContent.tsx)

### How to Add New Bucket Size Option

1. Update `BucketSize` type in TracesPageContext.tsx:
```typescript
export type BucketSize = 300 | 600 | /* new value */ | 3600 | ...
```

2. Add option to BUCKET_OPTIONS in GroupingSelector.tsx:
```typescript
const BUCKET_OPTIONS = [
  { value: '900', label: '15m' },  // New option
  { value: '3600', label: '1h' },
  // ...
];
```

3. No backend changes needed - bucket size is a parameter

### How to Debug SQL Queries

Enable Diesel query logging to see actual SQL:
```rust
// In core/src/metadata/services/group.rs
use diesel::debug_query;
println!("{}", debug_query::<diesel::sqlite::Sqlite, _>(&query));
```

---

## Related Documentation

- [Traces API](./traces.md) - Individual trace querying
- [Runs API](./runs.md) - Run-based trace organization
- [Spans API](./SPANS_API.md) - Detailed span information
