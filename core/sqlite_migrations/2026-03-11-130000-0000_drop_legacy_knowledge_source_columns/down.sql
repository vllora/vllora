CREATE TABLE knowledge_sources_old (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    content TEXT,
    extracted_content TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    progress TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    deleted_at TEXT,
    CONSTRAINT valid_ks_type CHECK (type IN ('pdf', 'image', 'url', 'text', 'markdown')),
    CONSTRAINT valid_ks_status CHECK (status IN ('pending', 'processing', 'ready', 'failed'))
);

INSERT INTO knowledge_sources_old (
    id,
    workflow_id,
    name,
    type,
    content,
    extracted_content,
    status,
    progress,
    created_at,
    deleted_at
)
SELECT
    id,
    workflow_id,
    name,
    'text' AS type,
    description AS content,
    NULL AS extracted_content,
    'ready' AS status,
    metadata AS progress,
    created_at,
    deleted_at
FROM knowledge_sources;

DROP TABLE knowledge_sources;
ALTER TABLE knowledge_sources_old RENAME TO knowledge_sources;
CREATE INDEX idx_knowledge_sources_workflow ON knowledge_sources(workflow_id);
