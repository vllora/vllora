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
    └── TracesPageContent
        ├── GroupingSelector (Two separate controls)
        │   ├── View Toggle: Run | Bucket
        │   └── Bucket Size Toggle: 5min | 15min | 1hr | 3hr | 1day (shown when Bucket selected)
        │
        └── RunTable (Container that delegates to sub-components)
            ├── (When groupByMode === "run")
            │   └── RunTableView → RunTableRow (Display individual runs as table)
            │
            └── (When groupByMode === "bucket")
                └── GroupCardGrid → GroupCard (Display time-bucketed groups as cards)
                    └── TimelineContent (Display all spans in bucket)
```

#### State Management

**Location:** [ui/src/contexts/TracesPageContext.tsx](../ui/src/contexts/TracesPageContext.tsx)

**Core State:**
```typescript
// Grouping mode: "run" or "bucket"
const [groupByMode, setGroupByMode] = useState<"run" | "bucket">("run");

// Bucket size in seconds (300, 900, 3600, 10800, 86400)
const [bucketSize, setBucketSize] = useState<number>(300);

// Currently opened groups (by time_bucket)
const [openGroups, setOpenGroups] = useState<{ time_bucket: number; tab: string }[]>([]);

// Loading state for each time bucket
const [loadingGroupsByTimeBucket, setLoadingGroupsByTimeBucket] = useState<Set<number>>(new Set());

// Computed value: Groups spans by time bucket (not state!)
const groupSpansMap = useMemo(() => {
  const bucket_size_us = bucketSize * 1_000_000;
  return flattenSpans.reduce((acc, span) => {
    if (span.start_time_us) {
      const timeBucket = Math.floor(span.start_time_us / bucket_size_us) * bucket_size_us;
      if (!acc[timeBucket]) {
        acc[timeBucket] = [];
      }
      acc[timeBucket].push(span);
    }
    return acc;
  }, {} as Record<number, Span[]>);
}, [flattenSpans, bucketSize]);
```

**Key Functions:**

1. **loadSpansByBucketGroup(timeBucket: number)** - Loads spans for a specific time bucket

```typescript
// Ref to track loading state (prevents stale closure reads)
const loadingGroupsByTimeBucketRef = useRef<Set<number>>(new Set());

// Sync ref with state
useEffect(() => {
  loadingGroupsByTimeBucketRef.current = loadingGroupsByTimeBucket;
}, [loadingGroupsByTimeBucket]);

