CREATE TABLE workflow_logs (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    target TEXT,
    log TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL
);

CREATE INDEX idx_workflow_logs_workflow_id ON workflow_logs(workflow_id);
CREATE INDEX idx_workflow_logs_created_at ON workflow_logs(created_at);
