-- This file should undo anything in `up.sql`
drop table if exists projects;
drop table if exists threads;
drop table if exists messages;
drop table if exists traces;
drop table if exists models;

-- This file should undo anything in `up.sql`
-- Drop indexes first
DROP INDEX IF EXISTS idx_provider_credentials_lookup;
DROP INDEX IF EXISTS idx_provider_credentials_is_active;
DROP INDEX IF EXISTS idx_provider_credentials_provider_project;
DROP INDEX IF EXISTS idx_provider_credentials_project_id;
DROP INDEX IF EXISTS idx_provider_credentials_provider_name;

-- Drop the table
DROP TABLE IF EXISTS provider_credentials;

DROP INDEX IF EXISTS idx_providers_priority;
DROP INDEX IF EXISTS idx_providers_is_active;
DROP INDEX IF EXISTS idx_providers_provider_name;
DROP TABLE IF EXISTS providers;