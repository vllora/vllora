CREATE TABLE workflow_topics (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    name TEXT NOT NULL,
    parent_id TEXT,
    selected INTEGER NOT NULL DEFAULT 1,
    source_chunk_refs TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    FOREIGN KEY (parent_id) REFERENCES workflow_topics(id)
);

CREATE INDEX idx_workflow_topics_workflow ON workflow_topics(workflow_id);
CREATE INDEX idx_workflow_topics_parent ON workflow_topics(parent_id);
