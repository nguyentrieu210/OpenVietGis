# R026 Integration Hub — developer quickstart

The first slice registers a **read-only** catalog inside the existing project
Plugin manager. It does not establish a connection to desktop software.

For a signed-in project member, the web UI calls:

GET /api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/catalog

Response fields:
- schema_version, organization_id, project_id, mode, default_write_policy
- adapters[]: key, name, category, mode, lifecycle, protocols, note,
  existing_api, actions[]
- actions[]: action, required_permission, eligible_by_role,
  executable_in_hub, allowed, reason

Operations besides catalog.discover are **not implemented** in R026. An
eligible project role alone is never authority to read attributes from an
external provider, upload, mutate or synchronize data.

Access is authenticated; backend performs RLS/project.read via
prepare_project_scope. The project GIS ACL guard remains enabled, including
restricted accounts. The R010 WMS/WFS/COG connection API and its allowlist,
SSRF and authorization rules are separate and unchanged.

Local tests:
PYTHONPATH=apps/api python -m pytest -q apps/api/tests/unit/integration_hub

Before deployment, conduct PostGIS integration tests for cross-tenant access,
service principals/credential ceiling and restricted GIS layer access. Promote
only after independent security review and rollback verification.
