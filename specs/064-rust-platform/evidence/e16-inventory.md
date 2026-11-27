# E16 Inventory & Contract Evidence — AI and MCP

- **Spec**: `specs/064-rust-platform/engines/e16-ai-mcp.md` (Parent refs: R041, R021)
- **Baseline SHA**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Extractor**: `specs/064-rust-platform/contracts/extract_client_module_inventory.py`
- **Contract JSON**: `specs/064-rust-platform/contracts/engines/e16.json`
- **AC1 Status**: `verified` (`E16-T01`)
- **Architecture Candidate**: Migrate MCP server, OAuth verifier, and live bridge internals behind existing MCP/OAuth/ai-copilot contracts; retain browser copilot actions and partner plugin.

## Exact Inventory Counts

| Metric | Count |
|---|---:|
| Runtime HTTP routes (mounted) | 0 |
| SSE streaming endpoints | 0 |
| WebSocket endpoints | 0 |
| MCP tools | 44 |
| Workbench commands | 0 |
| Client SDK / web API paths | 0 |
| Connectors / adapters / services | 30 |
| Migrations | 0 |
| Infra & operational scripts | 0 |
| Tracked source files | 35 |
| Mapped test files | 10 |
| Mapped test IDs | 29 |

## Classification Breakdown

| Classification | Source Files |
|---|---:|
| `migrate` | 24 |
| `retain` | 11 |
| `replace` | 0 |
| `approved_retire` | 0 |

## Auth & Tenant Boundaries

- **tenant_scope_counts**: `{}`
- **routes_with_project_access_guard**: 0
- **routes_with_csrf_enforcement**: 0
- **core_permissions_referenced**: none
- **extension_permissions_referenced**: none
- **mcp_oauth_scopes**: `gis:live`
- **mcp_required_jwt_claims**: `iss`, `sub`, `aud`, `exp`, `iat`, `azp`, `scope`
- **mutation_consent**: All mutating live/remote workspace tools require explicit browser GIS user approval + current revision pins + idempotent operation UUID.

## Component, Connector, Module & Operation Contracts

### OAuth & Security Contract (`migrate`)
- Source: `apps/mcp/remote_oauth.py` (`KeycloakVerifier`, `OidcConfig`)
- Required scopes: `gis:live`
- Required JWT claims: `iss`, `sub`, `aud`, `exp`, `iat`, `azp`, `scope`

