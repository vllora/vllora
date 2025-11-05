# Trace Grouping Feature Documentation

## Overview

The trace grouping feature allows users to view traces organized in three different modes:
- **Run Mode**: Individual runs displayed as separate entities
- **Time/Bucket Mode**: Traces grouped by time windows (5min, 10min, 1hr, etc.)
- **Thread Mode**: Traces grouped by thread ID

**Key Architecture:**
- **Generic Design**: Single unified API with discriminated union pattern
- **Extensible**: Easy to add new grouping types (model, user, etc.)
- **Type-Safe**: Rust enums + TypeScript discriminated unions
- **Real-time Capable**: Supports live updates for all grouping modes

---

## Architecture

### Backend (Rust)

#### 1. Generic Response Structure

The backend uses a discriminated union pattern to support multiple grouping types through a single API:

```rust
// Discriminated union for grouping keys
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "group_by", content = "group_key")]
pub enum GroupByKey {
    #[serde(rename = "time")]
    Time { time_bucket: i64 },

    #[serde(rename = "thread")]
    Thread { thread_id: String },

    #[serde(rename = "run")]
    Run { run_id: String },
}

// Single response struct for all grouping types
#[derive(Debug, Serialize)]
pub struct GenericGroupResponse {
    #[serde(flatten)]
    pub key: GroupByKey,
    pub thread_ids: Vec<String>,
    pub trace_ids: Vec<String>,
    pub run_ids: Vec<String>,
    pub root_span_ids: Vec<String>,
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

**JSON Response Examples:**

Time grouping:
```json
{
  "group_by": "time",
  "group_key": { "time_bucket": 1737100800000000 },
  "trace_ids": ["trace-1"],
  "cost": 0.05,
  "llm_calls": 3
}
```

Thread grouping:
```json
{
  "group_by": "thread",
  "group_key": { "thread_id": "thread-abc-123" },
  "trace_ids": ["trace-1", "trace-2"],
  "cost": 0.10,
  "llm_calls": 5
}
```

Run grouping:
```json
{
  "group_by": "run",
  "group_key": { "run_id": "run-xyz-789" },
  "trace_ids": ["trace-1"],
  "cost": 0.02,
  "llm_calls": 2
}
```

#### 2. API Endpoints

##### List Groups - `GET /group`

Groups traces based on the specified grouping mode.

**Location:** [gateway/src/handlers/group.rs](../gateway/src/handlers/group.rs)

**Query Parameters:**
- `groupBy` (string, optional): Grouping mode. Values: `time`, `thread`, `run`. Default: `time`
- `bucketSize` (integer, optional): Time bucket size in seconds. Only used when `groupBy=time`. Default: 3600
- `threadIds` (string, optional): Comma-separated thread IDs to filter by
- `traceIds` (string, optional): Comma-separated trace IDs to filter by
- `limit` (integer, optional): Number of results. Default: 100
- `offset` (integer, optional): Pagination offset. Default: 0

**Response Format:**
```json
{
  "pagination": { "offset": 0, "limit": 100, "total": 50 },
  "data": [
    {
      "group_by": "thread",
      "group_key": { "thread_id": "thread-123" },
      "thread_ids": ["thread-123"],
      "trace_ids": ["trace-1", "trace-2"],
      "run_ids": ["run-1"],
      "root_span_ids": ["span-1", "span-2"],
      "used_models": ["openai/gpt-4", "anthropic/claude-3-sonnet"],
      "llm_calls": 5,
      "cost": 0.0234,
      "input_tokens": 1500,
      "output_tokens": 800,
      "start_time_us": 1737100800000000,
      "finish_time_us": 1737104400000000,
      "errors": []
    }
  ]
}
```

##### Get Spans for Group - `GET /group/spans`

Unified endpoint to retrieve spans for any group type.

**Location:** [gateway/src/handlers/group.rs](../gateway/src/handlers/group.rs)

**Query Parameters:**
- `groupBy` (string, required): Grouping type (`time`, `thread`, or `run`)
- `timeBucket` (integer): Required if `groupBy=time`. Timestamp in microseconds
- `threadId` (string): Required if `groupBy=thread`. Thread identifier
- `runId` (string): Required if `groupBy=run`. Run identifier
- `bucketSize` (integer, optional): Time bucket size in seconds. Only for `groupBy=time`. Default: 3600
- `limit` (integer, optional): Number of results. Default: 100
- `offset` (integer, optional): Pagination offset. Default: 0

**Response Format:**
```json
{
  "pagination": { "offset": 0, "limit": 100, "total": 15 },
  "data": [
    {
      "trace_id": "trace-123",
      "span_id": "span-456",
      "thread_id": "thread-789",
      "parent_span_id": null,
      "operation_name": "chat.completion",
      "start_time_us": 1737100800000000,
      "finish_time_us": 1737100805000000,
      "attribute": { "model": "openai/gpt-4", "cost": 0.002 },
      "child_attribute": { "children_count": 3 },
      "run_id": "run-abc"
    }
  ]
}
```

**Important:** Returns ALL spans (root and children) within the group.

#### 3. Backend Implementation Details

**Database Model:**
```rust
#[derive(Debug, Serialize, Deserialize, QueryableByName)]
pub struct GroupUsageInformation {
    // Grouping key fields - one will be populated depending on group_by
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::BigInt>)]
    pub time_bucket: Option<i64>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub thread_id: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub run_id: Option<String>,

    // Aggregated data (same for all grouping types)
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub thread_ids_json: String,
    // ... other aggregated fields
}
```

**Service Layer:**
```rust
pub enum GroupBy {
    Time,
    Thread,
    Run,
}

