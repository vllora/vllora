DROP INDEX IF EXISTS idx_knowledge_source_parts_source_reference_id;
DROP INDEX IF EXISTS idx_knowledge_source_parts_reference_id;
DROP INDEX IF EXISTS idx_knowledge_sources_workflow_reference_id;
DROP INDEX IF EXISTS idx_knowledge_sources_reference_id;

CREATE TABLE knowledge_sources_old (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    metadata TEXT,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    deleted_at TEXT
);

INSERT INTO knowledge_sources_old (id, workflow_id, name, description, metadata, created_at, deleted_at)
SELECT id, workflow_id, name, description, metadata, created_at, deleted_at
FROM knowledge_sources;

DROP TABLE knowledge_sources;
ALTER TABLE knowledge_sources_old RENAME TO knowledge_sources;
CREATE INDEX idx_knowledge_sources_workflow ON knowledge_sources(workflow_id);

CREATE TABLE knowledge_source_parts_old (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL,
    part_type TEXT NOT NULL,
    content TEXT NOT NULL,
    content_metadata TEXT,
    title TEXT,
    extraction_path TEXT,
    extraction_metadata TEXT
);

INSERT INTO knowledge_source_parts_old (
    id, source_id, part_type, content, content_metadata, title, extraction_path, extraction_metadata
)
SELECT
    id, source_id, part_type, content, content_metadata, title, extraction_path, extraction_metadata
FROM knowledge_source_parts;

DROP TABLE knowledge_source_parts;
ALTER TABLE knowledge_source_parts_old RENAME TO knowledge_source_parts;
CREATE INDEX idx_knowledge_source_parts_source_id ON knowledge_source_parts(source_id);
