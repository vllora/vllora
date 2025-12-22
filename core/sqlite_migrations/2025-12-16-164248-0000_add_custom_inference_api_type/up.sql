-- Add custom_inference_api_type column to providers table
ALTER TABLE providers ADD COLUMN custom_inference_api_type TEXT;

-- Backfill existing providers based on provider_name
UPDATE providers 
SET custom_inference_api_type = 'openai' 
WHERE provider_name IN ('openai', 'azure', 'vllora_open', 'openrouter', 'parasail', 'togetherai', 'xai', 'zai', 'mistralai', 'groq', 'deepinfra', 'deepseek', 'fireworksai', 'langdb', 'vllora')
   OR custom_inference_api_type IS NULL;

UPDATE providers 
SET custom_inference_api_type = 'anthropic' 
WHERE provider_name = 'anthropic';

UPDATE providers 
SET custom_inference_api_type = 'bedrock' 
WHERE provider_name = 'bedrock';

UPDATE providers 
SET custom_inference_api_type = 'gemini' 
WHERE provider_name IN ('gemini', 'vertex-ai');

-- Set default for any remaining NULL values
UPDATE providers 
SET custom_inference_api_type = 'openai' 
WHERE custom_inference_api_type IS NULL;
