-- Your SQL goes here
create table projects
(
    id                   text     default (lower(hex(randomblob(16)))) not null
        primary key,
    name                 text                                   not null,
    description          text,
    created_at           text     default (datetime('now'))      not null,
    updated_at           text     default (datetime('now'))      not null,
    slug                 text                                   not null
        unique,
    settings             text     default '{"enabled_chat_tracing": true}',
    is_default           integer  default 0                      not null,
    archived_at          text,
    allowed_user_ids     text,
    private_model_prices text
);

-- Your SQL goes here
CREATE TABLE traces (
    trace_id TEXT NOT NULL,
    span_id TEXT NOT NULL,
    thread_id TEXT,
    parent_span_id TEXT,
    operation_name TEXT NOT NULL,
    start_time_us BIGINT NOT NULL,
    finish_time_us BIGINT NOT NULL,
    attribute TEXT NOT NULL, -- JSON stored as text
    run_id TEXT,
    project_id TEXT,
    PRIMARY KEY (trace_id, span_id)
);

-- Create indexes for common query patterns
CREATE INDEX idx_traces_trace_id ON traces(trace_id);
CREATE INDEX idx_traces_run_id ON traces(run_id);
CREATE INDEX idx_traces_project_id ON traces(project_id);
CREATE INDEX idx_traces_start_time_us ON traces(start_time_us);
CREATE INDEX idx_traces_finish_time_us ON traces(finish_time_us);
CREATE INDEX idx_traces_parent_span_id ON traces(parent_span_id);

-- Composite index for child_attribute JOIN query (trace_id, parent_span_id, operation_name, start_time_us)
CREATE INDEX idx_traces_child_lookup ON traces(trace_id, parent_span_id, operation_name, start_time_us);

-- Your SQL goes here
CREATE TABLE models (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))) NOT NULL,
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
    project_id TEXT,
    endpoint TEXT -- Foreign key to projects table
);

-- Add provider_credentials table for storing API keys and credentials
CREATE TABLE provider_credentials (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))) NOT NULL,
    provider_name TEXT NOT NULL,
    provider_type TEXT NOT NULL,
    credentials TEXT NOT NULL, -- JSON serialized credentials
    project_id TEXT, -- NULL for global credentials
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    is_active INTEGER DEFAULT 1 NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE providers (
    id TEXT PRIMARY KEY NOT NULL,
    provider_name TEXT NOT NULL UNIQUE,
    description TEXT,
    endpoint TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    privacy_policy_url TEXT,
    terms_of_service_url TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    is_active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE mcp_configs (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))) NOT NULL,
    company_slug TEXT NOT NULL,
    config TEXT NOT NULL,
    tools TEXT NOT NULL,
    tools_refreshed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY DEFAULT (
        lower(hex(randomblob(4))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(6)))
    ) NOT NULL
);

INSERT INTO sessions DEFAULT VALUES;

CREATE INDEX idx_providers_provider_name ON providers(provider_name);
CREATE INDEX idx_providers_is_active ON providers(is_active);
CREATE INDEX idx_providers_priority ON providers(priority);

-- Create indexes for common query patterns
CREATE INDEX idx_provider_credentials_provider_name ON provider_credentials(provider_name);
CREATE INDEX idx_provider_credentials_project_id ON provider_credentials(project_id);
CREATE INDEX idx_provider_credentials_provider_project ON provider_credentials(provider_name, project_id);
CREATE INDEX idx_provider_credentials_is_active ON provider_credentials(is_active);

-- Composite index for efficient credential lookups
CREATE INDEX idx_provider_credentials_lookup ON provider_credentials(provider_name, project_id, is_active);


-- Create indexes for common queries
CREATE INDEX idx_models_model_name ON models(model_name);
CREATE INDEX idx_models_provider_info_id ON models(provider_name);
CREATE INDEX idx_models_model_type ON models(model_type);
CREATE INDEX idx_models_owner_name ON models(owner_name);
CREATE INDEX idx_models_deleted_at ON models(deleted_at);
CREATE INDEX idx_models_project_id ON models(project_id);

CREATE INDEX idx_mcp_configs_company_slug ON mcp_configs(company_slug);