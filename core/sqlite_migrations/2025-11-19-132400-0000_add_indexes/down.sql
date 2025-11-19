DROP INDEX IF EXISTS idx_traces_title_first;
DROP INDEX IF EXISTS idx_traces_project_thread;

UPDATE traces
SET attribute = json_remove(COALESCE(attribute, '{}'), '$.title')
WHERE operation_name = 'api_invoke'
  AND json_extract(attribute, '$.title') IS NOT NULL;