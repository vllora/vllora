ALTER TABLE finetune_jobs RENAME TO finetune_jobs_old;

CREATE TABLE finetune_jobs (
    id TEXT PRIMARY KEY DEFAULT (
        lower(hex(randomblob(4))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(2))) || '-' ||
        lower(hex(randomblob(6)))
    ) NOT NULL,
    project_id TEXT NOT NULL,
    dataset_id TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending',
    provider TEXT NOT NULL,
    provider_job_id TEXT NOT NULL,
    base_model TEXT NOT NULL,
    fine_tuned_model TEXT,
    error_message TEXT,
    training_config TEXT,
    training_file_id TEXT,
    validation_file_id TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    completed_at TEXT,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    CONSTRAINT valid_finetune_job_state CHECK (state IN ('pending', 'running', 'succeeded', 'failed', 'cancelled'))
);

INSERT INTO finetune_jobs (
    id,
    project_id,
    dataset_id,
    state,
    provider,
    provider_job_id,
    base_model,
    fine_tuned_model,
    error_message,
    training_config,
    training_file_id,
    validation_file_id,
    created_at,
    updated_at,
    completed_at
)
SELECT
    id,
    project_id,
    dataset_id,
    state,
    provider,
    provider_job_id,
    base_model,
    fine_tuned_model,
    error_message,
    training_config,
    training_file_id,
    validation_file_id,
    created_at,
    updated_at,
    completed_at
FROM finetune_jobs_old;

DROP TABLE finetune_jobs_old;

CREATE INDEX idx_finetune_jobs_project_id ON finetune_jobs(project_id);
CREATE INDEX idx_finetune_jobs_state ON finetune_jobs(state);
CREATE INDEX idx_finetune_jobs_provider_job_id ON finetune_jobs(provider_job_id);
CREATE INDEX idx_finetune_jobs_provider ON finetune_jobs(provider);
CREATE INDEX idx_finetune_jobs_created_at ON finetune_jobs(created_at);
