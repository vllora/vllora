-- Drop indexes first
DROP INDEX IF EXISTS idx_project_model_restrictions_tag;
DROP INDEX IF EXISTS idx_project_model_restrictions_project;

-- Drop the table
DROP TABLE IF EXISTS project_model_restrictions;
