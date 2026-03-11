CREATE TABLE workflow_records (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    data TEXT NOT NULL,
    topic TEXT,
    span_id TEXT,
    is_generated INTEGER NOT NULL DEFAULT 0,
    source_record_id TEXT,
    dry_run_score REAL,
    finetune_score REAL,
    metadata TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL
);

CREATE INDEX idx_workflow_records_workflow ON workflow_records(workflow_id);
CREATE INDEX idx_workflow_records_topic ON workflow_records(workflow_id, topic);
CREATE INDEX idx_workflow_records_span ON workflow_records(span_id);
