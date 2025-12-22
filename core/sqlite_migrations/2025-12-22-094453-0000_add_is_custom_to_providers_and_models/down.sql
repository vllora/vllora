-- Remove is_custom column from providers table
-- SQLite doesn't support DROP COLUMN directly, so we need to recreate the table
CREATE TABLE providers_backup AS SELECT 
    id, provider_name, description, endpoint, priority, 
    privacy_policy_url, terms_of_service_url, created_at, updated_at, 
    is_active, custom_inference_api_type
FROM providers;

DROP TABLE providers;

CREATE TABLE providers (
    id TEXT PRIMARY KEY DEFAULT (
        lower(hex(randomblob(4))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(6)))
    ) NOT NULL,
    provider_name TEXT NOT NULL UNIQUE,
    description TEXT,
    endpoint TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    privacy_policy_url TEXT,
    terms_of_service_url TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    is_active INTEGER NOT NULL DEFAULT 1,
    custom_inference_api_type TEXT
);

INSERT INTO providers SELECT 
    id, provider_name, description, endpoint, priority, 
    privacy_policy_url, terms_of_service_url, created_at, updated_at, 
    is_active, custom_inference_api_type
FROM providers_backup;

DROP TABLE providers_backup;

-- Remove is_custom column from models table
CREATE TABLE models_backup AS SELECT 
    id, model_name, description, provider_name, model_type,
    input_token_price, output_token_price, context_size, capabilities,
    input_types, output_types, tags, type_prices, mp_price,
    model_name_in_provider, owner_name, priority, parameters,
    created_at, updated_at, deleted_at, benchmark_info,
    cached_input_token_price, cached_input_write_token_price,
    release_date, langdb_release_date, knowledge_cutoff_date,
    license, project_id, endpoint
FROM models;

DROP TABLE models;

CREATE TABLE models (
    id TEXT PRIMARY KEY DEFAULT (
        lower(hex(randomblob(4))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(6)))
    ) NOT NULL,
    model_name TEXT NOT NULL,
    description TEXT,
    provider_name TEXT NOT NULL,
    model_type TEXT NOT NULL,
    input_token_price REAL,
    output_token_price REAL,
    context_size INTEGER,
    capabilities TEXT,
    input_types TEXT,
    output_types TEXT,
    tags TEXT,
    type_prices TEXT,
    mp_price REAL,
    model_name_in_provider TEXT,
    owner_name TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    parameters TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    deleted_at TEXT,
    benchmark_info TEXT,
    cached_input_token_price REAL,
    cached_input_write_token_price REAL,
    release_date TEXT,
    langdb_release_date TEXT,
    knowledge_cutoff_date TEXT,
    license TEXT,
    project_id TEXT,
    endpoint TEXT
);

INSERT INTO models SELECT 
    id, model_name, description, provider_name, model_type,
    input_token_price, output_token_price, context_size, capabilities,
    input_types, output_types, tags, type_prices, mp_price,
    model_name_in_provider, owner_name, priority, parameters,
    created_at, updated_at, deleted_at, benchmark_info,
    cached_input_token_price, cached_input_write_token_price,
    release_date, langdb_release_date, knowledge_cutoff_date,
    license, project_id, endpoint
FROM models_backup;

DROP TABLE models_backup;
