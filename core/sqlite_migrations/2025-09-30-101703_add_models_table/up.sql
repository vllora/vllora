-- Your SQL goes here
CREATE TABLE models (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    model_name TEXT NOT NULL,
    description TEXT,
    provider_name TEXT NOT NULL,
    model_type TEXT NOT NULL,
    input_token_price REAL,
    output_token_price REAL,
    context_size INTEGER,
    capabilities TEXT, -- JSON array stored as text
    input_types TEXT, -- JSON array stored as text
    output_types TEXT, -- JSON array stored as text
    tags TEXT, -- JSON array stored as text
    type_prices TEXT, -- JSON object stored as text
    mp_price REAL,
    model_name_in_provider TEXT,
    owner_name TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    parameters TEXT, -- JSON object stored as text
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    deleted_at TEXT,
    benchmark_info TEXT, -- JSON object stored as text
    cached_input_token_price REAL,
    cached_input_write_token_price REAL,
    release_date TEXT,
    langdb_release_date TEXT,
    knowledge_cutoff_date TEXT,
    license TEXT,
    project_id TEXT -- Foreign key to projects table
);

-- Create indexes for common queries
CREATE INDEX idx_models_model_name ON models(model_name);
CREATE INDEX idx_models_provider_info_id ON models(provider_name);
CREATE INDEX idx_models_model_type ON models(model_type);
CREATE INDEX idx_models_owner_name ON models(owner_name);
CREATE INDEX idx_models_deleted_at ON models(deleted_at);
CREATE INDEX idx_models_project_id ON models(project_id);