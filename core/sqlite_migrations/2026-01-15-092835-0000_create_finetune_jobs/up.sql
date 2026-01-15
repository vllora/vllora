-- Create finetune_jobs table for tracking fine-tuning jobs
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
    hyperparameters TEXT, -- JSON stored as text
    training_file_id TEXT,
    validation_file_id TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    completed_at TEXT,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    CONSTRAINT valid_finetune_job_state CHECK (state IN ('pending', 'running', 'succeeded', 'failed', 'cancelled'))
);

-- Create indexes for common queries
CREATE INDEX idx_finetune_jobs_project_id ON finetune_jobs(project_id);
CREATE INDEX idx_finetune_jobs_state ON finetune_jobs(state);
CREATE INDEX idx_finetune_jobs_provider_job_id ON finetune_jobs(provider_job_id);
CREATE INDEX idx_finetune_jobs_provider ON finetune_jobs(provider);
CREATE INDEX idx_finetune_jobs_created_at ON finetune_jobs(created_at);
