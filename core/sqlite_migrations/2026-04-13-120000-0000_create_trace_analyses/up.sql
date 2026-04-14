-- Trace analysis results for the trace-informed curriculum feature.
-- Stores the 4 artifacts produced by trace_analyze.py, keyed by workflow_id.
-- One row per workflow (upsert semantics). Old workflows have no row (backward compatible).
CREATE TABLE IF NOT EXISTS trace_analyses (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    priority_json TEXT,
    topics_json TEXT,
    prompts_json TEXT,
    grader_hints_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_trace_analyses_workflow ON trace_analyses(workflow_id);
