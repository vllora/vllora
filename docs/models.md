### Models API

- GET /models: list available models for the project

Notes:
- Returns supported model identifiers and metadata.
- Removed min service level

### Schema

Source type: `cloud/src/data/models/global_model_info.rs::GlobalModelInfo`

Common fields:
- id (string, UUID)
- model_name (string)
- description (string)
- provider_info_id (string, UUID)
- model_type (string)
- input_token_price (number, nullable)
- output_token_price (number, nullable)
- context_size (number, nullable)
- capabilities (array<string>, nullable)
- input_types (array<string>, nullable)
- output_types (array<string>, nullable)
- tags (array<string>, nullable)
- type_prices (string, nullable)
- mp_price (number, nullable)
- model_name_in_provider (string, nullable)
- owner_name (string)
- priority (number)
- parameters (object, nullable)
- created_at, updated_at (string)
- deleted_at (string, nullable)
- benchmark_info (object, nullable)
- cached_input_token_price (number, nullable)
- cached_input_write_token_price (number, nullable)
- release_date, langdb_release_date, knowledge_cutoff_date (date string, nullable)
- license (string, nullable)
- project_id (strubg, uuid, nullable)

Example (truncated):
```json
{
  "id": "3b9d6e37-1a2b-4c5d-9e8f-0123456789ab",
  "model_name": "gpt-4o",
  "description": "General-purpose LLM",
  "provider_info_id": "11111111-2222-3333-4444-555555555555",
  "model_type": "chat",
  "input_token_price": 0.5,
  "output_token_price": 1.5,
  "context_size": 128000,
  "capabilities": ["vision", "tools"],
  "tags": ["stable"],
  "owner_name": "openai",
  "priority": 10,
  "min_service_level": 0
}
```
