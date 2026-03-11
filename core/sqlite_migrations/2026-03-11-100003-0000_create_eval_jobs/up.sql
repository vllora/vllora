CREATE TABLE eval_jobs (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    cloud_run_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    sample_size INTEGER,
    rollout_model TEXT,
    error TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    CONSTRAINT valid_eval_status CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled'))
);

CREATE INDEX idx_eval_jobs_workflow ON eval_jobs(workflow_id);
CREATE INDEX idx_eval_jobs_status ON eval_jobs(status);
