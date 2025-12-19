-- Remove custom_inference_api_type column from providers table
-- Note: SQLite doesn't support DROP COLUMN directly, so we need to recreate the table
-- This is a simplified version - in production you might want to preserve data

CREATE TABLE providers_backup AS SELECT 
    id, 
    provider_name, 
    description, 
    endpoint, 
    priority, 
    privacy_policy_url, 
    terms_of_service_url, 
    created_at, 
    updated_at, 
    is_active
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
    is_active INTEGER NOT NULL DEFAULT 1
);

INSERT INTO providers SELECT 
    id, 
    provider_name, 
    description, 
    endpoint, 
    priority, 
    privacy_policy_url, 
    terms_of_service_url, 
    created_at, 
    updated_at, 
    is_active
FROM providers_backup;

DROP TABLE providers_backup;