const loadSpansByBucketGroup = useCallback(async (timeBucket: number) => {
  // CRITICAL: Use ref in guard check to avoid stale closure reads
  if (loadingGroupsByTimeBucketRef.current.has(timeBucket)) {
    return;
  }

  setLoadingGroupsByTimeBucket(prev => new Set(prev).add(timeBucket));

  try {
    const response = await fetchSpansByBucketGroup({
      timeBucket,
      projectId,
      bucketSize,
      limit: 100,
      offset: 0,
    });

    // Add spans to flattenSpans - groupSpansMap will auto-update via useMemo
    updateBySpansArray(response.data);
  } catch (error: any) {
    toast.error("Failed to load group spans", {
      description: error.message || "An error occurred while loading group spans",
    });
  } finally {
    setLoadingGroupsByTimeBucket(prev => {
      const newSet = new Set(prev);
      newSet.delete(timeBucket);
      return newSet;
    });
  }
}, [projectId, bucketSize, flattenSpans]);
```

**Key Architectural Changes:**

✅ **groupSpansMap is computed, not state**
- Previously: `setGroupSpansMap(prev => ({ ...prev, [timeBucket]: response.data }))`
- Now: Add to `flattenSpans`, `groupSpansMap` recomputes automatically via `useMemo`
- Benefit: Single source of truth, no synchronization bugs

✅ **Uses updateBySpansArray from useWrapperHook**
- Consistent pattern with run mode
- Handles upsert logic (update existing, insert new)
- Reusable across different contexts

✅ **No duplicate data**
- All spans live in `flattenSpans`
- `groupSpansMap` is just a view/grouping of that data
- Changes to `flattenSpans` automatically reflect in `groupSpansMap`

2. **refreshSingleBucket(timeBucket: number)** - Refreshes bucket metadata from backend

```typescript
const refreshSingleBucket = useCallback(async (timeBucket: number) => {
  try {
    const updatedBucket = await fetchSingleBucket({
      timeBucket,
      projectId,
      bucketSize,
    });

    if (updatedBucket) {
      setGroups(prev => prev.map(g =>
        g.time_bucket === timeBucket ? updatedBucket : g
      ));
      console.log('Refreshed bucket stats for:', timeBucket);
    }
  } catch (error) {
    console.error('Failed to refresh bucket stats:', error);
  }
}, [projectId, bucketSize, setGroups]);
```

**Use Case:**
- Called when a run finishes (`RunFinished` or `RunError` events)
- Fetches updated aggregated stats (cost, tokens, errors) from backend
- Updates only the specific bucket in the groups list

**Why Backend Fetch Instead of Manual Calculation:**
- ✅ Backend has authoritative aggregated stats via SQL
- ✅ No duplication of aggregation logic
- ✅ Handles edge cases (concurrent runs, partial data)
- ❌ Previously: Calculated stats manually from `groupSpansMap` (removed)

**Real-time Flow:**
1. Span event arrives → Added to `flattenSpans`
2. Run completes → Call `refreshSingleBucket(timeBucket)`
3. Backend re-aggregates all spans in that bucket
4. Frontend receives updated stats and displays them

#### Detail Span - No Override Needed

**Location:** [ui/src/contexts/TracesPageContext.tsx](../ui/src/contexts/TracesPageContext.tsx)

The `detailSpan` from `useWrapperHook` works for both run and bucket modes without any override:

```typescript
// Get detailSpan directly from useWrapperHook
const {
  detailSpan,  // Searches flattenSpans - works for both modes!
  detailSpanId,
  setDetailSpanId,
  flattenSpans,
  // ... other values
} = useWrapperHook({ projectId });

// No override needed - just return it directly
return {
  // ... other values
  detailSpan,  // Works in both run and bucket modes
};
```

**Why No Override is Needed:**

✅ **groupSpansMap is computed from flattenSpans**
- `groupSpansMap = useMemo(() => flattenSpans.reduce(...))`
- All spans in `groupSpansMap` **must exist** in `flattenSpans` first
- `detailSpan` searches `flattenSpans` and finds everything

✅ **Single source of truth**
- Run mode spans: Live in `flattenSpans`
- Bucket mode spans: Also live in `flattenSpans`
- `groupSpansMap`: Just a computed grouping/view of `flattenSpans`

**Previous Architecture (Removed in refactoring):**
- ❌ `groupSpansMap` was separate state: `useState<Record<number, Span[]>>({})`
- ❌ Required `detailSpanOverride` to search both `flattenSpans` and `groupSpansMap`
- ❌ Duplicate data, synchronization bugs possible

**Current Architecture (Simple & Clean):**
- ✅ `groupSpansMap` is computed: `useMemo(() => flattenSpans.reduce(...))`
- ✅ No override needed - single search location
- ✅ Impossible to have data mismatch between the two

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
export async function fetchSpansByBucketGroup(params: FetchGroupSpansParams): Promise<PaginatedResponse<Span>>

// Fetch metadata for a single bucket (for refreshing stats)
export async function fetchSingleBucket(params: {
  timeBucket: number;
  projectId: string;
  bucketSize: number;
}): Promise<GroupDTO | null>
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

Two separate toggle controls for mode and bucket size selection:

```typescript
const BUCKET_OPTIONS = [
  { value: 300, label: '5min' },
  { value: 900, label: '15min' },
  { value: 3600, label: '1hr' },
  { value: 10800, label: '3hr' },
  { value: 86400, label: '1day' },
];

// View mode toggle
<div className="inline-flex items-center gap-3">
  <span className="text-sm font-medium text-muted-foreground">View:</span>
  <ToggleGroup type="single" value={groupByMode} onValueChange={onGroupByModeChange}>
    <ToggleGroupItem value="run">Run</ToggleGroupItem>
    <ToggleGroupItem value="bucket">Bucket</ToggleGroupItem>
  </ToggleGroup>
</div>

