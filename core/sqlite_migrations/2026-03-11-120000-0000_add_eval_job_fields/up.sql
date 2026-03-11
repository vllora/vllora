ALTER TABLE eval_jobs ADD COLUMN completed_at TEXT;
ALTER TABLE eval_jobs ADD COLUMN started_at TEXT;
ALTER TABLE eval_jobs ADD COLUMN polling_snapshot TEXT;
ALTER TABLE eval_jobs ADD COLUMN result TEXT;
