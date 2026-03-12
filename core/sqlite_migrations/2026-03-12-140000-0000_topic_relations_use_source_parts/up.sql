DROP INDEX IF EXISTS idx_workflow_topic_sources_topic_source;
DROP INDEX IF EXISTS idx_workflow_topic_sources_workflow_reference_id;
DROP INDEX IF EXISTS idx_workflow_topic_sources_reference_id;
DROP INDEX IF EXISTS idx_workflow_topic_sources_workflow;

CREATE TABLE workflow_topic_sources_new (
    id TEXT PRIMARY KEY NOT NULL,
    reference_id TEXT,
    workflow_id TEXT NOT NULL,
    topic_id TEXT NOT NULL,
    source_part_id TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    FOREIGN KEY (topic_id) REFERENCES workflow_topics(id),
    FOREIGN KEY (source_part_id) REFERENCES knowledge_source_parts(id)
);

INSERT INTO workflow_topic_sources_new (id, reference_id, workflow_id, topic_id, source_part_id, created_at)
SELECT
    id,
    reference_id,
    workflow_id,
    topic_id,
    source_id,
    created_at
FROM workflow_topic_sources;

DROP TABLE workflow_topic_sources;
ALTER TABLE workflow_topic_sources_new RENAME TO workflow_topic_sources;

CREATE INDEX idx_workflow_topic_sources_workflow ON workflow_topic_sources(workflow_id);
CREATE INDEX idx_workflow_topic_sources_reference_id ON workflow_topic_sources(reference_id);
CREATE UNIQUE INDEX idx_workflow_topic_sources_workflow_reference_id
ON workflow_topic_sources(workflow_id, reference_id);
CREATE UNIQUE INDEX idx_workflow_topic_sources_topic_part
ON workflow_topic_sources(workflow_id, topic_id, source_part_id);