pub trait GroupService {
    fn list_root_group(&self, query: ListGroupQuery) -> Result<Vec<GroupUsageInformation>>;
}
```

The service layer uses dynamic SQL generation based on the `GroupBy` enum:
- **Time**: `GROUP BY (start_time_us / bucket_size_us) * bucket_size_us`
- **Thread**: `GROUP BY thread_id`
- **Run**: `GROUP BY run_id`

**Handler Layer Transformation:**

The handler transforms `GroupUsageInformation` (with JSON strings) into `GenericGroupResponse` (with typed arrays):

```rust
impl From<GroupUsageInformation> for GenericGroupResponse {
    fn from(group: GroupUsageInformation) -> Self {
        // Parse JSON strings, defaulting to empty arrays on error
        let thread_ids: Vec<String> = serde_json::from_str(&group.thread_ids_json).unwrap_or_default();

        // Determine which grouping key to use
        let key = if let Some(time_bucket) = group.time_bucket {
            GroupByKey::Time { time_bucket }
        } else if let Some(thread_id) = group.thread_id {
            GroupByKey::Thread { thread_id }
        } else if let Some(run_id) = group.run_id {
            GroupByKey::Run { run_id }
        } else {
            panic!("GroupUsageInformation must have a grouping key set")
        };

        Self { key, thread_ids, /* ... */ }
    }
}
```

---

### Frontend (React/TypeScript)

#### Component Architecture

```
TracesPageContext (State Management)
    └── TracesPageContent
        ├── GroupingSelector
        │   ├── View Toggle: Run | Time | Thread
        │   └── Bucket Size Toggle (shown when Time selected)
        │
        └── RunTable (Container)
            ├── (When groupByMode === "run")
            │   └── RunTableView → RunTableRow
            │
            └── (When groupByMode === "bucket" or "thread")
                └── GroupCardGrid → GroupCard
                    └── TimelineContent
```

#### TypeScript Types

```typescript
// Generic Group DTO with discriminated union
export interface GenericGroupDTO {
  group_by: 'time' | 'thread' | 'run';
  group_key: {
    time_bucket?: number;
    thread_id?: string;
    run_id?: string;
  };
  thread_ids: string[];
  trace_ids: string[];
  run_ids: string[];
  root_span_ids: string[];
  used_models: string[];
  llm_calls: number;
  cost: number;
  input_tokens: number | null;
  output_tokens: number | null;
  start_time_us: number;
  finish_time_us: number;
  errors: string[];
}

// Type guards
export function isTimeGroup(group: GenericGroupDTO): boolean {
  return group.group_by === 'time' && group.group_key.time_bucket !== undefined;
}

export function isThreadGroup(group: GenericGroupDTO): boolean {
  return group.group_by === 'thread' && group.group_key.thread_id !== undefined;
}

export function isRunGroup(group: GenericGroupDTO): boolean {
  return group.group_by === 'run' && group.group_key.run_id !== undefined;
}
```

#### State Management

**Location:** [ui/src/contexts/TracesPageContext.tsx](../ui/src/contexts/TracesPageContext.tsx)

**Core State:**
```typescript
// Grouping mode: "run", "bucket" (time), or "thread"
const [groupByMode, setGroupByMode] = useState<GroupByMode>('bucket');

