# Evidence
Spec initialization only. No code change, runtime implementation, tests, production migration or deployment claimed.
Source inspection baseline: 529fade, current worktree baseline selected from main at initialization. Relevant source: integrations/qgis_plugin/openvietgis_connector/{plugin.py,workspace.py,client.py,metadata.txt}; apps/api/openvietgis_api/adapters/http/{integration_hub.py,integration_bindings.py,gis_access_guard.py}; apps/mcp/workspace_tools.py; specs/027-bidirectional-integration-sync. R028 already reserved by feature/028-windows-v8i-receiver.
Native QGIS certification, auth provider validation, actual latency and RLS tests remain pending.

## Gate A progress
- T001 source inventory complete: architecture-gate.md.
- Existing focused unit tests: 34 passed (editing domain, replay/rebase, QGIS workspace).
- Reference contract tests: 6 passed (cursor scope, retention, failed apply, replay identity, save state); not API integration proof.
- Native spike syntax passed; execution blocked by missing qgis Python package on VPS. Windows/Linux desktop verification not performed.
- T002–T005 remain open; no gate approval or implementation/deploy claim.

## Gate A isolated database verification
- tools/spike_r029_postgres_cursor.py: 4/4 checks against disposable PostgreSQL 18/PostGIS image, no network.
- Existing integration tests: test_scoped_layer_features_and_editing, test_feature_leases_enforce_core_writes_and_expire, test_oidc_authorization_redirect_contains_state_nonce_and_pkce: 3/3 passed using fresh loopback-only database with migrations and runtime RLS role.
- No production changes; both database containers removed. T003 cursor prototype proven, mutation coverage remains open; T004 existing web OIDC proven, native desktop pairing remains open.
- Native QGIS attempt: pinned-tag image not found; qgis/qgis:ltr pull did not finish within bounded 120-second attempt; image unavailable and spike not executed. No host packages installed. T002 remains open; Gate A still blocked.
