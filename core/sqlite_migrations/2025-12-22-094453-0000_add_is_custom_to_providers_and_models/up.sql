-- Add is_custom column to providers table
ALTER TABLE providers ADD COLUMN is_custom INTEGER NOT NULL DEFAULT 0;

-- Add is_custom column to models table
ALTER TABLE models ADD COLUMN is_custom INTEGER NOT NULL DEFAULT 0;

-- Update existing records: set is_custom = 0 for existing providers and models
-- (existing records are not custom, only new ones created via API will be custom)
UPDATE providers SET is_custom = 0 WHERE is_custom IS NULL;
UPDATE models SET is_custom = 0 WHERE is_custom IS NULL;
