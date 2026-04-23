-- Drop the traceability columns. SQLite DROP COLUMN is supported from 3.35.0+;
-- guard with try/catch-equivalent by recreating the table if older SQLite.
--
-- Preferred path (SQLite ≥ 3.35):
ALTER TABLE workflow_records DROP COLUMN origin_uri;
ALTER TABLE workflow_records DROP COLUMN origin_source_id;
