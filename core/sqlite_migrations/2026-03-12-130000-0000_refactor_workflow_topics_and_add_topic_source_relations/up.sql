CREATE TABLE workflow_topics_new (
    id TEXT PRIMARY KEY NOT NULL,
    reference_id TEXT,
    workflow_id TEXT NOT NULL,
    name TEXT NOT NULL,
    parent_id TEXT,
    system_prompt TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    FOREIGN KEY (parent_id) REFERENCES workflow_topics_new(id)
);

INSERT INTO workflow_topics_new (id, reference_id, workflow_id, name, parent_id, system_prompt, created_at)
SELECT
    id,
    NULL AS reference_id,
    workflow_id,
    name,
    parent_id,
    NULL AS system_prompt,
    created_at
FROM workflow_topics;

DROP TABLE workflow_topics;
ALTER TABLE workflow_topics_new RENAME TO workflow_topics;

CREATE INDEX idx_workflow_topics_workflow ON workflow_topics(workflow_id);
CREATE INDEX idx_workflow_topics_reference_id ON workflow_topics(reference_id);
CREATE INDEX idx_workflow_topics_parent ON workflow_topics(parent_id);
CREATE UNIQUE INDEX idx_workflow_topics_workflow_reference_id ON workflow_topics(workflow_id, reference_id);

CREATE TABLE workflow_topic_sources (
    id TEXT PRIMARY KEY NOT NULL,
    reference_id TEXT,
    workflow_id TEXT NOT NULL,
    topic_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    FOREIGN KEY (topic_id) REFERENCES workflow_topics(id),
    FOREIGN KEY (source_id) REFERENCES knowledge_sources(id)
);

CREATE INDEX idx_workflow_topic_sources_workflow ON workflow_topic_sources(workflow_id);
CREATE INDEX idx_workflow_topic_sources_reference_id ON workflow_topic_sources(reference_id);
CREATE UNIQUE INDEX idx_workflow_topic_sources_workflow_reference_id
ON workflow_topic_sources(workflow_id, reference_id);
CREATE UNIQUE INDEX idx_workflow_topic_sources_topic_source
ON workflow_topic_sources(workflow_id, topic_id, source_id);