### MCP Tools (44 entries, 41 unique names)
| Tool | Server | Transport | Read-Only | Destructive | Source | Classification |
|---|---|---|---|---|---|---|
| `gis_get_project` | `local_readonly` | `stdio` | `True` | `False` | `apps/mcp/server.py:16` | `migrate` |
| `gis_get_saved_map` | `local_readonly` | `stdio` | `True` | `False` | `apps/mcp/server.py:22` | `migrate` |
| `gis_list_layers` | `local_readonly` | `stdio` | `True` | `False` | `apps/mcp/server.py:28` | `migrate` |
| `gis_get_layer` | `local_readonly` | `stdio` | `True` | `False` | `apps/mcp/server.py:34` | `migrate` |
| `gis_list_features` | `local_readonly` | `stdio` | `True` | `False` | `apps/mcp/server.py:40` | `migrate` |
| `gis_live_view` | `live_bridge_stdio` | `stdio` | `True` | `False` | `apps/mcp/live_server.py:16` | `migrate` |
| `gis_live_selection` | `live_bridge_stdio` | `stdio` | `True` | `False` | `apps/mcp/live_server.py:22` | `migrate` |
| `gis_live_navigate` | `live_bridge_stdio` | `stdio` | `False` | `False` | `apps/mcp/live_server.py:28` | `migrate` |
| `gis_live_view` | `remote_oauth_http` | `streamable_http` | `True` | `False` | `apps/mcp/remote_server.py:86` | `migrate` |
| `gis_live_selection` | `remote_oauth_http` | `streamable_http` | `True` | `False` | `apps/mcp/remote_server.py:94` | `migrate` |
| `gis_live_navigate` | `remote_oauth_http` | `streamable_http` | `False` | `False` | `apps/mcp/remote_server.py:102` | `migrate` |
| `gis_operation_status` | `remote_oauth_http` | `streamable_http` | `True` | `False` | `apps/mcp/remote_server.py:119` | `migrate` |
| `gis_create_layer` | `remote_oauth_http (full_tools)` | `streamable_http` | `False` | `False` | `apps/mcp/workspace_tools.py:155` | `migrate` |
| `gis_schema` | `remote_oauth_http (full_tools)` | `streamable_http` | `False` | `False` | `apps/mcp/workspace_tools.py:156` | `migrate` |
| `gis_export` | `remote_oauth_http (full_tools)` | `streamable_http` | `False` | `False` | `apps/mcp/workspace_tools.py:157` | `migrate` |
| `gis_layers` | `remote_oauth_http (full_tools)` | `streamable_http` | `True` | `False` | `apps/mcp/workspace_tools.py:158` | `migrate` |
| `gis_features` | `remote_oauth_http (full_tools)` | `streamable_http` | `True` | `False` | `apps/mcp/workspace_tools.py:159` | `migrate` |
| `gis_query` | `remote_oauth_http (full_tools)` | `streamable_http` | `True` | `False` | `apps/mcp/workspace_tools.py:160` | `migrate` |
| `gis_feature` | `remote_oauth_http (full_tools)` | `streamable_http` | `True` | `False` | `apps/mcp/workspace_tools.py:161` | `migrate` |
| `gis_geometry_preview` | `remote_oauth_http (full_tools)` | `streamable_http` | `True` | `False` | `apps/mcp/workspace_tools.py:162` | `migrate` |
| `gis_topology` | `remote_oauth_http (full_tools)` | `streamable_http` | `True` | `False` | `apps/mcp/workspace_tools.py:163` | `migrate` |
| `gis_overlay` | `remote_oauth_http (full_tools)` | `streamable_http` | `False` | `False` | `apps/mcp/workspace_tools.py:164` | `migrate` |
| `gis_layer_visibility` | `remote_oauth_http (full_tools)` | `streamable_http` | `False` | `False` | `apps/mcp/workspace_tools.py:165` | `migrate` |
| `gis_select` | `remote_oauth_http (full_tools)` | `streamable_http` | `False` | `False` | `apps/mcp/workspace_tools.py:166` | `migrate` |
| `gis_changeset` | `remote_oauth_http (full_tools)` | `streamable_http` | `False` | `True` | `apps/mcp/workspace_tools.py:167` | `migrate` |
| `gis_revert` | `remote_oauth_http (full_tools)` | `streamable_http` | `False` | `True` | `apps/mcp/workspace_tools.py:170` | `migrate` |
| `gis_raster_layers` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `True` | `False` | `apps/mcp/advanced_tools.py:164` | `migrate` |
| `gis_raster_process` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `False` | `False` | `apps/mcp/advanced_tools.py:165` | `migrate` |
| `gis_raster_report` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `True` | `False` | `apps/mcp/advanced_tools.py:166` | `migrate` |
| `gis_raster_download` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `False` | `False` | `apps/mcp/advanced_tools.py:167` | `migrate` |
| `gis_processing_run` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `False` | `False` | `apps/mcp/advanced_tools.py:168` | `migrate` |
| `gis_processing_job` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `True` | `False` | `apps/mcp/advanced_tools.py:169` | `migrate` |
| `gis_models` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `True` | `False` | `apps/mcp/advanced_tools.py:170` | `migrate` |
| `gis_model_save` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `False` | `False` | `apps/mcp/advanced_tools.py:171` | `migrate` |
| `gis_model_run` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `False` | `False` | `apps/mcp/advanced_tools.py:172` | `migrate` |
| `gis_layouts` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `True` | `False` | `apps/mcp/advanced_tools.py:173` | `migrate` |
| `gis_layout_save` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `False` | `False` | `apps/mcp/advanced_tools.py:174` | `migrate` |
| `gis_layout_export` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `False` | `False` | `apps/mcp/advanced_tools.py:175` | `migrate` |
| `gis_plugins` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `True` | `False` | `apps/mcp/advanced_tools.py:176` | `migrate` |
| `gis_plugin_activate` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `False` | `False` | `apps/mcp/advanced_tools.py:177` | `migrate` |
| `gis_plugin_disable` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `False` | `True` | `apps/mcp/advanced_tools.py:178` | `migrate` |
| `gis_plugin_records` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `True` | `False` | `apps/mcp/advanced_tools.py:179` | `migrate` |
| `gis_plugin_create` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `False` | `False` | `apps/mcp/advanced_tools.py:180` | `migrate` |
| `gis_plugin_update` | `remote_oauth_http (full_tools/advanced)` | `streamable_http` | `False` | `False` | `apps/mcp/advanced_tools.py:181` | `migrate` |

