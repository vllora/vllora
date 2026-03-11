CREATE TABLE knowledge (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    metadata TEXT,
    description TEXT
);

CREATE INDEX idx_knowledge_workflow_id ON knowledge(workflow_id);
