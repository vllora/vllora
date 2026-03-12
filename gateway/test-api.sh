#!/usr/bin/env bash
# End-to-end test script for the finetune CRUD API
# Usage: ./test-api.sh [base_url]
#   base_url defaults to http://localhost:9090

set -euo pipefail

BASE="${1:-http://localhost:9090}"
PASS=0
FAIL=0
TESTS=()

# ─── Helpers ───────────────────────────────────────────────────────────────────

green() { printf "\033[32m%s\033[0m\n" "$1"; }
red()   { printf "\033[31m%s\033[0m\n" "$1"; }
bold()  { printf "\033[1m%s\033[0m\n" "$1"; }

# assert_status <test_name> <expected_status> <actual_status> <response_body>
assert_status() {
  local name="$1" expected="$2" actual="$3" body="$4"
  if [ "$actual" -eq "$expected" ]; then
    green "  ✓ $name (HTTP $actual)"
    PASS=$((PASS + 1))
  else
    red "  ✗ $name — expected $expected, got $actual"
    red "    Response: $body"
    FAIL=$((FAIL + 1))
  fi
  TESTS+=("$name:$actual:$expected")
}

# assert_json_field <test_name> <response_body> <jq_expression> <expected_value>
assert_json_field() {
  local name="$1" body="$2" jq_expr="$3" expected="$4"
  local actual
  actual=$(echo "$body" | jq -r "$jq_expr" 2>/dev/null || echo "PARSE_ERROR")
  if [ "$actual" = "$expected" ]; then
    green "  ✓ $name ($jq_expr = $expected)"
    PASS=$((PASS + 1))
  else
    red "  ✗ $name — $jq_expr: expected '$expected', got '$actual'"
    FAIL=$((FAIL + 1))
  fi
  TESTS+=("$name:$actual:$expected")
}

# curl wrapper that returns "status_code\nbody"
api() {
  local method="$1" path="$2"
  shift 2
  local response
  response=$(curl -s -w "\n%{http_code}" -X "$method" "$BASE$path" \
    -H "Content-Type: application/json" "$@" 2>&1)
  echo "$response"
}

parse_status() { echo "$1" | tail -1; }
parse_body()   { echo "$1" | sed '$d'; }

# Generate a short UUID-like ID
gen_id() { python3 -c "import uuid; print(str(uuid.uuid4())[:8])"; }

# ─── Health Check ──────────────────────────────────────────────────────────────

bold "=== Checking gateway at $BASE ==="
HEALTH_RESP=$(api GET /finetune/workflows)
HEALTH_STATUS=$(parse_status "$HEALTH_RESP")
if [ "$HEALTH_STATUS" != "200" ]; then
  red "Gateway not reachable at $BASE (got HTTP $HEALTH_STATUS)"
  red "Start the gateway first: cargo run -- serve (or npm run start:backend)"
  exit 1
fi
green "Gateway is up!"
echo ""

# ─── 1. Workflow CRUD ─────────────────────────────────────────────────────────

bold "=== 1. Workflow CRUD ==="

# Create
RESP=$(api POST /finetune/workflows -d '{"name":"Test Workflow","objective":"API test"}')
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Create workflow" 201 "$STATUS" "$BODY"

WF_ID=$(echo "$BODY" | jq -r '.id')
assert_json_field "Workflow has name" "$BODY" '.name' "Test Workflow"
assert_json_field "Workflow has objective" "$BODY" '.objective' "API test"
echo "  → workflow_id=$WF_ID"

# Get
RESP=$(api GET "/finetune/workflows/$WF_ID")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Get workflow" 200 "$STATUS" "$BODY"
assert_json_field "Get returns correct name" "$BODY" '.name' "Test Workflow"

# Update
RESP=$(api PUT "/finetune/workflows/$WF_ID" -d '{"name":"Updated Workflow"}')
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Update workflow" 200 "$STATUS" "$BODY"
assert_json_field "Update reflects new name" "$BODY" '.name' "Updated Workflow"

