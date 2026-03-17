-- SQLite doesn't support DROP COLUMN directly (added in 3.35.0+).
-- Recreate the table without polling_snapshot.

CREATE TABLE eval_jobs_new (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    cloud_run_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    sample_size INTEGER,
    rollout_model TEXT,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    completed_at TEXT,
    started_at TEXT,
    result TEXT
);

INSERT INTO eval_jobs_new (id, workflow_id, cloud_run_id, status, sample_size, rollout_model, error, created_at, updated_at, completed_at, started_at, result)
SELECT id, workflow_id, cloud_run_id, status, sample_size, rollout_model, error, created_at, updated_at, completed_at, started_at, result
FROM eval_jobs;

DROP TABLE eval_jobs;
ALTER TABLE eval_jobs_new RENAME TO eval_jobs;
