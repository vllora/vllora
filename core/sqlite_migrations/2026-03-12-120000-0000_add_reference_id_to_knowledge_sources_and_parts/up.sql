ALTER TABLE knowledge_sources ADD COLUMN reference_id TEXT;
CREATE INDEX IF NOT EXISTS idx_knowledge_sources_reference_id ON knowledge_sources(reference_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_sources_workflow_reference_id
ON knowledge_sources(workflow_id, reference_id);

ALTER TABLE knowledge_source_parts ADD COLUMN reference_id TEXT;
CREATE INDEX IF NOT EXISTS idx_knowledge_source_parts_reference_id ON knowledge_source_parts(reference_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_source_parts_source_reference_id
ON knowledge_source_parts(source_id, reference_id);