// Bucket size in seconds (only used when groupByMode === 'bucket')
const [bucketSize, setBucketSize] = useState<BucketSize>(300);

// Computed span maps for each grouping type
const groupSpansMap = useMemo(() => {
  const bucket_size_us = bucketSize * 1_000_000;
  return flattenSpans.reduce((acc, span) => {
    if (span.start_time_us) {
      const timeBucket = Math.floor(span.start_time_us / bucket_size_us) * bucket_size_us;
      if (!acc[timeBucket]) acc[timeBucket] = [];
      acc[timeBucket].push(span);
    }
    return acc;
  }, {} as Record<number, Span[]>);
}, [flattenSpans, bucketSize]);

const threadSpansMap = useMemo(() => {
  return flattenSpans.reduce((acc, span) => {
    if (span.thread_id) {
      if (!acc[span.thread_id]) acc[span.thread_id] = [];
      acc[span.thread_id].push(span);
    }
    return acc;
  }, {} as Record<string, Span[]>);
}, [flattenSpans]);

const runSpansMap = useMemo(() => {
  return flattenSpans.reduce((acc, span) => {
    if (span.run_id) {
      if (!acc[span.run_id]) acc[span.run_id] = [];
      acc[span.run_id].push(span);
    }
    return acc;
  }, {} as Record<string, Span[]>);
}, [flattenSpans]);
```

**Key Functions:**

```typescript
// Unified span loading for all group types
const loadGroupSpans = useCallback(async (group: GenericGroupDTO) => {
  let groupKey: string;
  if (isTimeGroup(group)) {
    groupKey = `time-${group.group_key.time_bucket}`;
  } else if (isThreadGroup(group)) {
    groupKey = `thread-${group.group_key.thread_id}`;
  } else if (isRunGroup(group)) {
    groupKey = `run-${group.group_key.run_id}`;
  } else {
    return;
  }

  if (loadingGroupsRef.current.has(groupKey)) return;
  setLoadingGroups(prev => new Set(prev).add(groupKey));

  try {
    let response;
    if (isTimeGroup(group)) {
      response = await fetchGroupSpans({
        projectId, groupBy: 'time',
        timeBucket: group.group_key.time_bucket,
        bucketSize, limit: 100, offset: 0,
      });
    } else if (isThreadGroup(group)) {
      response = await fetchGroupSpans({
        projectId, groupBy: 'thread',
        threadId: group.group_key.thread_id,
        limit: 100, offset: 0,
      });
    } else if (isRunGroup(group)) {
      response = await fetchGroupSpans({
        projectId, groupBy: 'run',
        runId: group.group_key.run_id,
        limit: 100, offset: 0,
      });
    }
    if (response) updateBySpansArray(response.data);
  } finally {
    setLoadingGroups(prev => {
      const newSet = new Set(prev);
      newSet.delete(groupKey);
      return newSet;
    });
  }
}, [projectId, bucketSize, updateBySpansArray]);
```

#### API Service Layer

**Location:** [ui/src/services/groups-api.ts](../ui/src/services/groups-api.ts)

```typescript
// Unified span fetching function
export const fetchGroupSpans = async (props: {
  projectId: string;
  groupBy: 'time' | 'thread' | 'run';
  timeBucket?: number;
  threadId?: string;
  runId?: string;
  bucketSize?: number;
  offset?: number;
  limit?: number;
}): Promise<{ data: Span[]; pagination: Pagination }> => {
  const queryParams = new URLSearchParams({
    groupBy: props.groupBy,
    offset: String(props.offset || 0),
    limit: String(props.limit || 100),
  });

  if (props.groupBy === 'time') {
    if (!props.timeBucket) throw new Error('timeBucket is required');
    queryParams.set('timeBucket', String(props.timeBucket));
    if (props.bucketSize) queryParams.set('bucketSize', String(props.bucketSize));
  } else if (props.groupBy === 'thread') {
    if (!props.threadId) throw new Error('threadId is required');
    queryParams.set('threadId', props.threadId);
  } else if (props.groupBy === 'run') {
    if (!props.runId) throw new Error('runId is required');
    queryParams.set('runId', props.runId);
  }

  const endpoint = `/group/spans?${queryParams.toString()}`;
  const response = await apiClient(endpoint, {
    method: 'GET',
    headers: { 'x-project-id': projectId },
  });

  return handleApiResponse(response);
};
```

#### UI Components

##### GroupingSelector

**Location:** [ui/src/components/traces/GroupingSelector.tsx](../ui/src/components/traces/GroupingSelector.tsx)

```typescript
<div className="flex items-center gap-6">
  {/* View mode toggle */}
  <div className="inline-flex items-center gap-3">
    <ToggleGroup type="single" value={groupByMode} onValueChange={onGroupByModeChange}>
      <ToggleGroupItem value="run">Run</ToggleGroupItem>
      <ToggleGroupItem value="bucket">Time</ToggleGroupItem>
      <ToggleGroupItem value="thread">Thread</ToggleGroupItem>
    </ToggleGroup>
  </div>

  {/* Bucket size selector - only shown when Time mode selected */}
  {groupByMode === 'bucket' && (
    <div className="inline-flex items-center gap-3">
      <span>Duration:</span>
      <ToggleGroup type="single" value={String(bucketSize)} onValueChange={onBucketSizeChange}>
        <ToggleGroupItem value="300">5min</ToggleGroupItem>
        <ToggleGroupItem value="600">10min</ToggleGroupItem>
        <ToggleGroupItem value="3600">1hr</ToggleGroupItem>
        <ToggleGroupItem value="7200">2hr</ToggleGroupItem>
        <ToggleGroupItem value="86400">24hr</ToggleGroupItem>
      </ToggleGroup>
    </div>
  )}
