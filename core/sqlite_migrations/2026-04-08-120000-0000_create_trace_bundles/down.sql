-- Reverse of up.sql. Drops the index + new column on knowledge_sources via table rebuild,
-- then drops the trace_bundles table.

DROP INDEX IF EXISTS idx_knowledge_sources_trace_bundle;

CREATE TABLE knowledge_sources_old (
    id TEXT PRIMARY KEY NOT NULL,
    reference_id TEXT,
    workflow_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    metadata TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    deleted_at TEXT
);

INSERT INTO knowledge_sources_old (id, reference_id, workflow_id, name, description, metadata, created_at, deleted_at)
SELECT id, reference_id, workflow_id, name, description, metadata, created_at, deleted_at
FROM knowledge_sources;

DROP TABLE knowledge_sources;
ALTER TABLE knowledge_sources_old RENAME TO knowledge_sources;
CREATE INDEX idx_knowledge_sources_workflow ON knowledge_sources(workflow_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_sources_reference_id ON knowledge_sources(reference_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_sources_workflow_reference_id
ON knowledge_sources(workflow_id, reference_id);

DROP INDEX IF EXISTS idx_trace_bundles_workflow;
DROP TABLE IF EXISTS trace_bundles;
