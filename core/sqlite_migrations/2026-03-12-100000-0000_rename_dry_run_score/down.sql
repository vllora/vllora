-- Revert: add score columns back to workflow_records, drop join table

CREATE TABLE workflow_records_old (
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

INSERT INTO workflow_records_old (id, workflow_id, data, topic, span_id, is_generated, source_record_id, metadata, created_at)
SELECT id, workflow_id, data, topic, span_id, is_generated, source_record_id, metadata, created_at
FROM workflow_records;

-- Restore latest eval scores
UPDATE workflow_records_old
SET dry_run_score = (
    SELECT s.score FROM workflow_record_scores s
    WHERE s.record_id = workflow_records_old.id AND s.score_type = 'eval'
    ORDER BY s.created_at DESC LIMIT 1
);

-- Restore latest finetune scores
UPDATE workflow_records_old
SET finetune_score = (
    SELECT s.score FROM workflow_record_scores s
    WHERE s.record_id = workflow_records_old.id AND s.score_type = 'finetune'
    ORDER BY s.created_at DESC LIMIT 1
);

DROP TABLE workflow_records;
ALTER TABLE workflow_records_old RENAME TO workflow_records;

CREATE INDEX idx_workflow_records_workflow ON workflow_records(workflow_id);
CREATE INDEX idx_workflow_records_topic ON workflow_records(workflow_id, topic);
CREATE INDEX idx_workflow_records_span ON workflow_records(span_id);

DROP TABLE workflow_record_scores;