// Bucket size toggle (only shown when bucket mode is selected)
{groupByMode === 'bucket' && (
  <div className="inline-flex items-center gap-3">
    <span className="text-sm font-medium text-muted-foreground">Bucket size:</span>
    <ToggleGroup type="single" value={String(bucketSize)} onValueChange={onBucketSizeChange}>
      {BUCKET_OPTIONS.map((option) => (
        <ToggleGroupItem key={option.value} value={String(option.value)}>
          {option.label}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  </div>
)}
```

**Key Features:**
- Progressive disclosure: Bucket size toggle only appears when "Bucket" mode is selected
- Cleaner options: Reduced from 10 options to 5 commonly-used time ranges
- Better UX: Separate controls make the mode/size relationship clearer

##### 2. RunTable - Container Component

**Location:** [ui/src/pages/chat/traces/run-table.tsx](../ui/src/pages/chat/traces/run-table.tsx)

Main container that handles loading/error states and delegates rendering to mode-specific sub-components:

**Implementation:**
```typescript
export function RunTable() {
  const {
    groupByMode,
    runs, groups,
    // ... loading states
  } = TracesPageConsumer();

  // ... loading/error/empty state handling

  return (
    <div className="flex-1 w-full h-full overflow-auto">
      {groupByMode === 'run' ? (
        <RunTableView
          runs={runs}
          hasMore={hasMore}
          loadingMore={loadingMore}
          onLoadMore={loadMore}
          observerRef={observerTarget}
        />
      ) : (
        <GroupCardGrid
          groups={groups}
          hasMore={hasMore}
          loadingMore={loadingMore}
          onLoadMore={loadMore}
          observerRef={observerTarget}
        />
      )}
    </div>
  );
}
```

##### 3. RunTableView - Table Display for Run Mode

**Location:** [ui/src/pages/chat/traces/run-table-view.tsx](../ui/src/pages/chat/traces/run-table-view.tsx)

Displays runs as a traditional table with sticky header:

**Key Features:**
- Table layout with sticky header
- Uses `RunTableHeader` and `RunTableRow` components
- Pagination support with "Load More" button

##### 4. GroupCardGrid - Card Display for Bucket Mode

**Location:** [ui/src/pages/chat/traces/group-card-grid.tsx](../ui/src/pages/chat/traces/group-card-grid.tsx)

Displays time buckets as cards instead of table rows:

**Key Features:**
- Card-based layout (no table structure)
- No header row (each card has inline labels)
- Better visual separation between buckets
- Responsive grid layout

**Implementation:**
```typescript
export function GroupCardGrid({ groups, hasMore, loadingMore, onLoadMore, observerRef }) {
  return (
    <div className="px-6 py-4">
      <div className="grid grid-cols-1 gap-4">
        {groups.map((group, index) => (
          <GroupCard key={group.time_bucket} group={group} index={index} />
        ))}
        {/* Load More button */}
      </div>
    </div>
  );
}
```

##### 5. GroupCard - Individual Bucket Card

**Location:** [ui/src/pages/chat/traces/group-card.tsx](../ui/src/pages/chat/traces/group-card.tsx)

Displays a single time bucket as a card with collapsible content:

**Key Features:**
- Card UI with rounded borders and shadows
- Compact single-row layout with time on left, stats on right
- Smart time display:
  - Today: Just shows time (e.g., "1:10 PM")
  - Yesterday: Shows "Yesterday, 1:10 PM"
  - This year: Shows date without year (e.g., "Oct 29, 1:10 PM")
  - Previous years: Shows full date with year
- Fixed-width grid columns for consistent alignment across cards
- Inline labels (Provider, Cost, Input, Output, Duration, Status)
- Status badges with icons (success ✓ or error ⚠️ with count)
- Auto-loads spans when expanded
- Shows all spans in unified timeline

**Grid Layout:**
```typescript
const CARD_STATS_GRID = 'auto 100px 100px 100px 100px 80px';
// Provider | Cost | Input | Output | Duration | Status
```

**Implementation:**
```typescript
export const GroupCard: React.FC<GroupCardProps> = ({ group, index }) => {
  const {
    openGroups,
    setOpenGroups,
    loadGroupSpans,
    groupSpansMap,
    loadingGroupsByTimeBucket,
  } = TracesPageConsumer();

  const timeBucket = group.time_bucket;
  const isOpen = openGroups.some(g => g.time_bucket === timeBucket);
  const allSpans = groupSpansMap[timeBucket] || [];

  // Auto-load spans when card is expanded
  useEffect(() => {
    if (isOpen && allSpans.length === 0 && !isLoadingSpans) {
      loadGroupSpans(timeBucket);
    }
  }, [isOpen, timeBucket]);

  return (
    <motion.div className="rounded-lg border border-border bg-[#0a0a0a]">
      {/* Card header with time and stats */}
      <div onClick={toggleAccordion} className="cursor-pointer p-4">
        <div className="flex items-center justify-between gap-6">
          {/* Left: Expand button + time */}
          <div className="flex items-center gap-3 flex-1">
            <ChevronIcon />
            <div>
              <h3>{formatSmartTime(timeBucket)}</h3>
              <span className="text-xs text-muted-foreground">{timeAgo}</span>
            </div>
          </div>

          {/* Right: Stats grid */}
          <div className="grid items-center gap-4" style={{ gridTemplateColumns: CARD_STATS_GRID }}>
            {/* Provider, Cost, Input, Output, Duration, Status with inline labels */}
          </div>
        </div>
      </div>

      {/* Expanded content */}
      {isOpen && <TimelineContent spansByRunId={allSpans} {...otherProps} />}
    </motion.div>
  );
};
```

**Why Cards Instead of Table Rows:**
- Better visual separation between time buckets
- More scannable with inline labels
- Cleaner UI without needing a header row
- Fixed column widths ensure alignment across cards
- Works better with animations and transitions

---

## Data Flow

### 1. User Selects Bucket Mode

```
User clicks "Bucket" in View toggle
    ↓
TracesPageContext.setGroupByMode('bucket')
    ↓
Bucket size selector appears (default: 5min/300s)
    ↓
User selects bucket size (5min, 15min, 1hr, 3hr, or 1day)
    ↓
TracesPageContext.setBucketSize(300)  // 5 minutes = 300 seconds
    ↓
useGroupsPagination hook triggers refetch
    ↓
API: GET /group?bucketSize=300&limit=20&offset=0
    ↓
Backend returns list of groups with aggregated stats
    ↓
Groups displayed in UI as GroupCard components (card layout)
```

### 2. User Expands a Group

```
User clicks on a GroupCard
    ↓
GroupCard.toggleAccordion()
    ↓
setOpenGroups([{ time_bucket: 1737100800000000, tab: 'trace' }])
    ↓
GroupCard.useEffect detects isOpen = true
    ↓
TracesPageContext.loadGroupSpans(1737100800000000)
    ↓
Check cache and loading state
    ↓
API: GET /group/1737100800000000?bucketSize=300&limit=100&offset=0
    ↓
Backend returns ALL spans (root + children) in that time bucket
    ↓
Cache all spans in groupSpansMap[1737100800000000]
    ↓
GroupCard.allSpans updates (directly from groupSpansMap)
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
  → Displays RunTableView → RunTableRow components (table layout)
  → Bucket size selector hidden

// Bucket Mode
groupByMode === "bucket"
  → useGroupsPagination hook active
  → Fetches time-bucketed groups
  → Displays GroupCardGrid → GroupCard components (card layout)
  → Bucket size selector visible
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

## Real-time Event Integration

The trace grouping feature fully supports real-time events in both run mode and bucket mode.

**Location:** [ui/src/contexts/TracesPageContext.tsx:162-237](../ui/src/contexts/TracesPageContext.tsx)

### Implementation

```typescript
const handleEvent = useCallback((event: ProjectEventUnion) => {
  if (event.run_id) {
    // Run mode: Update flattenSpans and run metrics
    if (groupByMode === 'run') {
      // ... existing run mode logic
    }
    // Bucket mode: Calculate bucket and update groupSpansMap
    else if (groupByMode === 'bucket') {
      // Process event to extract span
      const currentSpans = flattenSpans;
      const updatedSpans = processEvent(currentSpans, event);
      const newSpan = updatedSpans.find(s => !currentSpans.find(cs => cs.span_id === s.span_id));

      if (newSpan && newSpan.start_time_us) {
        // Calculate which bucket this span belongs to
        const bucket_size_us = bucketSize * 1_000_000;
        const timeBucket = Math.floor(newSpan.start_time_us / bucket_size_us) * bucket_size_us;

        // Check if this bucket exists in the groups list
        const bucketExists = groups.some(g => g.time_bucket === timeBucket);

        // If bucket doesn't exist, refresh groups to show it (auto-discovery)
        if (!bucketExists) {
          console.log('New bucket detected, refreshing groups list:', timeBucket);
          setTimeout(() => {
            refreshGroups();
          }, 50);
        }

        // Check if this bucket is currently opened
        const isOpen = openGroups.some(g => g.time_bucket === timeBucket);

        if (isOpen) {
          // Add or update span in the opened bucket
          setGroupSpansMap(prev => {
            const existingSpans = prev[timeBucket] || [];
            // Check if span already exists (update it)
            if (existingSpans.some(s => s.span_id === newSpan.span_id)) {
              return {
                ...prev,
                [timeBucket]: existingSpans.map(s =>
                  s.span_id === newSpan.span_id ? newSpan : s
                ),
              };
            }
            // Add new span
            return {
              ...prev,
              [timeBucket]: [...existingSpans, newSpan],
            };
          });
        }

        // Update flattenSpans for any components that need it
        setFlattenSpans(updatedSpans);
      }

      // Refresh groups list when run finishes to update aggregated stats
      if (event.type === 'RunFinished' || event.type === 'RunError') {
        setTimeout(() => {
          refreshGroups();
        }, 100);
      }
    }
  }
}, [groupByMode, flattenSpans, bucketSize, openGroups, refreshGroups, ...]);
```

### How It Works

**When a new span event arrives:**

1. **Calculate Time Bucket**
   - Extract `start_time_us` from the span
   - Calculate: `timeBucket = floor(start_time_us / bucket_size_us) * bucket_size_us`
   - This determines which bucket the span belongs to

2. **Auto-Create and Open Optimistic Bucket**
   - Check if this bucket exists in the `groups` list
   - If not found: **Create optimistic bucket** with initial data from the span
   - Cannot refresh from DB yet because span hasn't been committed
   - Bucket appears immediately with placeholder stats (0 cost, no tokens)
   - Sorted and inserted in correct position (desc by time_bucket)
   - **Auto-opens the bucket** so user sees the real-time span immediately

3. **Check if Bucket is Open**
   - Look in `openGroups` array to see if this bucket is currently expanded
   - Only update `groupSpansMap` if the bucket is visible to the user

4. **Update or Insert Span**
   - If span already exists (by `span_id`): **Update** it (handles span updates)
   - If span is new: **Insert** it into the bucket's span array
   - This triggers a re-render of `GroupTableRow` and `TimelineContent`

5. **Update Bucket Stats on Run Completion**
   - When `RunFinished` or `RunError` events arrive
   - Calculate accurate stats from spans in `groupSpansMap[timeBucket]`
   - Update only the specific bucket (not whole list)
   - Aggregates: thread_ids, trace_ids, run_ids, root_span_ids, cost, tokens, errors
   - This ensures the group row shows accurate totals without DB query

### Key Benefits

✅ **Instant Bucket Creation & Auto-Open**: New buckets appear and expand immediately
✅ **Zero DB Queries on Completion**: Stats calculated from in-memory spans
✅ **Efficient**: Only updates opened buckets and specific bucket stats
✅ **No Duplicates**: Checks for existing spans before adding
✅ **Handles Updates**: Updates existing spans when they change (e.g., span finishes)
✅ **Accurate Aggregation**: Bucket stats updated from actual spans
✅ **Best-in-Class UX**: New spans visible immediately, zero waiting

### Testing Real-time Events

To test real-time event handling in bucket mode:

**Test 1: New Bucket Auto-Creation**
1. Open the application in bucket mode (e.g., select "1h")
2. Trigger a trace event for a time period that doesn't have a bucket yet
3. **Expected**: New bucket appears immediately in the list (sorted correctly)
4. **Expected**: Bucket is auto-opened and shows the timeline
5. **Expected**: Span appears in the timeline in real-time

**Test 2: Existing Bucket Updates**
1. Open the application in bucket mode
2. Expand an existing time bucket group
3. Trigger trace events that fall into that bucket's time range
4. **Expected**: New spans appear in the timeline automatically
5. **Expected**: When run finishes, group row updates stats (cost, tokens, errors)

**Test 3: Stats Accuracy**
1. Watch a bucket as spans arrive in real-time
2. When the run finishes, check the bucket row
3. **Expected**: Cost, tokens, and errors match the sum of all spans shown
4. **Expected**: No extra DB queries were made (check network tab)

## Future Enhancements

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
   - Verify UI changes from table (RunTableView) to cards (GroupCardGrid)
   - Verify bucket size selector appears/disappears when switching modes
   - Verify progressive disclosure (bucket size only shown in bucket mode)
2. Test different bucket sizes (5min, 15min, 1hr, 3hr, 1day)
3. Test group expansion and span loading in cards
4. Test pagination with large datasets
5. Test empty states (no groups, no spans)
6. Verify smart time display in cards:
   - Today: Shows only time (e.g., "1:10 PM")
   - Yesterday: Shows "Yesterday, 1:10 PM"
   - This year: Shows date without year
   - Previous years: Shows full date with year
7. Verify card stats alignment across multiple cards
8. Test status badges (success icon vs error icon with count)

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

### Issue: Cards not displaying in bucket mode

**Check:**
1. Verify `groupByMode === 'bucket'` in TracesPageContext
2. Check that `GroupCardGrid` component is being rendered (not `RunTableView`)
3. Verify `groups` array has data
4. Check browser console for component errors

### Issue: Card stats not aligning across multiple cards

**Check:**
1. Verify `CARD_STATS_GRID` constant is defined and used consistently
2. Check that all GroupCard components use the same grid template
3. Inspect gridTemplateColumns in browser DevTools
4. Ensure no cards have custom overrides

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

**Real-time span event arrives (bucket mode):**
```
1. Real-time event received (e.g., SpanCreated, SpanUpdated)
2. handleEvent extracts span from event
3. Calculate timeBucket = floor(start_time_us / bucket_size_us) * bucket_size_us
4. Check: Does this bucket exist in groups list?
5a. If NO: Create optimistic bucket with placeholder stats
    → Bucket appears immediately in UI (sorted correctly)
    → Auto-open the bucket (add to openGroups)
    → User sees new bucket expanded, ready to show spans
5b. If YES: Continue
6. Check: Is this bucket currently opened?
7a. If YES: Add/update span in groupSpansMap[timeBucket]
    → Timeline automatically re-renders with new span
7b. If NO: Skip (don't process unopened buckets)
    (Note: New buckets from step 5a are auto-opened, so will hit 7a)
8. Update flattenSpans for compatibility
9. If RunFinished/RunError: Calculate stats from groupSpansMap[timeBucket]
    → Update specific bucket (thread_ids, cost, tokens, errors)
    → No DB query needed!
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

**Error: SpanDetailPanel doesn't show in bucket mode**
- **Symptom**: Clicking on a span in bucket mode doesn't open the detail panel
- **Root Cause**: `detailSpan` is computed from `flattenSpans` only, but bucket mode uses `groupSpansMap`
- **Fix**: Override `detailSpan` to search both `flattenSpans` and `groupSpansMap`
- **Location**: [ui/src/contexts/TracesPageContext.tsx:98-116](../ui/src/contexts/TracesPageContext.tsx)
- **How to verify**: Click on any span in a time bucket, detail panel should appear with span info

### How to Add New Bucket Size Option

1. Update `BucketSize` type in TracesPageContext.tsx:
```typescript
export type BucketSize = 300 | 900 | 3600 | /* new value */ | 10800 | 86400;
```

2. Add option to BUCKET_OPTIONS in GroupingSelector.tsx:
```typescript
const BUCKET_OPTIONS = [
  { value: 300, label: '5min' },
  { value: 900, label: '15min' },
  { value: 1800, label: '30min' },  // New option
  { value: 3600, label: '1hr' },
  { value: 10800, label: '3hr' },
  { value: 86400, label: '1day' },
];
```

3. No backend changes needed - bucket size is a parameter
4. Consider UI space - too many options can make the toggle group crowded

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