# List
RESP=$(api GET /finetune/workflows)
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "List workflows" 200 "$STATUS" "$BODY"
echo ""

# ─── 2. Knowledge Sources CRUD ────────────────────────────────────────────────

bold "=== 2. Knowledge Sources CRUD ==="

# Create
RESP=$(api POST "/finetune/workflows/$WF_ID/knowledge" \
  -d '{"name":"test-doc.pdf","type":"pdf","content":"hello world","extracted_content":{"chunks":["chunk1","chunk2"]}}')
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Create knowledge source" 201 "$STATUS" "$BODY"

KS_ID=$(echo "$BODY" | jq -r '.id')
assert_json_field "KS has name" "$BODY" '.name' "test-doc.pdf"
echo "  → ks_id=$KS_ID"

# Get
RESP=$(api GET "/finetune/workflows/$WF_ID/knowledge/$KS_ID")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Get knowledge source" 200 "$STATUS" "$BODY"

# List
RESP=$(api GET "/finetune/workflows/$WF_ID/knowledge")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "List knowledge sources" 200 "$STATUS" "$BODY"
assert_json_field "List returns array" "$BODY" '.knowledge_sources | length' "1"

# Count
RESP=$(api GET "/finetune/workflows/$WF_ID/knowledge/count")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Count knowledge sources" 200 "$STATUS" "$BODY"
assert_json_field "Count is 1" "$BODY" '.count' "1"

# Update status
RESP=$(api PATCH "/finetune/workflows/$WF_ID/knowledge/$KS_ID/status" \
  -d '{"status":"ready"}')
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Update KS status" 200 "$STATUS" "$BODY"

# Update chunks
RESP=$(api PATCH "/finetune/workflows/$WF_ID/knowledge/$KS_ID/chunks" \
  -d '{"extracted_content":{"chunks":["new_chunk1","new_chunk2","new_chunk3"]}}')
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Update KS chunks" 200 "$STATUS" "$BODY"

# Verify update
RESP=$(api GET "/finetune/workflows/$WF_ID/knowledge/$KS_ID")
BODY=$(parse_body "$RESP")
assert_json_field "KS status updated" "$BODY" '.status' "ready"

# Verify full KS data correctness
assert_json_field "KS type is pdf" "$BODY" '.type' "pdf"
assert_json_field "KS content stored" "$BODY" '.content' "hello world"
assert_json_field "KS workflow_id correct" "$BODY" '.workflow_id' "$WF_ID"
# extracted_content was updated to new chunks
KS_CHUNKS=$(echo "$BODY" | jq -r '.extracted_content')
if echo "$KS_CHUNKS" | jq -e '.chunks | length == 3' > /dev/null 2>&1; then
  green "  ✓ KS extracted_content has 3 chunks after update"
  PASS=$((PASS + 1))
else
  red "  ✗ KS extracted_content: expected 3 chunks, got: $KS_CHUNKS"
  FAIL=$((FAIL + 1))
fi
echo ""

# ─── 3. Topics CRUD ───────────────────────────────────────────────────────────

bold "=== 3. Topics CRUD ==="

T1=$(gen_id); T2=$(gen_id); T3=$(gen_id)
echo "  → topic IDs: $T1, $T2, $T3"

