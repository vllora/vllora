CREATE TABLE workflows (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    objective TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    deleted_at TEXT
);

CREATE INDEX idx_workflows_deleted_at ON workflows(deleted_at);
