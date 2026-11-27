# E14 Rust-Native Parity Evidence — Spec 064

- **Branch**: `064-e14-rust-native` (off `064-convergence`)
- **Scope**: All 27 E14 HTTP routes across `extensions`, `official_modules`, `marketplace`, `marketplace_developers`, and `partner_platform`.

## Route Coverage Matrix (27 / 27 Ported to `api-rust`)

| # | Method | Path | Rust Handler | Storage / Gate |
|---:|---|---|---|---|
| 1 | `GET` | `/api/v1/organizations/{organization_id}/extensions/manifests` | `extensions::manifests::list_manifests` | `OrgExtensionScope::list_manifests` (RLS, human + service principal) |
| 2 | `POST` | `/api/v1/organizations/{organization_id}/extensions/manifests` | `extensions::manifests::register_manifest` | `OrgExtensionScope::register_manifest` (CSRF, human org admin, contract validation, official key guard) |
| 3 | `GET` | `/api/v1/organizations/{organization_id}/extensions/official` | `extensions::manifests::list_official_catalog` | `OrgExtensionScope` + `ModuleRegistry::list_manifests` |
| 4 | `POST` | `/api/v1/organizations/{organization_id}/extensions/official/{extension_key}/register` | `extensions::manifests::register_official_manifest` | `OrgExtensionScope` + `ModuleRegistry` runtime match |
| 5 | `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/activations` | `extensions::activations::list_activations` | `ProjectReadPolicy::LayerAcl` + `ModuleManifest::runtime_contract_matches` |
| 6 | `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/activations` | `extensions::activations::activate_extension` | `ProjectReadPolicy::UnrestrictedManage` + CSRF + audit + `activation_events` |
| 7 | `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/disable` | `extensions::activations::disable_extension` | `ProjectReadPolicy::UnrestrictedManage` + idempotent disable + audit |
| 8 | `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/activations/{activation_id}/deactivate` | `extensions::activations::deactivate_extension` | `ProjectReadPolicy::UnrestrictedManage` + CSRF + audit |
| 9 | `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/activations/{activation_id}/transition` | `extensions::activations::transition_extension` | `ProjectReadPolicy::UnrestrictedManage` + same-major/same-contract CAS |
| 10 | `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/activations/{activation_id}/history` | `extensions::activations::list_activation_history` | `ProjectReadPolicy::Unrestricted` + 1,000-event cap |
| 11 | `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/records` | `extensions::records::list_records` | `ProjectReadPolicy::Unrestricted` + retained-data read |
| 12 | `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/records` | `extensions::records::create_record` | `ProjectReadPolicy::UnrestrictedEdit` + schema/link_context/`operation_id` + revision/audit |
| 13 | `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/records/{record_id}` | `extensions::records::update_record` | `ProjectReadPolicy::UnrestrictedEdit` + optimistic revision/manifest/link CAS + semantic no-op |
| 14 | `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/records/{record_id}/history` | `extensions::records::list_record_history` | `ProjectReadPolicy::Unrestricted` + 1,000-rev / 2 MiB limits |
| 15 | `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/portable-export` | `extensions::records::export_bundle` | `ProjectReadPolicy::Unrestricted` + `Cache-Control: no-store` + canonical SHA-256 |
| 16 | `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/portable-import` | `extensions::records::import_bundle` | `ProjectReadPolicy::UnrestrictedEdit` + SHA-256/contract/link/receipt replay |
| 17 | `GET` | `/api/v1/marketplace/catalog` | `extensions::marketplace::marketplace_catalog` | Ed25519 operator & publisher signature verification + `Cache-Control: no-store` |
| 18 | `POST` | `/api/v1/organizations/{organization_id}/marketplace/install/{publisher_id}/{extension_key}/{version}` | `extensions::marketplace::install_marketplace_release` | CSRF + signed catalog lookup + `OrgExtensionScope::register_manifest` |
| 19 | `GET` | `/api/v1/developer/marketplace/me` | `extensions::marketplace_developers::my_developer_state` | `DeveloperTx` (human RLS) + `Cache-Control: private, no-store` |
| 20 | `POST` | `/api/v1/developer/marketplace/publisher` | `extensions::marketplace_developers::register_publisher` | CSRF + Ed25519 proof verification + advisory lock |
| 21 | `POST` | `/api/v1/developer/marketplace/submissions` | `extensions::marketplace_developers::submit_release` | CSRF + publisher ownership + Ed25519 signature + quota 30 |
| 22 | `POST` | `/api/v1/developer/marketplace/submissions/{submission_id}/withdraw` | `extensions::marketplace_developers::withdraw_submission` | CSRF + owner pending->withdrawn RLS update |
| 23 | `GET` | `/api/v1/developer/marketplace/review-queue` | `extensions::marketplace_developers::review_queue` | Reviewer allowlist + `openvietgis.marketplace_reviewer` GUC |
| 24 | `POST` | `/api/v1/developer/marketplace/review-queue/{submission_id}/decision` | `extensions::marketplace_developers::decide_submission` | CSRF + reviewer allowlist + self-review forbid + immutable decision |
| 25 | `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/partner-contributions` | `extensions::partner_platform::list_contributions` | `ProjectReadPolicy::Unrestricted` + non-scoped + `Cache-Control: no-store` |
| 26 | `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/partner-contributions/{manifest_id}` | `extensions::partner_platform::save_contributions` | `ProjectReadPolicy::UnrestrictedManage` + human manager + GeoJSON/tool validation + CAS |
| 27 | `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integrations/events` | `extensions::partner_platform::list_events` | `ProjectReadPolicy::Unrestricted` + non-scoped + sequence cursor + `Cache-Control: no-store` |

## Differential Parity & CI Results

- Pending GitHub Actions verification on PR into `064-convergence`.
