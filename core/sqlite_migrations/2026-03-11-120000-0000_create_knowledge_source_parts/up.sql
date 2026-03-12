CREATE TABLE knowledge_source_parts (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL,
    part_type TEXT NOT NULL,
    content TEXT NOT NULL,
    content_metadata TEXT,
    title TEXT,
    extraction_path TEXT,
    extraction_metadata TEXT
);

CREATE INDEX idx_knowledge_source_parts_source_id ON knowledge_source_parts(source_id);
