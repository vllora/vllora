CREATE TABLE workflow_record_scores_old (
    id TEXT PRIMARY KEY NOT NULL,
    record_id TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    score_type TEXT NOT NULL,
    score REAL NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    UNIQUE(record_id, job_id, score_type)
);

INSERT INTO workflow_record_scores_old (
    id,
    record_id,
    workflow_id,
    job_id,
    score_type,
    score,
    created_at
)
SELECT
    id,
    record_id,
    workflow_id,
    job_id,
    score_type,
    score,
    created_at
FROM workflow_record_scores;

DROP TABLE workflow_record_scores;
ALTER TABLE workflow_record_scores_old RENAME TO workflow_record_scores;

CREATE INDEX idx_wrs_record ON workflow_record_scores(record_id);
CREATE INDEX idx_wrs_workflow ON workflow_record_scores(workflow_id);
CREATE INDEX idx_wrs_job ON workflow_record_scores(job_id);

CREATE TABLE workflow_records_old (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    data TEXT NOT NULL,
    topic_id TEXT,
    span_id TEXT,
    is_generated INTEGER NOT NULL DEFAULT 0,
    source_record_id TEXT,
    metadata TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    FOREIGN KEY (topic_id) REFERENCES workflow_topics(id) ON DELETE SET NULL
);

INSERT INTO workflow_records_old (
    id,
    workflow_id,
    data,
    topic_id,
    span_id,
    is_generated,
    source_record_id,
    metadata,
    created_at
)
SELECT
    id,
    workflow_id,
    data,
    topic_id,
    span_id,
    is_generated,
    source_record_id,
    metadata,
    created_at
FROM workflow_records;

DROP TABLE workflow_records;
ALTER TABLE workflow_records_old RENAME TO workflow_records;

CREATE INDEX idx_workflow_records_workflow ON workflow_records(workflow_id);
CREATE INDEX idx_workflow_records_topic_id ON workflow_records(workflow_id, topic_id);
CREATE INDEX idx_workflow_records_span ON workflow_records(span_id);