### MCP / Bridge HTTP Endpoints (19)
| Methods | Path | Component | Handler | Source | Classification |
|---|---|---|---|---|---|
| `GET` | `/v1/index.html` | `live_bridge_http` | `plugin` | `apps/mcp/bridge_http.py:98` | `migrate` |
| `GET` | `/v1/plugin.js` | `live_bridge_http` | `script` | `apps/mcp/bridge_http.py:98` | `migrate` |
| `GET` | `/v1/style.css` | `live_bridge_http` | `style` | `apps/mcp/bridge_http.py:98` | `migrate` |
| `GET` | `/v1/sdk.js` | `live_bridge_http` | `sdk` | `apps/mcp/bridge_http.py:99` | `migrate` |
| `GET` | `/v1/config` | `live_bridge_http` | `config` | `apps/mcp/bridge_http.py:99` | `migrate` |
| `POST` | `/v1/sessions` | `live_bridge_http` | `create` | `apps/mcp/bridge_http.py:100` | `migrate` |
| `POST` | `/v1/sessions/{sid}/poll` | `live_bridge_http` | `poll` | `apps/mcp/bridge_http.py:101` | `migrate` |
| `POST` | `/v1/sessions/{sid}/ping` | `live_bridge_http` | `ping` | `apps/mcp/bridge_http.py:102` | `migrate` |
| `POST` | `/v1/sessions/{sid}/commands` | `live_bridge_http` | `submit` | `apps/mcp/bridge_http.py:103` | `migrate` |
| `POST` | `/v1/sessions/{sid}/commands/{cid}/ack` | `live_bridge_http` | `ack` | `apps/mcp/bridge_http.py:104` | `migrate` |
| `GET` | `/v1/sessions/{sid}/commands/{cid}/result` | `live_bridge_http` | `result` | `apps/mcp/bridge_http.py:105` | `migrate` |
| `POST` | `/v1/sessions/{sid}/close` | `live_bridge_http` | `close` | `apps/mcp/bridge_http.py:106` | `migrate` |
| `POST` | `/ai-copilot/sessions` | `remote_oauth_copilot_http` | `open_session` | `apps/mcp/remote_server.py:217` | `migrate` |
| `POST` | `/ai-copilot/sessions/{sid}/poll` | `remote_oauth_copilot_http` | `poll` | `apps/mcp/remote_server.py:218` | `migrate` |
| `POST` | `/ai-copilot/sessions/{sid}/ping` | `remote_oauth_copilot_http` | `ping` | `apps/mcp/remote_server.py:219` | `migrate` |
| `POST` | `/ai-copilot/sessions/{sid}/commands/{cid}/prepare` | `remote_oauth_copilot_http` | `prepare` | `apps/mcp/remote_server.py:220` | `migrate` |
| `POST` | `/ai-copilot/sessions/{sid}/commands/{cid}/ack` | `remote_oauth_copilot_http` | `ack` | `apps/mcp/remote_server.py:225` | `migrate` |
| `POST` | `/ai-copilot/sessions/{sid}/close` | `remote_oauth_copilot_http` | `close` | `apps/mcp/remote_server.py:228` | `migrate` |
| `GET,POST,DELETE` | `/mcp` | `remote_oauth_mcp_streamable_http` | `mcp.streamable_http_app` | `apps/mcp/remote_server.py:1` | `migrate` |

## Mapped Existing Test Coverage (10 files, 29 test IDs)

| Test File | Category | Test IDs | Sample Identifiers |
|---|---|---:|---|
| `apps/mcp/deploy/real-keycloak-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/mcp/live-browser-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/mcp/test_advanced_tools.py` | `unit` | 3 | `test_raster_and_layout_model_validation`, `test_long_task_tickets_bound_to_subject_and_version`, `test_long_expiry_and_denial` |
| `apps/mcp/test_live_bridge.py` | `unit` | 6 | `test_roles_and_lifecycle`, `test_isolation_validation_and_expiry`, `test_selection_sanitization`, `test_http_browser_and_ai` (+2 more) |
| `apps/mcp/test_mcp.py` | `unit` | 7 | `test_project_scope_and_token`, `test_feature_pins_and_crs`, `test_api_failure_closed`, `test_invalid_arguments_and_config` (+3 more) |
| `apps/mcp/test_remote.py` | `unit` | 5 | `test_reject_wrong_claims`, `test_reject_invalid_signature_and_accept`, `test_same_origin_identity_and_restricted_scope`, `test_disconnected_workspace_returns_actionable_result` (+1 more) |
| `apps/mcp/test_workspace_tools.py` | `unit` | 3 | `test_changeset_validation_and_legacy_isolation`, `test_old_browser_cannot_receive_workspace_mutations`, `test_full_tool_discovery_and_approved_changeset_relay` |
| `apps/mcp/verify_live_contract.py` | `smoke` | 1 | `script_main` |
| `apps/mcp/web/advanced-integration-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/mcp/web/browser-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |

## E16-AC1 Completion Evidence

- [x] All runtime routes / tools / commands / connectors / scripts extracted directly from source and live FastAPI app introspection (no guessed routes).
- [x] Every source artifact and operation carries an explicit classification (`migrate` | `retain` | `replace` | `approved_retire`).
- [x] Auth, CSRF, OAuth, and tenant/project isolation boundaries documented per operation.
- [x] Existing unit, integration, contract, E2E, and smoke test files and test IDs mapped in `contracts/engines/e16.json`.
