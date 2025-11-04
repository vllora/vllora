# Thread Grouping Implementation Status

## ✅ Phase 1: Backend Implementation (COMPLETED)

### 1. Query Parameters Updated
**File:** `gateway/src/handlers/group.rs`
- ✅ Added `group_by: Option<String>` to `ListGroupQueryParams`
- ✅ Supports `groupBy` as camelCase alias via serde

### 2. Generic Response Structures Created
**File:** `gateway/src/handlers/group.rs`
- ✅ Created `GroupByKey` enum with discriminated union pattern
  - Supports `Time { time_bucket: i64 }`
  - Supports `Thread { thread_id: String }`
  - Ready for future grouping types (Model, User, etc.)
- ✅ Created `GenericGroupResponse` struct
  - Uses `#[serde(flatten)]` to embed grouping key
  - Returns `group_by` and `group_key` fields in JSON
- ✅ Implemented `From<GroupUsageInformation>` trait for automatic conversion

### 3. Database Model Updated
**File:** `core/src/metadata/services/group.rs`
- ✅ Updated `GroupUsageInformation` struct
  - `time_bucket: Option<i64>` (populated when group_by=time)
  - `thread_id: Option<String>` (populated when group_by=thread)
  - Single struct handles all grouping types!

### 4. Service Layer Enhanced
**File:** `core/src/metadata/services/group.rs`
- ✅ Added `GroupBy` enum (Time, Thread)
- ✅ Added `group_by: GroupBy` field to `ListGroupQuery`
- ✅ Updated `list_root_group()` to handle both grouping types
  - Dynamic SQL generation based on `group_by`
  - Time grouping: Groups by calculated time buckets
  - Thread grouping: Groups by `thread_id` field
- ✅ Updated `count_root_group()` to count correct entities
  - Time: Counts distinct time buckets
  - Thread: Counts distinct thread IDs

### 5. Handler Updated
**File:** `gateway/src/handlers/group.rs`
- ✅ Parse `group_by` parameter (defaults to "time")
- ✅ Validate parameter ("time" or "thread" only)
- ✅ Pass `GroupBy` enum to service layer
- ✅ Return `GenericGroupResponse` instead of `GroupResponse`

---

## 📊 API Response Examples

### Time Grouping (existing behavior)
```bash
GET /group?group_by=time&bucketSize=3600
```

**Response:**
```json
{
  "pagination": { "offset": 0, "limit": 100, "total": 5 },
  "data": [
    {
      "group_by": "time",
      "group_key": {
        "time_bucket": 1737100800000000
      },
      "thread_ids": ["thread-1", "thread-2"],
      "trace_ids": ["trace-1"],
      "run_ids": ["run-1"],
      "cost": 0.05,
      "llm_calls": 3,
      ...
    }
  ]
}
```

### Thread Grouping (NEW feature)
```bash
GET /group?group_by=thread
```

**Response:**
```json
{
  "pagination": { "offset": 0, "limit": 100, "total": 10 },
  "data": [
    {
      "group_by": "thread",
      "group_key": {
        "thread_id": "thread-abc-123"
      },
      "trace_ids": ["trace-1", "trace-2"],
      "run_ids": ["run-1"],
      "cost": 0.10,
      "llm_calls": 5,
      ...
    }
  ]
}
```

---

## 🧪 Backend Testing

### Test 1: Time Grouping (Backward Compatibility)
```bash
# Default behavior (no group_by parameter)
curl "http://localhost:8080/group?bucketSize=3600&limit=10" \
  -H "x-project-id: default"

# Expected: Returns time-bucketed groups (existing behavior)
```

### Test 2: Thread Grouping (NEW)
```bash
# Group by thread
curl "http://localhost:8080/group?group_by=thread&limit=10" \
  -H "x-project-id: default"

# Expected: Returns thread-grouped data
# Each group has group_by="thread" and group_key.thread_id
```

### Test 3: Invalid Parameter
```bash
# Invalid group_by value
curl "http://localhost:8080/group?group_by=invalid" \
  -H "x-project-id: default"

# Expected: 400 Bad Request with error message
```

### Test 4: Thread Filtering
```bash
# Get specific threads
curl "http://localhost:8080/group?group_by=thread&threadIds=thread-1,thread-2" \
  -H "x-project-id: default"

# Expected: Only returns those two threads
```

---

## 🔄 Compilation Status

✅ **Backend compiles successfully!**

```
Checking vllora_core v1.0.0
Checking vllora_guardrails v1.0.0
Checking vllora v1.0.0
Finished `dev` profile
```

Only warning: `GroupResponse` struct unused (deprecated in favor of `GenericGroupResponse`)

---

## 📝 Remaining Tasks

### Backend Tasks
1. ⏳ **Add endpoint for fetching spans by thread ID**
   - Similar to `GET /group/{time_bucket}`
   - New: `GET /group/thread/{thread_id}`
   - Or unified: `GET /group/key/{key_value}?group_by=thread`

2. ⏳ **Test endpoints with real data**
   - Start backend server
   - Test with curl commands
   - Verify JSON response format

### Frontend Tasks
3. ⏳ **Update TypeScript types**
   - Add `GenericGroupDTO` interface
   - Add type guards (`isTimeGroup`, `isThreadGroup`)
   - Update API service functions

4. ⏳ **Update UI components**
   - Add "Thread" option to GroupingSelector
   - Create unified group card component
   - Handle both grouping types in display

5. ⏳ **Update state management**
   - Add thread grouping to TracesPageContext
   - Handle thread group state
   - Integrate with existing hooks

6. ⏳ **End-to-end testing**
   - Manual UI testing
   - Verify data flow
   - Test edge cases

---

## 🎯 Design Benefits Achieved

✅ **Single Generic Response** - No need for separate response types
✅ **Extensible** - Adding new grouping types is trivial
✅ **Type Safe** - Rust enums provide compile-time guarantees
✅ **Backward Compatible** - Default behavior unchanged
✅ **Self-Documenting** - `group_by` field indicates grouping type
✅ **Clean Code** - No duplication, clear logic

---

## 🚀 Next Steps

1. **Test the backend** with curl to verify correctness
2. **Add span retrieval endpoint** for thread groups
3. **Implement frontend** changes for UI
4. **Integration testing** with real traces
5. **Documentation** update for API consumers

---

**Status:** Backend implementation complete and compiling ✅
**Next:** Backend testing and span retrieval endpoint
**Updated:** 2025-11-04
