CREATE TABLE knowledge_sources_new (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    metadata TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    deleted_at TEXT
);

INSERT INTO knowledge_sources_new (id, workflow_id, name, description, metadata, created_at, deleted_at)
SELECT
    id,
    workflow_id,
    name,
    content AS description,
    progress AS metadata,
    created_at,
    deleted_at
FROM knowledge_sources;

DROP TABLE knowledge_sources;
ALTER TABLE knowledge_sources_new RENAME TO knowledge_sources;
CREATE INDEX idx_knowledge_sources_workflow ON knowledge_sources(workflow_id);
