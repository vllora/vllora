-- Create experiments table to track experimental request variations
CREATE TABLE experiments (
    id TEXT PRIMARY KEY DEFAULT (
        lower(hex(randomblob(4))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(6)))
    ) NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    original_span_id TEXT NOT NULL,
    original_trace_id TEXT NOT NULL,
    original_request TEXT NOT NULL, -- JSON stored as text
    modified_request TEXT NOT NULL, -- JSON stored as text
    headers TEXT, -- JSON stored as text
    prompt_variables TEXT, -- JSON stored as text (Mustache variables)
    model_parameters TEXT, -- JSON stored as text (temperature, max_tokens, etc.)
    result_span_id TEXT, -- Span ID of the experiment result
    result_trace_id TEXT, -- Trace ID of the experiment result
    status TEXT NOT NULL DEFAULT 'draft', -- draft, running, completed, failed
    project_id TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

-- Create indexes for common query patterns
CREATE INDEX idx_experiments_project_id ON experiments(project_id);
CREATE INDEX idx_experiments_original_span_id ON experiments(original_span_id);
CREATE INDEX idx_experiments_original_trace_id ON experiments(original_trace_id);
CREATE INDEX idx_experiments_status ON experiments(status);
CREATE INDEX idx_experiments_created_at ON experiments(created_at);
