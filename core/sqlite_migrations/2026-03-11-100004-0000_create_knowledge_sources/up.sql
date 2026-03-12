CREATE TABLE knowledge_sources (
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

CREATE INDEX idx_knowledge_sources_workflow ON knowledge_sources(workflow_id);
