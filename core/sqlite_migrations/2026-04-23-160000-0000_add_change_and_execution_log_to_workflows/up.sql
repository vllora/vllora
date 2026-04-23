-- Mirror Feature 002 state-layer markdown artefacts into the workflows row so
-- the UI can render them without reading the local `finetune-project/`
-- checkout. Both are raw Markdown (append-only on the client side); the
-- server just stores the latest snapshot.

ALTER TABLE workflows ADD COLUMN change_log_md TEXT;
ALTER TABLE workflows ADD COLUMN execution_log_md TEXT;