</div>
```

##### GroupCard

**Location:** [ui/src/pages/chat/traces/group-card/index.tsx](../ui/src/pages/chat/traces/group-card/index.tsx)

Unified component that handles all three grouping types:

```typescript
export const GroupCard: React.FC<GroupCardProps> = ({ group }) => {
  const {
    loadGroupSpans,
    groupSpansMap,
    threadSpansMap,
    runSpansMap,
    loadingGroups,
  } = TracesPageConsumer();

  // Get appropriate spans based on group type
  const allSpans = useMemo(() => {
    if (isTimeGroup(group)) {
      return groupSpansMap[group.group_key.time_bucket] || [];
    } else if (isThreadGroup(group)) {
      return threadSpansMap[group.group_key.thread_id] || [];
    } else if (isRunGroup(group)) {
      return runSpansMap[group.group_key.run_id] || [];
    }
    return [];
  }, [group, groupSpansMap, threadSpansMap, runSpansMap]);

  // Display logic based on group type
  const bucketTimeDisplay = useMemo(() => {
    if (isThreadGroup(group)) {
      return `Thread: ${group.group_key.thread_id.substring(0, 8)}...`;
    }
    if (isRunGroup(group)) {
      return `Run: ${group.group_key.run_id.substring(0, 8)}...`;
    }
    // Time group formatting...
    const date = new Date(group.group_key.time_bucket / 1000);
    return formatSmartTime(date);
  }, [group]);

  return (
    <motion.div className="rounded-lg bg-[#0a0a0a]">
      <div onClick={toggleAccordion}>
        <GroupCardHeader
          isOpen={isOpen}
          bucketTimeDisplay={bucketTimeDisplay}
          providersInfo={providersInfo}
          totalCost={totalCost}
          tokensInfo={tokensInfo}
          errors={errors}
          llm_calls={group.llm_calls}
        />
      </div>
      {isOpen && <TimelineContent spansByRunId={allSpans} {...props} />}
    </motion.div>
  );
};
```

---

## Data Flow

### 1. User Selects Grouping Mode

```
User clicks "Thread" in View toggle
    ↓
TracesPageContext.setGroupByMode('thread')
    ↓
useGroupsPagination hook triggers with groupBy='thread'
    ↓
API: GET /group?groupBy=thread&limit=20&offset=0
    ↓
Backend returns thread-grouped data
    ↓
Groups displayed as GroupCard components
```

### 2. User Expands a Group

```
User clicks on a GroupCard
    ↓
GroupCard.toggleAccordion()
    ↓
setHideGroups updates (removes group from hidden list)
    ↓
GroupCard.useEffect detects isOpen = true
    ↓
TracesPageContext.loadGroupSpans(group)
    ↓
API: GET /group/spans?groupBy=thread&threadId=abc-123&limit=100
    ↓
Backend returns ALL spans for that thread
    ↓
