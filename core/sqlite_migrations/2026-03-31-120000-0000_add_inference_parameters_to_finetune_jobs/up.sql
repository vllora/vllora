-- Add inference_parameters column to store per-job inference settings.
-- Mirrors training_config semantics but for inference/runtime parameters.

ALTER TABLE finetune_jobs
ADD COLUMN inference_parameters TEXT;

