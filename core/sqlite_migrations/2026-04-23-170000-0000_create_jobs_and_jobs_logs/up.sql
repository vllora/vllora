CREATE TABLE jobs (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    job_type TEXT NOT NULL,
    operation TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'queued',
    idempotency_key TEXT,
    request_fingerprint TEXT,
    progress_json TEXT,
    result_ref TEXT,
    error_code TEXT,
    error_message TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE,
    CONSTRAINT valid_job_state CHECK (
        state IN ('queued', 'running', 'completed', 'failed', 'cancelled')
    )
);

CREATE INDEX idx_jobs_state_created_at ON jobs(state, created_at);
CREATE INDEX idx_jobs_job_type_state_created_at ON jobs(job_type, state, created_at);
CREATE INDEX idx_jobs_workflow_id ON jobs(workflow_id);

CREATE UNIQUE INDEX idx_jobs_idempotency
ON jobs(workflow_id, idempotency_key, request_fingerprint)
WHERE idempotency_key IS NOT NULL AND request_fingerprint IS NOT NULL;

CREATE TABLE jobs_logs (
    id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL,
    level TEXT NOT NULL,
    event TEXT NOT NULL,
    payload_json TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
);

CREATE INDEX idx_jobs_logs_job_id_created_at ON jobs_logs(job_id, created_at);