updateBySpansArray adds spans to flattenSpans
    ↓
threadSpansMap recomputes via useMemo
    ↓
GroupCard.allSpans updates
    ↓
TimelineContent renders all spans
```

### 3. Switching Between Modes

```typescript
// Run Mode
groupByMode === "run"
  → useRunsPagination hook active
  → Displays RunTableView → RunTableRow components (table layout)

// Time/Bucket Mode
groupByMode === "bucket"
  → useGroupsPagination with groupBy='time'
  → Displays GroupCardGrid → GroupCard components (card layout)
  → Bucket size selector visible

// Thread Mode
groupByMode === "thread"
  → useGroupsPagination with groupBy='thread'
  → Displays GroupCardGrid → GroupCard components (card layout)
  → Bucket size selector hidden
```

---

## Real-time Event Integration

The trace grouping feature fully supports real-time events for all grouping modes.

**Location:** [ui/src/contexts/TracesPageContext.tsx](../ui/src/contexts/TracesPageContext.tsx)

```typescript
const handleEvent = useCallback((event: ProjectEventUnion) => {
  if (event.run_id) {
    const currentSpans = flattenSpans;
    const updatedSpans = processEvent(currentSpans, event);
    const newSpan = updatedSpans.find(s => !currentSpans.find(cs => cs.span_id === s.span_id));

    if (groupByMode === 'bucket' && newSpan?.start_time_us) {
      const bucket_size_us = bucketSize * 1_000_000;
      const timeBucket = Math.floor(newSpan.start_time_us / bucket_size_us) * bucket_size_us;

      // Create optimistic bucket if doesn't exist
      setGroups(prev => {
        const bucketExists = prev.some(g => isTimeGroup(g) && g.group_key.time_bucket === timeBucket);
        if (bucketExists) return prev;

        const optimisticBucket: GenericGroupDTO = {
          group_by: 'time',
          group_key: { time_bucket: timeBucket },
          thread_ids: newSpan.thread_id ? [newSpan.thread_id] : [],
          trace_ids: [newSpan.trace_id],
          // ... other fields
        };
        return [...prev, optimisticBucket].sort((a, b) => {
          const aTime = isTimeGroup(a) ? a.group_key.time_bucket : a.start_time_us;
          const bTime = isTimeGroup(b) ? b.group_key.time_bucket : b.start_time_us;
          return bTime - aTime;
        });
      });

      // Refresh bucket stats when run finishes
      if (event.type === 'RunFinished' || event.type === 'RunError') {
        setTimeout(() => refreshSingleBucketStat(timeBucket), 100);
      }
    }

    // Add spans to flattenSpans - all maps auto-update via useMemo
    setFlattenSpans(updatedSpans);
  }
}, [groupByMode, bucketSize, flattenSpans, setGroups, setFlattenSpans]);
```

**Key Benefits:**
- ✅ Instant bucket/thread/run creation
- ✅ Auto-opens new groups
- ✅ Zero DB queries on event arrival
- ✅ All span maps update automatically via useMemo

---

## Key Design Decisions

### 1. Generic Discriminated Union Pattern

**Rationale:**
- Single response structure for all grouping types
- Type-safe discrimination via `group_by` field
- Easy to add new grouping types (just extend enum)
- No code duplication

**Implementation:**
```rust
#[serde(tag = "group_by", content = "group_key")]
pub enum GroupByKey {
    Time { time_bucket: i64 },
    Thread { thread_id: String },
    Run { run_id: String },
}
```

### 2. Unified API Endpoint

**Rationale:**
- Single `/group` endpoint with `groupBy` parameter
- Single `/group/spans` endpoint for fetching spans
- Consistent API patterns
- Easier to maintain

### 3. Computed Span Maps

**Rationale:**
- `groupSpansMap`, `threadSpansMap`, `runSpansMap` are computed via `useMemo`
- Single source of truth (`flattenSpans`)
- Automatic updates when `flattenSpans` changes
- No synchronization bugs

### 4. Backend Returns All Spans

**Rationale:**
- Single API call instead of 1 + N calls
- Simpler frontend logic
- Better performance
- Backend controls span inclusion logic

### 5. JSON-to-Array Transformation in Handler

**Rationale:**
- Frontend receives properly typed arrays
- No JSON parsing errors in frontend
- Better developer experience
- SQLite's `json_group_array()` returns strings

---

## Adding New Grouping Types

Want to add "group by model"? Here's what to change:

### Backend:

1. Add to `GroupByKey` enum:
```rust
#[serde(rename = "model")]
Model { model_name: String },
```

2. Add to `GroupBy` enum:
```rust
pub enum GroupBy { Time, Thread, Run, Model }
```

3. Update `GroupUsageInformation`:
```rust
pub model_name: Option<String>,
```

4. Update SQL generation in service layer

5. Update conversion logic

### Frontend:

1. Update `GenericGroupDTO`:
```typescript
group_by: 'time' | 'thread' | 'run' | 'model';
group_key: {
  time_bucket?: number;
  thread_id?: string;
  run_id?: string;
  model_name?: string;
};
```

2. Add type guard:
```typescript
export function isModelGroup(group: GenericGroupDTO): boolean {
  return group.group_by === 'model' && group.group_key.model_name !== undefined;
}
```

3. Update components to handle new type

**Total:** ~30 lines of code

---

## Testing

### Backend Tests

```bash
# List time groups (default)
curl "http://localhost:8080/group?bucketSize=3600&limit=10"

