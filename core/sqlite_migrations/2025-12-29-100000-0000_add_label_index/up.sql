-- Add index for filtering traces by label (stored in attribute JSON)
-- This improves performance of GET /spans?labels=... and GET /labels queries
CREATE INDEX IF NOT EXISTS idx_traces_label ON traces(project_id, json_extract(attribute, '$.label'))
WHERE json_extract(attribute, '$.label') IS NOT NULL;
