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

INSERT INTO workflow_records_old (
    id,
    workflow_id,
    data,
    topic,
    span_id,
    is_generated,
    source_record_id,
    dry_run_score,
    finetune_score,
    metadata,
    created_at
)
SELECT
    wr.id,
    wr.workflow_id,
    wr.data,
    (
        SELECT wt.name
        FROM workflow_topics wt
        WHERE wt.id = wr.topic_id
        LIMIT 1
    ) AS topic,
    wr.span_id,
    wr.is_generated,
    wr.source_record_id,
    wr.dry_run_score,
    wr.finetune_score,
    wr.metadata,
    wr.created_at
FROM workflow_records wr;

DROP TABLE workflow_records;
ALTER TABLE workflow_records_old RENAME TO workflow_records;

CREATE INDEX idx_workflow_records_workflow ON workflow_records(workflow_id);
CREATE INDEX idx_workflow_records_topic ON workflow_records(workflow_id, topic);
CREATE INDEX idx_workflow_records_span ON workflow_records(span_id);
