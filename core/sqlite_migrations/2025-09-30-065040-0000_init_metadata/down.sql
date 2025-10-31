-- This file should undo anything in `up.sql`
-- Drop indexes first

-- traces
DROP INDEX IF EXISTS idx_traces_child_lookup;
DROP INDEX IF EXISTS idx_traces_parent_span_id;
DROP INDEX IF EXISTS idx_traces_finish_time_us;
DROP INDEX IF EXISTS idx_traces_start_time_us;
DROP INDEX IF EXISTS idx_traces_project_id;
DROP INDEX IF EXISTS idx_traces_run_id;
DROP INDEX IF EXISTS idx_traces_trace_id;

-- models
DROP INDEX IF EXISTS idx_models_project_id;
DROP INDEX IF EXISTS idx_models_deleted_at;
DROP INDEX IF EXISTS idx_models_owner_name;
DROP INDEX IF EXISTS idx_models_model_type;
DROP INDEX IF EXISTS idx_models_provider_info_id;
DROP INDEX IF EXISTS idx_models_model_name;

-- provider_credentials
DROP INDEX IF EXISTS idx_provider_credentials_lookup;
DROP INDEX IF EXISTS idx_provider_credentials_is_active;
DROP INDEX IF EXISTS idx_provider_credentials_provider_project;
DROP INDEX IF EXISTS idx_provider_credentials_project_id;
DROP INDEX IF EXISTS idx_provider_credentials_provider_name;

-- providers
DROP INDEX IF EXISTS idx_providers_priority;
DROP INDEX IF EXISTS idx_providers_is_active;
DROP INDEX IF EXISTS idx_providers_provider_name;

-- mcp_configs
DROP INDEX IF EXISTS idx_mcp_configs_company_slug;

-- Drop tables (children before parents)
DROP TABLE IF EXISTS sessions;
DROP TABLE IF EXISTS mcp_configs;
DROP TABLE IF EXISTS providers;
DROP TABLE IF EXISTS provider_credentials;
DROP TABLE IF EXISTS models;
DROP TABLE IF EXISTS traces;
DROP TABLE IF EXISTS projects;