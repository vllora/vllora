-- Move scores from workflow_records columns into a separate join table.
-- This supports multiple eval/finetune jobs per record, each with its own score.

CREATE TABLE workflow_record_scores (
    id TEXT PRIMARY KEY NOT NULL,
    record_id TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    score_type TEXT NOT NULL,
    score REAL NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    UNIQUE(record_id, job_id, score_type)
);

CREATE INDEX idx_wrs_record ON workflow_record_scores(record_id);
CREATE INDEX idx_wrs_workflow ON workflow_record_scores(workflow_id);
CREATE INDEX idx_wrs_job ON workflow_record_scores(job_id);

-- Migrate existing scores (dry_run_score → eval, finetune_score → finetune)
INSERT INTO workflow_record_scores (id, record_id, workflow_id, job_id, score_type, score)
SELECT
    lower(hex(randomblob(16))),
    id,
    workflow_id,
    'legacy',
    'eval',
    dry_run_score
FROM workflow_records WHERE dry_run_score IS NOT NULL;

INSERT INTO workflow_record_scores (id, record_id, workflow_id, job_id, score_type, score)
SELECT
    lower(hex(randomblob(16))),
    id,
    workflow_id,
    'legacy',
    'finetune',
    finetune_score
FROM workflow_records WHERE finetune_score IS NOT NULL;

-- Drop the old score columns by recreating the table
CREATE TABLE workflow_records_new (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    data TEXT NOT NULL,
    topic TEXT,
    span_id TEXT,
    is_generated INTEGER NOT NULL DEFAULT 0,
    source_record_id TEXT,
    metadata TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL
);

INSERT INTO workflow_records_new (id, workflow_id, data, topic, span_id, is_generated, source_record_id, metadata, created_at)
SELECT id, workflow_id, data, topic, span_id, is_generated, source_record_id, metadata, created_at
FROM workflow_records;

DROP TABLE workflow_records;
ALTER TABLE workflow_records_new RENAME TO workflow_records;

CREATE INDEX idx_workflow_records_workflow ON workflow_records(workflow_id);
CREATE INDEX idx_workflow_records_topic ON workflow_records(workflow_id, topic);
CREATE INDEX idx_workflow_records_span ON workflow_records(span_id);
