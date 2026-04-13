-- Additive migration: new trace_bundles table + nullable FK on knowledge_sources.
-- Part of the OTel trace finetune pipeline (Track A).
-- See docs/workflow-skill-first-approach/trace-pipeline-implementation-plan.md

CREATE TABLE trace_bundles (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    name TEXT NOT NULL,
    span_count INTEGER NOT NULL DEFAULT 0,
    tool_names TEXT NOT NULL DEFAULT '[]',
    model_names TEXT NOT NULL DEFAULT '[]',
    raw_blob BLOB NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL
);

CREATE INDEX idx_trace_bundles_workflow ON trace_bundles(workflow_id);

-- Nullable FK on knowledge_sources so an OTel-trace-backed source can point at its bundle.
-- SQLite does not enforce ADD COLUMN FK constraints; this is a logical FK only (matches existing
-- conventions in this repo, cf. workflow_topic_sources.source_id).
ALTER TABLE knowledge_sources ADD COLUMN trace_bundle_id TEXT;
CREATE INDEX idx_knowledge_sources_trace_bundle ON knowledge_sources(trace_bundle_id);
