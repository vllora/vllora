### Projects API

- GET /projects: list all user projects
- POST /projects: create a project
- GET /projects/{id}: get a project by id
- DELETE /projects/{id}: delete a project by id
- PUT /projects: update project settings and default options

Notes:
- Auth required. Project scoping via tenant/project headers when applicable.
- Request/response schemas TBD.

### Schema

Source type: `ai-gateway/core/src/types/metadata/project.rs::Project`

Fields:
- id (string, UUID)
- name (string)
- description (string, nullable)
- created_at (string, RFC3339 or DB timestamp)
- updated_at (string, RFC3339 or DB timestamp)
- company_id (string, UUID)
- slug (string)
- settings (object, nullable)
- is_default (boolean)
- archived_at (string, nullable)
- allowed_user_ids (array<string>, nullable)
- private_model_prices (object, nullable)

Example:
```json
{
  "id": "a7c3a9e2-5c4e-4a8c-9f2c-9d1b5f4c1234",
  "name": "Demo Project",
  "description": null,
  "created_at": "2025-01-01T12:00:00Z",
  "updated_at": "2025-01-02T12:00:00Z",
  "company_id": "b1f6a9d0-1234-5678-9abc-def012345678",
  "slug": "demo-project",
  "settings": {"feature_flags": {"chat_tracing": true}},
  "is_default": false,
  "archived_at": null,
  "allowed_user_ids": ["user-1", "user-2"],
  "private_model_prices": null
}
```
