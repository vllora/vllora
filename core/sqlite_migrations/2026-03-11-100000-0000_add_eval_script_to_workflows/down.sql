-- SQLite doesn't support DROP COLUMN before 3.35.0
-- Recreate without eval_script
CREATE TABLE workflows_backup AS SELECT id, name, objective, created_at, updated_at, deleted_at FROM workflows;
DROP TABLE workflows;
CREATE TABLE workflows (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    objective TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    deleted_at TEXT
);
INSERT INTO workflows SELECT * FROM workflows_backup;
DROP TABLE workflows_backup;
CREATE INDEX idx_workflows_deleted_at ON workflows(deleted_at);
