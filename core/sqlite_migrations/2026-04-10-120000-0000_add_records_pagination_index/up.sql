-- Covering index for paginated record listing: eliminates TEMP B-TREE sort.
-- Without this, ORDER BY created_at DESC on 7000+ records (avg 180KB each)
-- forces SQLite to read ~1.3GB just to sort before returning 200 rows.
CREATE INDEX IF NOT EXISTS idx_workflow_records_workflow_created
    ON workflow_records(workflow_id, created_at DESC);
