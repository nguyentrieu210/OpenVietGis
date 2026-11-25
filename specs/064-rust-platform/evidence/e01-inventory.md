# E01 Inventory & Contract Evidence — Nền tảng, identity, tenant, security

- **Task**: `E01-T01` (Acceptance Criterion: `E01-AC1`)
- **Parent Spec**: Spec 064 (`064-rust-platform`), references: R001, R002, R015
- **Child Spec**: `specs/064-rust-platform/engines/e01-platform-security.md`
- **Contract Artifact**: `specs/064-rust-platform/contracts/engines/e01.json`
- **Baseline Commit**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Generated (UTC)**: `2026-11-25T17:00:35Z`
- **Architecture Candidate**: Rust Axum/SQLx; retain external OIDC provider; preserve token/cookie/CSRF/RLS/audit contract.

## 1. Summary Counts

| Metric | Count |
|---|---:|
| Live HTTP Operations | 54 |
| Live WebSocket Routes | 0 |
| Live SSE / Streaming Routes | 0 / 0 |
| Classification: `migrate` | 54 |
| Classification: `replace` | 0 |
| Classification: `retain` | 0 |
| Classification: `approved_retire` | 0 |
| Owned Database Migrations | 8 |
| Mapped & Triaged Source Files | 66 |
| Mapped Pytest Files | 77 |
| Mapped Pytest Test Case IDs | 209 |

## 2. Live Runtime Operation Matrix

