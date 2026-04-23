-- Add per-record traceability columns for Feature 001 + Feature 002.
--
-- `origin_uri` — source URI the record was derived from (file://, hf://, s3://,
-- gs://, azblob://, https://). Populated by `vllora finetune import-records`
-- and by `record_generator` worker output. Workers never see URIs; adapters
-- resolve to local paths and the original URI is preserved here for audit.
--
-- `origin_source_id` — source-system identifier (e.g., HuggingFace dataset
-- name). Kept separate from `origin_uri` so we can query "all records from
-- source X" across URI revisions.
--
-- Both nullable so historical rows (pre-2026-04-23) remain valid. New writes
-- populate when the source is known.

ALTER TABLE workflow_records ADD COLUMN origin_uri TEXT;
ALTER TABLE workflow_records ADD COLUMN origin_source_id TEXT;

-- Spec: specs/001-job-based-cli-api/data-model.md § Appendix: Auxiliary
-- traceability fields on downstream tables.
