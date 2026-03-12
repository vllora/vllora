DROP INDEX IF EXISTS idx_workflow_topic_sources_topic_source;
DROP INDEX IF EXISTS idx_workflow_topic_sources_workflow_reference_id;
DROP INDEX IF EXISTS idx_workflow_topic_sources_reference_id;
DROP INDEX IF EXISTS idx_workflow_topic_sources_workflow;
DROP TABLE IF EXISTS workflow_topic_sources;

DROP INDEX IF EXISTS idx_workflow_topics_workflow_reference_id;
DROP INDEX IF EXISTS idx_workflow_topics_reference_id;
DROP INDEX IF EXISTS idx_workflow_topics_parent;
DROP INDEX IF EXISTS idx_workflow_topics_workflow;

CREATE TABLE workflow_topics_old (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    name TEXT NOT NULL,
    parent_id TEXT,
    selected INTEGER NOT NULL DEFAULT 1,
    source_chunk_refs TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    FOREIGN KEY (parent_id) REFERENCES workflow_topics_old(id)
);

INSERT INTO workflow_topics_old (id, workflow_id, name, parent_id, selected, source_chunk_refs, created_at)
SELECT
    id,
    workflow_id,
    name,
    NULL AS parent_id,
    1 AS selected,
    NULL AS source_chunk_refs,
    created_at
FROM workflow_topics;

DROP TABLE workflow_topics;
ALTER TABLE workflow_topics_old RENAME TO workflow_topics;

CREATE INDEX idx_workflow_topics_workflow ON workflow_topics(workflow_id);
CREATE INDEX idx_workflow_topics_parent ON workflow_topics(parent_id);
