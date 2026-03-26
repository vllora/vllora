CREATE TABLE knowledge_source_parts_old (
  id TEXT PRIMARY KEY NOT NULL,
  reference_id TEXT,
  source_id TEXT NOT NULL,
  part_type TEXT NOT NULL,
  content TEXT NOT NULL,
  content_metadata TEXT,
  title TEXT,
  extraction_path TEXT,
  extraction_metadata TEXT
);

INSERT INTO knowledge_source_parts_old (
  id,
  reference_id,
  source_id,
  part_type,
  content,
  content_metadata,
  title,
  extraction_path,
  extraction_metadata
)
SELECT
  id,
  reference_id,
  source_id,
  part_type,
  content,
  content_metadata,
  title,
  extraction_path,
  extraction_metadata
FROM knowledge_source_parts;

DROP TABLE knowledge_source_parts;

ALTER TABLE knowledge_source_parts_old RENAME TO knowledge_source_parts;