| Method | Path | Operation ID | Handler (`module`) | Classification | Auth / Guard | Revision / Idempotency | Tests |
|---|---|---|---|---|---|---|---:|
| `GET` | `/api/v1/auth/callback/{provider}` | `complete_login_api_v1_auth_callback__provider__get` | `complete_login` (`auth`) | `migrate` | public | — | 20 files (59 IDs) |
| `GET` | `/api/v1/auth/csrf` | `get_csrf_api_v1_auth_csrf_get` | `get_csrf` (`auth`) | `migrate` | session_cookie_or_service_token | — | 21 files (60 IDs) |
| `GET` | `/api/v1/auth/login/{provider}` | `begin_login_api_v1_auth_login__provider__get` | `begin_login` (`auth`) | `migrate` | session_cookie_or_service_token | — | 20 files (59 IDs) |
| `POST` | `/api/v1/auth/logout` | `logout_api_v1_auth_logout_post` | `logout` (`auth`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 20 files (59 IDs) |
| `GET` | `/api/v1/auth/providers` | `list_identity_providers_api_v1_auth_providers_get` | `list_identity_providers` (`auth`) | `migrate` | public | — | 20 files (59 IDs) |
| `POST` | `/api/v1/bootstrap/initialize` | `initialize_bootstrap_api_v1_bootstrap_initialize_post` | `initialize_bootstrap` (`bootstrap`) | `migrate` | bootstrap_secret_header + CSRF | — | 3 files (3 IDs) |
| `GET` | `/api/v1/bootstrap/status` | `get_bootstrap_status_api_v1_bootstrap_status_get` | `get_bootstrap_status` (`bootstrap`) | `migrate` | bootstrap_secret_header | — | 3 files (3 IDs) |
| `POST` | `/api/v1/invitations/accept` | `accept_invitation_api_v1_invitations_accept_post` | `accept_invitation` (`invitations`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 6 files (23 IDs) |
| `POST` | `/api/v1/invitations/check` | `check_invitation_api_v1_invitations_check_post` | `check_invitation` (`invitations`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 6 files (23 IDs) |
| `GET` | `/api/v1/me` | `get_current_user_api_v1_me_get` | `get_current_user` (`me`) | `migrate` | session_cookie_or_service_token | — | 7 files (22 IDs) |
| `DELETE` | `/api/v1/me/avatar` | `remove_avatar_api_v1_me_avatar_delete` | `remove_avatar` (`me`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 7 files (22 IDs) |
| `GET` | `/api/v1/me/avatar` | `get_avatar_api_v1_me_avatar_get` | `get_avatar` (`me`) | `migrate` | session_cookie_or_service_token | — | 7 files (22 IDs) |
| `PUT` | `/api/v1/me/avatar` | `put_avatar_api_v1_me_avatar_put` | `put_avatar` (`me`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 7 files (22 IDs) |
| `DELETE` | `/api/v1/me/identity-links` | `unlink_identity_api_v1_me_identity_links_delete` | `unlink_identity` (`me`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 7 files (22 IDs) |
| `GET` | `/api/v1/me/identity-links` | `list_identity_links_api_v1_me_identity_links_get` | `list_identity_links` (`me`) | `migrate` | session_cookie_or_service_token | — | 7 files (22 IDs) |
| `GET` | `/api/v1/me/organizations` | `list_my_organizations_api_v1_me_organizations_get` | `list_my_organizations` (`me`) | `migrate` | session_cookie_or_service_token | — | 7 files (22 IDs) |
| `POST` | `/api/v1/organizations` | `create_organization_api_v1_organizations_post` | `create_organization` (`organizations`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 11 files (24 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}` | `get_organization_api_v1_organizations__organization_id__get` | `get_organization` (`organizations`) | `migrate` | session_cookie_or_service_token | — | 11 files (24 IDs) |
| `PATCH` | `/api/v1/organizations/{organization_id}` | `update_organization_api_v1_organizations__organization_id__patch` | `update_organization` (`organizations`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 11 files (24 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/audit-events` | `list_audit_events_api_v1_organizations__organization_id__audit_events_get` | `list_audit_events` (`audit`) | `migrate` | session_cookie_or_service_token | — | 5 files (10 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/effective-permissions` | `get_effective_permissions_api_v1_organizations__organization_id__effective_permissions_get` | `get_effective_permissions` (`organizations`) | `migrate` | session_cookie_or_service_token | — | 11 files (24 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/grants` | `list_resource_grants_api_v1_organizations__organization_id__grants_get` | `list_resource_grants` (`grants`) | `migrate` | session_cookie_or_service_token | — | 11 files (38 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/grants` | `create_resource_grant_api_v1_organizations__organization_id__grants_post` | `create_resource_grant` (`grants`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 11 files (38 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/grants/{grant_id}/revoke` | `revoke_resource_grant_api_v1_organizations__organization_id__grants__grant_id__revoke_post` | `revoke_resource_grant` (`grants`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 11 files (38 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/invitations` | `list_invitations_api_v1_organizations__organization_id__invitations_get` | `list_invitations` (`invitations`) | `migrate` | session_cookie_or_service_token | — | 6 files (23 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/invitations` | `create_invitation_api_v1_organizations__organization_id__invitations_post` | `create_invitation` (`invitations`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 6 files (23 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/invitations/email` | `email_invitation_api_v1_organizations__organization_id__invitations_email_post` | `email_invitation` (`invitations`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 6 files (23 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/invitations/{invitation_id}/revoke` | `revoke_invitation_api_v1_organizations__organization_id__invitations__invitation_id__revoke_post` | `revoke_invitation` (`invitations`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 6 files (23 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/members` | `list_organization_members_api_v1_organizations__organization_id__members_get` | `list_organization_members` (`organizations`) | `migrate` | session_cookie_or_service_token | — | 12 files (36 IDs) |
| `PATCH` | `/api/v1/organizations/{organization_id}/members/{membership_id}` | `update_organization_membership_api_v1_organizations__organization_id__members__membership_id__patch` | `update_organization_membership` (`organizations`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 12 files (36 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/ownership-transfer` | `transfer_organization_ownership_api_v1_organizations__organization_id__ownership_transfer_post` | `transfer_organization_ownership` (`organizations`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 11 files (24 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects` | `list_projects_api_v1_organizations__organization_id__projects_get` | `list_projects` (`projects`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, None) | — | 12 files (35 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects` | `create_project_api_v1_organizations__organization_id__projects_post` | `create_project` (`projects`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, None) | — | 12 files (35 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}` | `get_project_api_v1_organizations__organization_id__projects__project_id__get` | `get_project` (`projects`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:allow, FOR SHARE) | — | 12 files (35 IDs) |
| `PATCH` | `/api/v1/organizations/{organization_id}/projects/{project_id}` | `update_project_api_v1_organizations__organization_id__projects__project_id__patch` | `update_project` (`projects`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 12 files (35 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/access` | `get_access_api_v1_organizations__organization_id__projects__project_id__access_get` | `get_access` (`gis_access`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 4 files (7 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/access/export/{layer_id}` | `export_layer_api_v1_organizations__organization_id__projects__project_id__access_export__layer_id__get` | `export_layer` (`gis_access`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:allow, FOR SHARE) | — | 4 files (7 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/access/groups` | `create_group_api_v1_organizations__organization_id__projects__project_id__access_groups_post` | `create_group` (`gis_access`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 4 files (7 IDs) |
| `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/access/groups/{group_id}` | `replace_group_api_v1_organizations__organization_id__projects__project_id__access_groups__group_id__put` | `replace_group` (`gis_access`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 4 files (7 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/access/mine` | `mine_api_v1_organizations__organization_id__projects__project_id__access_mine_get` | `mine` (`gis_access`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:allow, FOR SHARE) | — | 4 files (7 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/access/reviews` | `list_reviews_api_v1_organizations__organization_id__projects__project_id__access_reviews_get` | `list_reviews` (`gis_access`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:allow, FOR SHARE) | — | 4 files (7 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/access/reviews/{changeset_id}` | `decide_api_v1_organizations__organization_id__projects__project_id__access_reviews__changeset_id__post` | `decide` (`gis_access`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:allow, FOR UPDATE) | — | 4 files (7 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/access/rules` | `create_rule_api_v1_organizations__organization_id__projects__project_id__access_rules_post` | `create_rule` (`gis_access`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 4 files (7 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/access/rules/batch` | `batch_rules_api_v1_organizations__organization_id__projects__project_id__access_rules_batch_post` | `batch_rules` (`gis_access`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 4 files (7 IDs) |
| `DELETE` | `/api/v1/organizations/{organization_id}/projects/{project_id}/access/rules/{rule_id}` | `revoke_rule_api_v1_organizations__organization_id__projects__project_id__access_rules__rule_id__delete` | `revoke_rule` (`gis_access`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 4 files (7 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/service-principals` | `list_service_principals_api_v1_organizations__organization_id__service_principals_get` | `list_service_principals` (`service_principals`) | `migrate` | session_cookie_or_service_token | — | 5 files (14 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/service-principals` | `create_service_principal_api_v1_organizations__organization_id__service_principals_post` | `create_service_principal` (`service_principals`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 5 files (14 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/service-principals/{service_principal_id}/credentials` | `list_service_credentials_api_v1_organizations__organization_id__service_principals__service_principal_id__credentials_get` | `list_service_credentials` (`service_principals`) | `migrate` | session_cookie_or_service_token | — | 5 files (14 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/service-principals/{service_principal_id}/credentials` | `issue_service_credential_api_v1_organizations__organization_id__service_principals__service_principal_id__credentials_post` | `issue_service_credential` (`service_principals`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 5 files (14 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/service-principals/{service_principal_id}/credentials/{credential_id}/revoke` | `revoke_service_credential_api_v1_organizations__organization_id__service_principals__service_principal_id__credentials__credential_id__revoke_post` | `revoke_service_credential` (`service_principals`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 5 files (14 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/service-principals/{service_principal_id}/revoke` | `revoke_service_principal_api_v1_organizations__organization_id__service_principals__service_principal_id__revoke_post` | `revoke_service_principal` (`service_principals`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 5 files (14 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/workspaces` | `list_workspaces_api_v1_organizations__organization_id__workspaces_get` | `list_workspaces` (`workspaces`) | `migrate` | session_cookie_or_service_token | — | 4 files (13 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/workspaces` | `create_workspace_api_v1_organizations__organization_id__workspaces_post` | `create_workspace` (`workspaces`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 4 files (13 IDs) |
| `PATCH` | `/api/v1/organizations/{organization_id}/workspaces/{workspace_id}` | `update_workspace_api_v1_organizations__organization_id__workspaces__workspace_id__patch` | `update_workspace` (`workspaces`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF | — | 4 files (13 IDs) |

## 3. Algorithms, Registries & Domain Capabilities

```json
{
  "auth_mechanisms": [
    "OIDC authorization code flow (SessionMiddleware ovg_oidc_state)",
    "Signed human session cookie (ovg_session, configurable TTL 300..604800s)",
    "Double-submit / signed CSRF header (x-openvietgis-csrf) on mutating human routes",
    "Machine/service principal bearer credential tokens",
    "One-time bootstrap secret header (OPENVIETGIS_BOOTSTRAP_SECRET)"
  ],
  "project_access_guard": {
    "safe_get_suffixes": [
      "",
      "/access/export/{layer_id}",
      "/access/mine",
      "/access/reviews",
      "/extensions/activations",
      "/integration-hub/adapters/{adapter_key}",
      "/integration-hub/adapters/{adapter_key}/layers/{layer_id}/export",
      "/integration-hub/catalog",
      "/integration-hub/events",
      "/integration-hub/managed/bootstrap",
      "/integration-hub/managed/commits/{operation_id}",
      "/integration-hub/managed/delta",
      "/integration-hub/managed/desktop-sessions/{session_id}/commands",
      "/integration-hub/managed/events",
      "/integration-hub/managed/sessions",
      "/integration-hub/managed/sessions/{session_id}",
      "/layers",
      "/layers/{layer_id}",
      "/layers/{layer_id}/display-extent",
      "/layers/{layer_id}/features",
      "/layers/{layer_id}/features/{feature_id}",
      "/layers/{layer_id}/viewport",
      "/map",
      "/pro/tiles/{layer_id}/{z}/{x}/{y}.mvt"
    ],
    "safe_post_suffixes": [
      "/access/reviews/{changeset_id}",
      "/editing/snap",
      "/integration-hub/adapters/{adapter_key}/layers/{layer_id}/sync-preview",
      "/integration-hub/adapters/{adapter_key}/layers/{layer_id}/sync-push",
      "/integration-hub/managed/commits",
      "/integration-hub/managed/desktop-sessions/{session_id}/commands",
      "/integration-hub/managed/desktop-sessions/{session_id}/commands/{command_id}/respond",
      "/integration-hub/managed/sessions",
      "/layers/{layer_id}/geometry-preview",
      "/pro/geometry",
      "/sources/{source_id}/changesets"
    ],
    "safe_delete_suffixes": [
      "/integration-hub/managed/sessions/{session_id}"
    ],
    "write_lock_get_suffixes": [
      "/field/packages",
      "/field/packages/{package_id}/download",
      "/field/packages/{package_id}/snapshot"
    ],
    "lock_order": "org.organizations -> workspace.projects (FOR SHARE on read, FOR UPDATE on write)"
  },
  "rls_enforcement": "PostgreSQL RLS policies enforced under restricted openvietgis_runtime role in production."
}
```

## 4. Owned Database Migrations

| Revision | Down Revision | File | RLS | Classification | Summary |
|---|---|---|---|---|---|
| `0001_foundation_sample` | `None` | `apps/api/migrations/versions/0001_foundation_sample.py` | no | `retain` | Create disposable foundation sample storage. |
| `0002_identity_tenant_core` | `0001_foundation_sample` | `apps/api/migrations/versions/0002_identity_tenant_core.py` | no | `retain` | Create identity, tenancy, audit, and installation-state foundation. |
| `0003_tenant_rls` | `0002_identity_tenant_core` | `apps/api/migrations/versions/0003_tenant_rls.py` | yes | `retain` | Add runtime role and tenant row-level security. |
| `0003a_identity_unlink_security` | `0003_tenant_rls` | `apps/api/migrations/versions/0003a_identity_unlink_security.py` | no | `retain` | Guard identity unlink atomically with a narrowly scoped runtime function. |
| `0004_machine_identity` | `0003a_identity_unlink_security` | `apps/api/migrations/versions/0004_machine_identity.py` | yes | `retain` | Add tenant-scoped service principals and credentials. |
| `0005_retire_foundation_sample` | `0004_machine_identity` | `apps/api/migrations/versions/0005_retire_foundation_sample.py` | no | `retain` | Retire the disposable sample after the real tenant slice is accepted. |
| `0023_gis_access` | `0023_user_avatar` | `apps/api/migrations/versions/0023_gis_access.py` | yes | `retain` | Project GIS access scopes with deny-by-default membership policies. |
| `0023_user_avatar` | `0022_schema_change_evidence` | `apps/api/migrations/versions/0023_user_avatar.py` | yes | `retain` | Persist account avatars with self-only database access. |

## 5. Mapped & Triaged Source Files

Total source files mapped to `E01`: **66**

- `apps/api/openvietgis_api/__init__.py` (`replace` — triaged from CROSS_CUTTING_REVIEW: Python API package root; replaced by Rust workspace crate layout after cutover.)
- `apps/api/openvietgis_api/adapters/__init__.py` (`replace` — triaged from CROSS_CUTTING_REVIEW: Python adapters package marker; replaced by Rust adapter modules.)
- `apps/api/openvietgis_api/adapters/http/__init__.py` (`replace` — triaged from CROSS_CUTTING_REVIEW: Python HTTP adapter package marker; replaced by Rust Axum router registry.)
- `apps/api/openvietgis_api/adapters/http/audit.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/auth.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/authorization.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/bootstrap.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/errors.py` (`migrate` — triaged from CROSS_CUTTING_REVIEW: Canonical HTTP exception-to-status/error-body mapping (400/403/404/409 OV409/422 sanitized validation); must be preserved verbatim across all engines.)
- `apps/api/openvietgis_api/adapters/http/gis_access.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/gis_access_guard.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/grants.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/invitation_mail.py` (`migrate` — triaged from CROSS_CUTTING_REVIEW: Invitation email delivery adapter (Resend HTTPS transport) owned by E01 identity/invitations.)
- `apps/api/openvietgis_api/adapters/http/invitations.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/me.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/organizations.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/projects.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/security.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/service_principals.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/tenant_access.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/tenant_serialization.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/workspaces.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/oidc/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/oidc/provider.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/__init__.py` (`replace` — triaged from CROSS_CUTTING_REVIEW: Python persistence package marker; replaced by Rust SQLx repository crates.)
- `apps/api/openvietgis_api/adapters/persistence/audit_repository.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/authorization_repository.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/db.py` (`migrate` — triaged from CROSS_CUTTING_REVIEW: Database engine/session pool and restricted runtime role connection setup; migrated to Rust SQLx pool with identical RLS transaction semantics.)
- `apps/api/openvietgis_api/adapters/persistence/gis_access.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/identity_repository.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/platform_repository.py` (`migrate` — triaged from CROSS_CUTTING_REVIEW: Shared PostgreSQL persistence helpers for platform entities/revisions/audit.)
- `apps/api/openvietgis_api/adapters/persistence/rls.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/scope_helper.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/tenancy_repository.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/application/__init__.py` (`replace` — triaged from CROSS_CUTTING_REVIEW: Legacy Python application layer package marker.)
- `apps/api/openvietgis_api/config.py` (`migrate` — triaged from CROSS_CUTTING_REVIEW: OPENVIETGIS_* environment configuration and fail-closed production profile validation (HTTPS origins, >=32 char distinct secrets, openvietgis_runtime DB role).)
- `apps/api/openvietgis_api/domain/__init__.py` (`replace` — triaged from CROSS_CUTTING_REVIEW: Legacy Python domain package marker.)
- `apps/api/openvietgis_api/domains/audit/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/audit/domain.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/audit/ports.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/audit/service.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/authorization/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/authorization/context.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/authorization/domain.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/authorization/permissions.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/authorization/ports.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/authorization/roles.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/authorization/service.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/bootstrap/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/bootstrap/domain.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/bootstrap/ports.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/bootstrap/service.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/identity/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/identity/domain.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/identity/ports.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/identity/service.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/platform_errors.py` (`migrate` — triaged from CROSS_CUTTING_REVIEW: Shared PlatformDomainError hierarchy (status_code, code, message, details) used across E01-E15.)
- `apps/api/openvietgis_api/domains/tenancy/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/tenancy/domain.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/tenancy/ports.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/tenancy/service.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/main.py` (`migrate` — triaged from CROSS_CUTTING_REVIEW: FastAPI composition root: SessionMiddleware, CORSMiddleware, /api/v1 router mounts, project_access_guard, official module discovery, and error handler installation.)
- `apps/api/openvietgis_api/ports/__init__.py` (`replace` — triaged from CROSS_CUTTING_REVIEW: Legacy Python ports package marker.)
- `apps/api/openvietgis_api/security/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/security/csrf.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/security/secrets.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/security/sessions.py` (`migrate` — static-inventory direct mapping)

## 6. Cross-Engine Dependencies

- **E02..E15**: Provides auth, session/CSRF, project_access_guard, RLS context, and audit logging for all domain engines.
- **E18**: Production profile config validation and health/ops security controls.

## 7. Existing Pytest Evidence & Test Identifiers

Mapped **77 pytest files** containing **209 test case IDs** (primary + cross-engine coverage):

- `apps/api/tests/contract/test_foundation_contract.py` (primary, 1 tests): `test_foundation_openapi_contract_contains_required_paths`
- `apps/api/tests/contract/test_identity_tenancy_contracts.py` (primary, 3 tests): `test_spec_002_openapi_and_generated_contracts_are_valid`, `test_permission_registry_contract_exists_and_is_product_permission_based`, `test_historical_migrations_do_not_import_mutable_runtime_contracts`
- `apps/api/tests/contract/test_permission_registry.py` (primary, 3 tests): `test_permission_registry_has_no_cross_org_transfer_permission`, `test_permission_registry_has_no_configurable_deny_or_custom_role_surface`, `test_resource_permissions_are_additive_and_credential_scope_only_restricts`
- `apps/api/tests/integration/identity/test_account_avatar.py` (primary, 1 tests): `test_account_avatar_private_and_validated`
- `apps/api/tests/integration/identity/test_bootstrap.py` (primary, 1 tests): `test_bootstrap_requires_session_and_authority_and_commits_once`
- `apps/api/tests/integration/identity/test_bootstrap_concurrency.py` (primary, 1 tests): `test_bootstrap_two_authenticated_requests_have_one_winner`
- `apps/api/tests/integration/identity/test_bootstrap_storage.py` (primary, 1 tests): `test_installation_state_lock_and_immutable_transition`
- `apps/api/tests/integration/identity/test_http_security.py` (primary, 2 tests): `test_cookie_session_authentication_and_csrf`, `test_unlink_identity_invalid_payload_is_safe_422`
- `apps/api/tests/integration/identity/test_link_conflict_callback.py` (primary, 2 tests): `test_oidc_callback_preserves_denied_audit_and_does_not_change_links`, `test_concurrent_distinct_users_linking_one_identity_audits_loser`
- `apps/api/tests/integration/identity/test_live_keycloak_oidc.py` (primary, 2 tests): `test_live_oidc_secret_monitor_is_private_and_nonvacuous`, `test_live_keycloak_code_pkce_login_logout_and_stable_identity`
- `apps/api/tests/integration/identity/test_logout_switch.py` (primary, 1 tests): `test_logout_clears_only_same_origin_realm_and_revokes_app_session`
- `apps/api/tests/integration/identity/test_multi_org_session.py` (primary, 1 tests): `test_normal_organization_create_requires_initialized_installation_and_creates_owner`
- `apps/api/tests/integration/identity/test_oidc_callback_concurrency.py` (primary, 1 tests): `test_concurrent_first_login_same_identity_one_user_two_sessions`
- `apps/api/tests/integration/identity/test_oidc_login.py` (primary, 3 tests): `test_oidc_authorization_redirect_contains_state_nonce_and_pkce`, `test_redirect_fixture_supports_isolated_loopback_profiles`, `test_identity_link_conflict_and_link_unlink_are_audited`
- `apps/api/tests/integration/identity/test_p2_secret_surfaces.py` (primary, 1 tests): `test_p2_actual_bootstrap_and_service_secrets_stay_out_of_log_and_audit`
- `apps/api/tests/integration/identity/test_unlink_concurrency.py` (primary, 1 tests): `test_runtime_unlink_is_actor_bound_and_atomic`
- `apps/api/tests/integration/m3/test_extension_lifecycle_security.py` (primary, 1 tests): `test_extension_lifecycle_requires_admin_and_project_manage`
- `apps/api/tests/integration/rls/test_cross_tenant_api.py` (primary, 2 tests): `test_cross_tenant_routes_and_identifier_substitution`, `test_invitation_accept_and_immediate_revocation_via_runtime_api`
- `apps/api/tests/integration/rls/test_cross_tenant_constraints.py` (primary, 1 tests): `test_runtime_cannot_insert_or_reference_foreign_tenant`
- `apps/api/tests/integration/rls/test_cross_tenant_repository.py` (primary, 2 tests): `test_all_p1_protected_tables_and_pool_reuse`, `test_runtime_cannot_change_audit_even_with_tenant_context`
- `apps/api/tests/integration/rls/test_pool_tenant_leak.py` (primary, 1 tests): `test_repeated_pool_checkout_isolates_a_b_and_no_context`
- `apps/api/tests/integration/rls/test_runtime_role.py` (primary, 3 tests): `test_runtime_role_is_not_superuser_or_bypassrls`, `test_runtime_role_rls_hides_other_organization_and_audit_rows`, `test_application_rejects_privileged_runtime_connection`
- `apps/api/tests/integration/rls/test_transaction_context.py` (primary, 1 tests): `test_transaction_local_context_does_not_leak_across_reused_connection`
- `apps/api/tests/integration/service_identity/test_audit_actor_snapshot.py` (primary, 2 tests): `test_actor_snapshots_survive_human_and_service_revocation`, `test_runtime_service_mutations_are_audited_and_survive_revocation`
- `apps/api/tests/integration/service_identity/test_audit_storage.py` (primary, 1 tests): `test_audit_read_is_authorized_tenant_scoped_and_deterministically_ordered`
- `apps/api/tests/integration/service_identity/test_service_authorization.py` (primary, 4 tests): `test_service_credential_allowed_scope_ceiling_cross_tenant_and_revocation`, `test_revoke_between_credential_lookup_and_use_denies_stale_authentication`, `test_service_credential_use_holds_org_lock_before_project_mutation`, `test_service_principal_management_is_human_admin_only`
- `apps/api/tests/integration/service_identity/test_service_identity_migration.py` (primary, 3 tests): `test_machine_identity_tables_are_force_rls_and_runtime_tenant_scoped`, `test_service_grant_and_credential_cannot_cross_organization`, `test_service_credential_constraints_reject_bad_hash_ceiling_and_expiry`
- `apps/api/tests/integration/spatial/test_authorization.py` (primary, 5 tests): `test_human_current_membership_and_grant`, `test_service_current_grant_and_credential_ceiling`, `test_human_csrf_all_mutations_and_bearer_edit_control`, `test_scope_substitution_and_dual_authorized_cross_project_creation`, `test_change_history_rechecks_current_read_and_scope`
- `apps/api/tests/integration/spatial/test_gis_access.py` (primary, 1 tests): `test_scoped_layer_features_and_editing`
- `apps/api/tests/integration/spatial/test_gis_access_batch.py` (primary, 1 tests): `test_batch_rules_and_shared_activation`
- `apps/api/tests/integration/spatial/test_lifecycle_security.py` (primary, 7 tests): `test_archived_project_preserves_reads_but_checks_permission_before_state`, `test_project_deleted_or_organization_suspended_hides_even_pinned_reads`, `test_old_pins_do_not_cache_grant_or_membership_authority`, `test_workspace_reassignment_rechecks_inherited_grants_with_same_old_pins`, `test_retired_resources_hide_old_pins_and_ordinary_metadata_but_keep_history` (+2 more)
- `apps/api/tests/integration/spatial/test_migration_rls.py` (primary, 12 tests): `test_spatial_schema_runtime_role_is_non_owner_and_rls_forced`, `test_project_rls_reads_hide_same_org_sibling_and_foreign_org`, `test_project_rls_write_checks_and_composite_foreign_keys`, `test_append_only_catalog_evidence_and_source_uniqueness`, `test_storage_constraints_enforce_metadata_identity_and_evidence` (+7 more)
- `apps/api/tests/integration/spatial/test_repository_scope.py` (primary, 6 tests): `test_repository_fails_closed_before_verified_scope_and_on_wrong_project`, `test_repository_archived_read_allowed_mutation_denied_and_scope_cleared`, `test_repository_denies_ungranted_member_and_action_mismatch`, `test_source_cas_and_audit_are_atomic`, `test_read_scope_cannot_write_and_active_layer_blocks_retire` (+1 more)
- `apps/api/tests/integration/spatial/test_restart.py` (primary, 2 tests): `test_fresh_app_and_pool_recreation_preserve_committed_catalog_and_pins`, `test_interrupted_registration_rolls_back_then_fresh_app_retry_is_complete`
- `apps/api/tests/integration/tenancy/test_deleted_recovery.py` (primary, 2 tests): `test_deleted_workspace_and_project_restore_is_status_only_tenant_scoped_and_audited`, `test_deleted_project_restore_requires_both_permissions_even_with_service_credential`
- `apps/api/tests/integration/tenancy/test_identity_tenancy_migration_cycle.py` (primary, 1 tests): `test_p1_and_p2_migration_cycle_and_logical_backup`
- `apps/api/tests/integration/tenancy/test_identity_tenant_migrations.py` (primary, 4 tests): `test_identity_tenant_schemas_and_tables_exist`, `test_installation_state_is_singleton`, `test_project_workspace_composite_fk_rejects_cross_organization_grouping`, `test_identity_link_issuer_subject_is_unique`
- `apps/api/tests/integration/tenancy/test_invitation_concurrency.py` (primary, 1 tests): `test_concurrent_double_accept_commits_exactly_once`
- `apps/api/tests/integration/tenancy/test_invitation_lifecycle.py` (primary, 5 tests): `test_invitation_recipient_binding_accept_once_and_optional_grant`, `test_invitation_expiry_and_revocation_are_rejected`, `test_runtime_accepts_only_matching_invitation_recipient`, `test_runtime_invitation_expired_or_revoked_is_unusable`, `test_acceptance_reuses_existing_identical_active_grant`
- `apps/api/tests/integration/tenancy/test_invitation_list.py` (primary, 1 tests): `test_invitation_list_is_admin_only_and_excludes_secret`
- `apps/api/tests/integration/tenancy/test_isolated_deployment_parity.py` (primary, 1 tests): `test_same_domain_and_contracts_in_shared_and_isolated_deployment_fixtures`
- `apps/api/tests/integration/tenancy/test_last_owner.py` (primary, 3 tests): `test_concurrent_owner_downgrades_never_leave_zero_active_owners`, `test_runtime_concurrent_owner_role_and_status_changes`, `test_runtime_cannot_remove_the_last_active_owner`
- `apps/api/tests/integration/tenancy/test_organization_lifecycle.py` (primary, 2 tests): `test_organization_lifecycle_is_owner_only_and_suspension_blocks_normal_access`, `test_runtime_owner_controls_suspend_restore_and_soft_delete`
- `apps/api/tests/integration/tenancy/test_p1_audit_mutations.py` (primary, 2 tests): `test_private_runtime_logs_are_not_forwarded`, `test_p1_sensitive_mutations_append_durable_actor_attributed_audit`
- `apps/api/tests/integration/tenancy/test_p1_http_access_gate.py` (primary, 4 tests): `test_http_grant_revocation_takes_effect_on_same_authenticated_session`, `test_http_membership_suspension_or_revocation_denies_next_request`, `test_http_workspace_inheritance_tracks_project_moves_without_grant_copying`, `test_foreign_workspace_create_and_move_payloads_are_non_disclosing`
- `apps/api/tests/integration/tenancy/test_p1_resource_regressions.py` (primary, 2 tests): `test_archived_project_cannot_restore_inside_deleted_workspace`, `test_empty_project_patch_cannot_mutate_or_append_audit`
- `apps/api/tests/integration/tenancy/test_project_ownership_boundary.py` (primary, 1 tests): `test_database_rejects_project_workspace_cross_tenant_reference`
- `apps/api/tests/integration/tenancy/test_revocation.py` (primary, 1 tests): `test_grant_and_membership_revocation_apply_on_next_authorization`
- `apps/api/tests/integration/tenancy/test_tenant_error_semantics.py` (primary, 1 tests): `test_foreign_resource_is_indistinguishable_from_absent_and_not_same_tenant_403`
- `apps/api/tests/integration/tenancy/test_workspace_grant_inheritance.py` (primary, 1 tests): `test_workspace_grant_inheritance_is_dynamic_and_project_grant_is_additive`
- `apps/api/tests/integration/tenancy/test_workspace_project.py` (primary, 3 tests): `test_workspace_project_lifecycle_and_same_org_moves`, `test_cross_organization_workspace_move_is_rejected`, `test_workspace_delete_rejects_active_project`
- `apps/api/tests/integration/test_startup_config.py` (primary, 1 tests): `test_application_import_fails_fast_when_required_config_is_missing`
- `apps/api/tests/integration/test_tenant_foundation_api.py` (primary, 1 tests): `test_real_tenant_create_read_not_found_and_validation`
- `apps/api/tests/integration/test_tenant_foundation_process_restart.py` (primary, 1 tests): `test_tenant_record_survives_ten_real_api_process_restarts`
- `apps/api/tests/integration/test_tenant_foundation_repository.py` (primary, 1 tests): `test_real_tenant_repository_persists_stable_identity_and_timestamps`
- `apps/api/tests/integration/test_tenant_foundation_restart.py` (primary, 1 tests): `test_tenant_create_restart_read_path_ten_consecutive_runs`
- `apps/api/tests/unit/authorization/test_audit_redaction.py` (primary, 2 tests): `test_audit_redaction_removes_all_secret_classes_recursively`, `test_audit_redaction_uses_sensitive_keys_even_for_unknown_secret_format`
- `apps/api/tests/unit/authorization/test_authorization_service.py` (primary, 2 tests): `test_authorization_service_allows_effective_permission`, `test_authorization_service_denies_missing_permission`
- `apps/api/tests/unit/authorization/test_context.py` (primary, 3 tests): `test_requested_actor_context_round_trip_has_no_permission_snapshot`, `test_service_requested_context_preserves_exact_credential_identity`, `test_system_requested_context_json_boundary_and_no_cached_authority`
- `apps/api/tests/unit/authorization/test_repository_context.py` (primary, 11 tests): `test_required_context_and_foreign_org_rejected_before_sql`, `test_raw_tenant_setting_never_proves_membership`, `test_transaction_pin_blocks_raw_scope_switch`, `test_transaction_reset_requires_new_membership_check`, `test_savepoint_transition_invalidates_membership_proof` (+6 more)
- `apps/api/tests/unit/authorization/test_role_grant_union.py` (primary, 2 tests): `test_owner_and_admin_bundles_keep_owner_only_permissions_distinct`, `test_member_guest_and_resource_roles_are_additive_capability_bundles`
- `apps/api/tests/unit/authorization/test_service_permission_ceiling.py` (primary, 2 tests): `test_permission_ceiling_only_restricts_granted_permissions`, `test_permission_ceiling_cannot_expand_principal_grants`
- `apps/api/tests/unit/identity/test_csrf.py` (primary, 1 tests): `test_csrf_token_is_session_bound`
- `apps/api/tests/unit/identity/test_identity_claim_refresh.py` (primary, 1 tests): `test_returning_oidc_login_refreshes_current_identity_claims`
- `apps/api/tests/unit/identity/test_link_conflict_audit.py` (primary, 3 tests): `test_conflict_denial_is_audited_without_foreign_identity_data`, `test_concurrent_same_owner_link_retry_is_idempotent`, `test_unrelated_insert_failure_does_not_become_audited_conflict`
- `apps/api/tests/unit/identity/test_oidc_link_binding.py` (primary, 8 tests): `test_link_reauthenticates_exact_session_and_consumes_state`, `test_link_rejects_changed_or_revoked_session`, `test_link_rejects_session_revoked_before_mutation`, `test_link_rejects_wrong_state_or_provider_without_provider_exchange`, `test_superseded_link_cannot_complete_as_login_or_link` (+3 more)
- `apps/api/tests/unit/identity/test_oidc_provider_validation.py` (primary, 2 tests): `test_oidc_registry_requires_transaction_validated_id_token`, `test_oidc_registry_does_not_treat_string_false_as_verified`
- `apps/api/tests/unit/identity/test_secrets.py` (primary, 2 tests): `test_secret_generation_hash_and_verification`, `test_secret_prefix_validation`
- `apps/api/tests/unit/identity/test_service_credentials.py` (primary, 4 tests): `test_service_secret_encodes_nonsecret_credential_id_and_verifies_hash`, `test_service_secret_rejects_unrecognized_or_malformed_tokens`, `test_service_credential_expiry_and_revocation_are_immediate`, `test_service_principal_status_is_explicit`
- `apps/api/tests/unit/identity/test_sessions.py` (primary, 2 tests): `test_session_token_round_trip_and_digest_verification`, `test_session_token_rejects_tampering`
- `apps/api/tests/unit/tenancy/test_administration_http.py` (primary, 8 tests): `test_tenant_admin_routes_registered`, `test_nonmember_hides_organization_before_tenant_lookup`, `test_admin_cannot_promote_self_to_owner`, `test_admin_cannot_downgrade_existing_owner`, `test_owner_can_restore_suspended_organization` (+3 more)
- `apps/api/tests/unit/tenancy/test_membership_invitation_invariants.py` (primary, 12 tests): `test_admin_cannot_promote_self_or_revoke_owner`, `test_last_owner_downgrade_rejected_while_holding_organization_lock`, `test_owner_transfer_promotes_before_demoting_and_audits_once`, `test_member_and_disabled_admin_cannot_update_org_settings`, `test_inactive_organization_rejects_invite_acceptance_and_normal_mutation` (+7 more)
- `apps/api/tests/unit/tenancy/test_workspace_lifecycle.py` (primary, 1 tests): `test_workspace_with_active_projects_cannot_be_deleted`
- `apps/api/tests/unit/test_config.py` (primary, 8 tests): `test_settings_require_security_and_database_configuration`, `test_settings_reject_unknown_environment`, `test_settings_reject_empty_database_url`, `test_runtime_url_must_be_explicit_and_distinct`, `test_proposed_production_api_has_no_migration_authority` (+3 more)
- `apps/api/tests/unit/test_project_access_lock_order.py` (primary, 1 tests): `test_project_guard_acquires_required_lock_on_first_read`
- `apps/api/tests/unit/test_tenant_foundation_domain.py` (primary, 3 tests): `test_real_project_name_is_trimmed`, `test_real_project_rejects_blank_name`, `test_real_project_rejects_overlong_unicode_name`
- `tests/architecture/test_architecture.py` (cross-ref from E18, 11 tests): `test_rejects_core_to_vertical_module_dependency`, `test_accepts_module_to_public_contract_dependency`, `test_core_validation_succeeds_when_modules_directory_is_absent`, `test_rejects_domain_to_adapter_dependency`, `test_does_not_treat_module_api_urls_or_prose_as_code_dependency` (+6 more)

## 8. Edge Cases & Parity Risks

- Expired or revoked session cookie / service token must fail closed (401) across HTTP, SSE, and WebSocket.
- Cross-tenant organization_id / workspace_id / project_id access must return 403/404 with zero RLS pool bleed.
- Missing or invalid x-openvietgis-csrf header on human mutating endpoints must fail closed.
- Scoped human GIS accounts calling unallowlisted project_access_guard routes must return 403.
- Concurrent OIDC callback / unlink / bootstrap race conditions and last-owner protection.
- Sanitized 422 validation envelope must never echo secrets (bootstrap/invitation/OIDC tokens).

## 9. E01-AC1 Verification

- [x] Live FastAPI runtime instantiated via `create_app()` from worktree (`3e78ac1`), zero mocked route tables.
- [x] All **54** live runtime operations in `E01` extracted with exact OpenAPI method, path, operationId, schemas, auth/CSRF/guard dependencies, and revision/CRS/error semantics.
- [x] 100% of `E01` operations explicitly classified (`migrate` / `retain` / `replace` / `approved_retire`); zero unclassified or silently dropped routes.
- [x] Algorithms/capabilities, **8** owned migrations, **66** source files, and **77** pytest files (**209** test IDs) linked in `contracts/engines/e01.json`.
- [ ] Next phase (`E01-T02`): expand golden/negative/permission fixtures for any operation relying only on engine-level suite coverage.