# List thread groups
curl "http://localhost:8080/group?groupBy=thread&limit=10"

# List run groups
curl "http://localhost:8080/group?groupBy=run&limit=10"

# Get spans for time bucket
curl "http://localhost:8080/group/spans?groupBy=time&timeBucket=1737100800000000&bucketSize=3600"

# Get spans for thread
curl "http://localhost:8080/group/spans?groupBy=thread&threadId=thread-abc-123"

# Get spans for run
curl "http://localhost:8080/group/spans?groupBy=run&runId=run-xyz-789"
```

### Frontend Tests

1. **Mode Switching**
   - Test Run ↔ Time ↔ Thread mode transitions
   - Verify UI changes appropriately
   - Verify bucket size selector appears/disappears

2. **Group Display**
   - Verify group cards show correct information
   - Verify smart time display for time groups
   - Verify thread ID truncation
   - Verify stats alignment across cards

3. **Group Expansion**
   - Test expanding/collapsing groups
   - Verify spans load correctly
   - Verify loading indicators
   - Verify multiple groups can be open

4. **Pagination**
   - Test "Load More" button
   - Verify correct data loads
   - Verify button disappears when done

5. **Real-time Updates**
   - Test new span arrivals
   - Test group auto-creation
   - Test stats updates

---

## Troubleshooting

### Issue: Spans not loading when group expands

**Check:**
1. Is `groupBy` parameter correct in API call?
2. Is the group key (time_bucket/thread_id/run_id) valid?
3. Check browser console for API errors
4. Verify `loadGroupSpans` is being called

### Issue: Wrong spans appearing in group

**Check:**
1. Verify `groupBy` parameter matches group type
2. Check API response in network tab
3. Verify span maps are computed correctly
4. Check `allSpans` logic in GroupCard

### Issue: Groups not refreshing when mode changes

**Check:**
1. Verify `useGroupsPagination` dependency array includes `groupBy`
2. Check that API calls include correct parameters
3. Verify React Query cache keys include `groupBy`

### Issue: Thread/Run groups showing empty

**Check:**
1. Verify `threadSpansMap`/`runSpansMap` are exported from context
2. Check that GroupCard uses correct span map
3. Verify spans have `thread_id`/`run_id` populated
4. Check backend filtering logic

---

## Performance Considerations

### Database

- Add indexes on grouping columns:
  ```sql
  CREATE INDEX idx_traces_thread_id ON traces(thread_id);
  CREATE INDEX idx_traces_run_id ON traces(run_id);
  CREATE INDEX idx_traces_thread_project ON traces(thread_id, project_id);
  ```

### Frontend

- Span maps use `useMemo` for efficient recomputation
- Only visible groups load spans
- Pagination prevents loading too much data
- Virtual scrolling can be added for very large lists

---

## Future Enhancements

1. **Group by Model** - See traces using specific models
2. **Group by User** - User-based trace organization
3. **Group by Cost Range** - Budget-based grouping
4. **Nested Grouping** - Combine multiple grouping types
5. **Custom Sorting** - Sort by cost, tokens, time, errors
6. **Thread Search** - Filter threads by ID or content
7. **Export** - Export grouped data to CSV/JSON

---

## Related Documentation

- [Runs API](./runs.md)
- [Spans API](./SPANS_API.md)
- [Real-time Events](./events.md)
