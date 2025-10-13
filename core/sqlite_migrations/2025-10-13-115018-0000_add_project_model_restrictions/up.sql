-- Create project_model_restrictions table
CREATE TABLE project_model_restrictions (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    tag_type TEXT NOT NULL,
    tag TEXT NOT NULL,
    allowed_models TEXT NOT NULL DEFAULT '[]',      -- JSON array
    disallowed_models TEXT NOT NULL DEFAULT '[]',  -- JSON array
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX idx_project_model_restrictions_project
    ON project_model_restrictions(project_id);
CREATE INDEX idx_project_model_restrictions_tag
    ON project_model_restrictions(tag_type, tag);