# Create topics (replace all)
RESP=$(api POST "/finetune/workflows/$WF_ID/topics" -d "{
  \"topics\": [
    {\"id\":\"$T1\",\"name\":\"Math\",\"selected\":true,\"source_chunk_refs\":[\"ref1\"]},
    {\"id\":\"$T2\",\"name\":\"Science\",\"parent_id\":\"$T1\",\"selected\":false},
    {\"id\":\"$T3\",\"name\":\"History\",\"selected\":true}
  ]
}")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Create topics" 201 "$STATUS" "$BODY"
assert_json_field "Created 3 topics" "$BODY" '.created' "3"

# List
RESP=$(api GET "/finetune/workflows/$WF_ID/topics")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "List topics" 200 "$STATUS" "$BODY"
assert_json_field "List returns 3" "$BODY" '.topics | length' "3"

# Delete all
RESP=$(api DELETE "/finetune/workflows/$WF_ID/topics")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Delete all topics" 200 "$STATUS" "$BODY"
assert_json_field "Deleted 3" "$BODY" '.deleted' "3"

# Verify empty
RESP=$(api GET "/finetune/workflows/$WF_ID/topics")
BODY=$(parse_body "$RESP")
assert_json_field "Topics now empty" "$BODY" '.topics | length' "0"

# Re-create for records tests
T4=$(gen_id); T5=$(gen_id)
api POST "/finetune/workflows/$WF_ID/topics" -d "{
  \"topics\": [{\"id\":\"$T4\",\"name\":\"Math\",\"selected\":true},{\"id\":\"$T5\",\"name\":\"Science\",\"selected\":true}]
}" > /dev/null
echo ""

# ─── 4. Records CRUD ──────────────────────────────────────────────────────────

bold "=== 4. Records CRUD ==="

R1=$(gen_id); R2=$(gen_id); R3=$(gen_id)
echo "  → record IDs: $R1, $R2, $R3"

# Add records
RESP=$(api POST "/finetune/workflows/$WF_ID/records" -d "{
  \"records\": [
    {\"id\":\"$R1\",\"data\":{\"messages\":[{\"role\":\"user\",\"content\":\"What is 2+2?\"},{\"role\":\"assistant\",\"content\":\"4\"}]},\"topic\":\"Math\"},
    {\"id\":\"$R2\",\"data\":{\"messages\":[{\"role\":\"user\",\"content\":\"What is gravity?\"},{\"role\":\"assistant\",\"content\":\"A force\"}]},\"topic\":\"Science\"},
    {\"id\":\"$R3\",\"data\":{\"messages\":[{\"role\":\"user\",\"content\":\"Generated Q\"},{\"role\":\"assistant\",\"content\":\"Generated A\"}]},\"topic\":\"Math\",\"is_generated\":true,\"source_record_id\":\"$R1\"}
  ]
}")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Add records" 201 "$STATUS" "$BODY"
assert_json_field "Added 3 records" "$BODY" '.added' "3"

# List
RESP=$(api GET "/finetune/workflows/$WF_ID/records")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "List records" 200 "$STATUS" "$BODY"
assert_json_field "List returns 3" "$BODY" '.records | length' "3"

# Update single record topic
RESP=$(api PATCH "/finetune/workflows/$WF_ID/records/$R1" -d '{"topic":"Science"}')
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Update record topic" 200 "$STATUS" "$BODY"

# Batch update topics
RESP=$(api PATCH "/finetune/workflows/$WF_ID/records/topics" -d "{
  \"updates\": [{\"record_id\":\"$R1\",\"topic\":\"Math\"},{\"record_id\":\"$R2\",\"topic\":\"Math\"}]
}")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Batch update topics" 200 "$STATUS" "$BODY"

# Update record data
RESP=$(api PATCH "/finetune/workflows/$WF_ID/records/$R1/data" -d '{"data":"{\"messages\":[{\"role\":\"user\",\"content\":\"Updated Q\"}]}"}')
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Update record data" 200 "$STATUS" "$BODY"

# Update record scores
RESP=$(api PATCH "/finetune/workflows/$WF_ID/records/$R1/scores" -d '{"dry_run_score":0.85,"finetune_score":0.92}')
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Update record scores" 200 "$STATUS" "$BODY"

# ── Verify data correctness after all mutations ──
RESP=$(api GET "/finetune/workflows/$WF_ID/records")
BODY=$(parse_body "$RESP")
# R1 should have: topic=Math (from batch update), updated data, scores set
R1_DATA=$(echo "$BODY" | jq -r --arg id "$R1" '.records[] | select(.id == $id)')
assert_json_field "R1 topic after batch update" "$R1_DATA" '.topic' "Math"
assert_json_field "R1 data updated" "$R1_DATA" '.data' '{"messages":[{"role":"user","content":"Updated Q"}]}'
R1_DRY=$(echo "$R1_DATA" | jq '.dry_run_score')
R1_FT=$(echo "$R1_DATA" | jq '.finetune_score')
if [ "$R1_DRY" = "0.85" ] || [ "$R1_DRY" = "0.8500000238418579" ]; then
  green "  ✓ R1 dry_run_score ≈ 0.85 (got $R1_DRY)"
  PASS=$((PASS + 1))
else
  red "  ✗ R1 dry_run_score: expected ~0.85, got $R1_DRY"
  FAIL=$((FAIL + 1))
fi
if [ "$R1_FT" = "0.92" ] || [ "$R1_FT" = "0.9200000166893005" ]; then
  green "  ✓ R1 finetune_score ≈ 0.92 (got $R1_FT)"
  PASS=$((PASS + 1))
else
  red "  ✗ R1 finetune_score: expected ~0.92, got $R1_FT"
  FAIL=$((FAIL + 1))
fi
# R2 should have: topic=Math (from batch update)
R2_DATA=$(echo "$BODY" | jq -r --arg id "$R2" '.records[] | select(.id == $id)')
assert_json_field "R2 topic after batch update" "$R2_DATA" '.topic' "Math"
# R3 should have: is_generated=1, source_record_id=$R1
R3_DATA=$(echo "$BODY" | jq -r --arg id "$R3" '.records[] | select(.id == $id)')
assert_json_field "R3 is_generated" "$R3_DATA" '.is_generated' "1"
assert_json_field "R3 source_record_id" "$R3_DATA" '.source_record_id' "$R1"

# Rename topic
RESP=$(api PATCH "/finetune/workflows/$WF_ID/records/rename-topic" -d '{"old_name":"Math","new_name":"Mathematics"}')
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Rename topic" 200 "$STATUS" "$BODY"

# Clear specific topic
RESP=$(api DELETE "/finetune/workflows/$WF_ID/records/topics/Mathematics")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Clear topic 'Mathematics'" 200 "$STATUS" "$BODY"

# Delete single record
RESP=$(api DELETE "/finetune/workflows/$WF_ID/records/$R3")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Delete single record" 200 "$STATUS" "$BODY"

# Replace all records
R10=$(gen_id)
RESP=$(api PUT "/finetune/workflows/$WF_ID/records" -d "{
  \"records\": [
    {\"id\":\"$R10\",\"data\":{\"messages\":[{\"role\":\"user\",\"content\":\"Fresh Q\"}]},\"topic\":\"Science\"}
  ]
}")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Replace all records" 200 "$STATUS" "$BODY"

# Verify replacement
RESP=$(api GET "/finetune/workflows/$WF_ID/records")
BODY=$(parse_body "$RESP")
assert_json_field "After replace, 1 record" "$BODY" '.records | length' "1"

# Delete all
RESP=$(api DELETE "/finetune/workflows/$WF_ID/records")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Delete all records" 200 "$STATUS" "$BODY"
echo ""

# ─── 5. Eval Jobs CRUD ────────────────────────────────────────────────────────

bold "=== 5. Eval Jobs CRUD ==="

# Create
RESP=$(api POST "/finetune/workflows/$WF_ID/eval-jobs" -d '{
  "cloud_run_id":"run_abc123","sample_size":100,"rollout_model":"gpt-4o-mini"
}')
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Create eval job" 201 "$STATUS" "$BODY"

JOB_ID=$(echo "$BODY" | jq -r '.id')
assert_json_field "Job has cloud_run_id" "$BODY" '.cloud_run_id' "run_abc123"
assert_json_field "Job status is pending" "$BODY" '.status' "pending"
echo "  → job_id=$JOB_ID"

# Get
RESP=$(api GET "/finetune/workflows/$WF_ID/eval-jobs/$JOB_ID")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Get eval job" 200 "$STATUS" "$BODY"

# List by workflow
RESP=$(api GET "/finetune/workflows/$WF_ID/eval-jobs")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "List eval jobs by workflow" 200 "$STATUS" "$BODY"
assert_json_field "Has 1 job" "$BODY" '.jobs | length' "1"

# List by status (cross-workflow)
RESP=$(api GET "/finetune/eval-jobs?status=pending")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "List eval jobs by status" 200 "$STATUS" "$BODY"

# Update status
RESP=$(api PATCH "/finetune/workflows/$WF_ID/eval-jobs/$JOB_ID" -d '{"status":"running"}')
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Update eval job status" 200 "$STATUS" "$BODY"
assert_json_field "Status is running" "$BODY" '.status' "running"

# Update with error
RESP=$(api PATCH "/finetune/workflows/$WF_ID/eval-jobs/$JOB_ID" -d '{"status":"failed","error":"timeout"}')
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Update eval job with error" 200 "$STATUS" "$BODY"
assert_json_field "Status is failed" "$BODY" '.status' "failed"
assert_json_field "Error message set" "$BODY" '.error' "timeout"

# Verify full eval job data
RESP=$(api GET "/finetune/workflows/$WF_ID/eval-jobs/$JOB_ID")
BODY=$(parse_body "$RESP")
assert_json_field "Job workflow_id correct" "$BODY" '.workflow_id' "$WF_ID"
assert_json_field "Job sample_size correct" "$BODY" '.sample_size' "100"
assert_json_field "Job rollout_model correct" "$BODY" '.rollout_model' "gpt-4o-mini"
assert_json_field "Job status persisted as failed" "$BODY" '.status' "failed"

# Create a second job for delete-by-workflow test
api POST "/finetune/workflows/$WF_ID/eval-jobs" -d '{"sample_size":50}' > /dev/null

# Delete single
RESP=$(api DELETE "/finetune/workflows/$WF_ID/eval-jobs/$JOB_ID")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Delete eval job" 200 "$STATUS" "$BODY"

# Delete by workflow
RESP=$(api DELETE "/finetune/workflows/$WF_ID/eval-jobs")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Delete workflow eval jobs" 200 "$STATUS" "$BODY"
echo ""

# ─── 6. Error Cases ───────────────────────────────────────────────────────────

bold "=== 6. Error Cases ==="

# 404 on non-existent workflow
RESP=$(api GET "/finetune/workflows/nonexistent-id-12345")
STATUS=$(parse_status "$RESP")
assert_status "Get non-existent workflow → 404" 404 "$STATUS" ""

# 404 on non-existent eval job
RESP=$(api GET "/finetune/workflows/$WF_ID/eval-jobs/nonexistent-job")
STATUS=$(parse_status "$RESP")
assert_status "Get non-existent eval job → 404" 404 "$STATUS" ""

# 404 on non-existent knowledge source
RESP=$(api GET "/finetune/workflows/$WF_ID/knowledge/nonexistent-ks")
STATUS=$(parse_status "$RESP")
assert_status "Get non-existent KS → 404" 404 "$STATUS" ""
echo ""

# ─── 7. Cleanup ───────────────────────────────────────────────────────────────

bold "=== 7. Cleanup ==="

# Soft delete workflow
RESP=$(api DELETE "/finetune/workflows/$WF_ID")
STATUS=$(parse_status "$RESP"); BODY=$(parse_body "$RESP")
assert_status "Soft delete workflow" 200 "$STATUS" "$BODY"
assert_json_field "Deleted flag true" "$BODY" '.deleted' "true"
echo ""

# ─── Summary ───────────────────────────────────────────────────────────────────

bold "═══════════════════════════════════════"
bold "  Results: $PASS passed, $FAIL failed"
bold "═══════════════════════════════════════"

if [ "$FAIL" -gt 0 ]; then
  red "Some tests failed!"
  exit 1
else
  green "All tests passed!"
  exit 0
fi
