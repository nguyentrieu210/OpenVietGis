# E11 Inventory & Contract Evidence — Realtime, edit leases, chat, tasks

- **Spec**: `specs/064-rust-platform/engines/e11-sync-collaboration.md` (Parent refs: R011, R040)
- **Baseline SHA**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Extractor**: `specs/064-rust-platform/contracts/extract_client_module_inventory.py`
- **Contract JSON**: `specs/064-rust-platform/contracts/engines/e11.json`
- **AC1 Status**: `verified` (`E11-T01`)
- **Architecture Candidate**: Rust/Tokio, transactional outbox, bounded queues; preserve existing SSE and WebSocket contracts.

## Exact Inventory Counts

| Metric | Count |
|---|---:|
| Runtime HTTP routes (mounted) | 57 |
| SSE streaming endpoints | 3 |
| WebSocket endpoints | 1 |
| MCP tools | 0 |
| Workbench commands | 0 |
| Client SDK / web API paths | 0 |
| Connectors / adapters / services | 4 |
| Migrations | 0 |
| Infra & operational scripts | 0 |
| Tracked source files | 11 |
| Mapped test files | 16 |
| Mapped test IDs | 42 |

## Classification Breakdown

| Classification | Source Files |
|---|---:|
| `migrate` | 11 |
| `retain` | 0 |
| `replace` | 0 |
| `approved_retire` | 0 |

## Auth & Tenant Boundaries

- **tenant_scope_counts**: `{"organization+project": 35, "organization": 22}`
- **routes_with_project_access_guard**: 35
- **routes_with_csrf_enforcement**: 31
- **core_permissions_referenced**: `PROJECT_EDIT`, `PROJECT_MANAGE`, `PROJECT_READ`
- **extension_permissions_referenced**: none
- **websocket_auth**: Human session cookie + strict Origin check + project read scope + 5s revalidation; scoped users rejected.
- **sse_auth**: Session/Bearer auth + tenant/project scope + periodic revocation checks (event: revoked).

## Mounted Runtime HTTP / SSE Routes (57)

| Method | Path | Transport | Handler | Tenant Scope | Guard / Auth | Classification |
|---|---|---|---|---|---|---|
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/activities` | `HTTP` | `collaboration.list_activities` | `organization+project` | project_access_guard | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/reviews` | `HTTP` | `collaboration.list_reviews` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/reviews` | `HTTP` | `collaboration.create_review` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/reviews/{review_id}/comments` | `HTTP` | `collaboration.list_comments` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/reviews/{review_id}/comments` | `HTTP` | `collaboration.add_comment` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/reviews/{review_id}/decide` | `HTTP` | `collaboration.decide_review` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/sources/{source_id}/conflict-rebase` | `HTTP` | `collaboration.inspect_conflict_rebase` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/bootstrap` | `HTTP` | `managed_sync.bootstrap_managed_layer` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/commits` | `HTTP` | `managed_sync.commit_managed_changes` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/commits/{operation_id}` | `HTTP` | `managed_sync.get_managed_commit_receipt` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/delta` | `HTTP` | `managed_sync.fetch_managed_delta` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/desktop-sessions/{session_id}/commands` | `HTTP` | `managed_sync.list_desktop_commands` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/desktop-sessions/{session_id}/commands` | `HTTP` | `managed_sync.enqueue_desktop_command` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/desktop-sessions/{session_id}/commands/{command_id}/respond` | `HTTP` | `managed_sync.respond_desktop_command` | `organization+project` | project_access_guard, CSRF, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/events` | `SSE` | `managed_sync.stream_managed_events` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/sessions` | `HTTP` | `managed_sync.list_managed_sessions` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/sessions` | `HTTP` | `managed_sync.create_managed_session` | `organization+project` | project_access_guard, CSRF, PROJECT_READ | `migrate` |
| `DELETE` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/sessions/{session_id}` | `HTTP` | `managed_sync.revoke_managed_session` | `organization+project` | project_access_guard, CSRF, PROJECT_MANAGE, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/sessions/{session_id}` | `HTTP` | `managed_sync.get_managed_session` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/chat/attachments/{attachment_id}` | `HTTP` | `org_chat.attachment` | `organization` | authenticate_request, get_db_session, org_scope, project_scope | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/chat/contacts` | `HTTP` | `org_chat.contacts` | `organization` | authenticate_request, get_db_session, org_scope, project_scope | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/chat/contacts/requests` | `HTTP` | `org_chat.request_contact` | `organization` | CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/chat/contacts/requests/{request_id}/accept` | `HTTP` | `org_chat.accept` | `organization` | CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/chat/contacts/requests/{request_id}/decline` | `HTTP` | `org_chat.decline` | `organization` | CSRF | `migrate` |
| `DELETE` | `/api/v1/organizations/{organization_id}/chat/contacts/{user_id}` | `HTTP` | `org_chat.remove_contact` | `organization` | CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/chat/events` | `SSE` | `org_chat.events` | `organization` | authenticate_request, get_db_session, org_scope, project_scope | `migrate` |
| `DELETE` | `/api/v1/organizations/{organization_id}/chat/messages/{message_id}/pin` | `HTTP` | `org_chat.unpin` | `organization` | CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/chat/messages/{message_id}/pin` | `HTTP` | `org_chat.pin` | `organization` | CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/chat/messages/{message_id}/reactions` | `HTTP` | `org_chat.react` | `organization` | CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/chat/people` | `HTTP` | `org_chat.list_people` | `organization` | authenticate_request, get_db_session, org_scope, project_scope | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/chat/people/{user_id}/avatar` | `HTTP` | `org_chat.person_avatar` | `organization` | authenticate_request, get_db_session, org_scope, project_scope | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/chat/rooms` | `HTTP` | `org_chat.rooms` | `organization` | authenticate_request, get_db_session, org_scope, project_scope | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/chat/rooms` | `HTTP` | `org_chat.create_room` | `organization` | CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/chat/rooms/direct` | `HTTP` | `org_chat.direct_room` | `organization` | CSRF | `migrate` |
| `PATCH` | `/api/v1/organizations/{organization_id}/chat/rooms/{room_id}` | `HTTP` | `org_chat.rename_room` | `organization` | CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/chat/rooms/{room_id}/members` | `HTTP` | `org_chat.invite` | `organization` | CSRF | `migrate` |
| `DELETE` | `/api/v1/organizations/{organization_id}/chat/rooms/{room_id}/members/{user_id}` | `HTTP` | `org_chat.remove_member` | `organization` | CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/chat/rooms/{room_id}/messages` | `HTTP` | `org_chat.messages` | `organization` | authenticate_request, get_db_session, org_scope, project_scope | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/chat/rooms/{room_id}/messages` | `HTTP` | `org_chat.post_message` | `organization` | CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/chat/rooms/{room_id}/pins` | `HTTP` | `org_chat.pins` | `organization` | authenticate_request, get_db_session, org_scope, project_scope | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/chat/rooms/{room_id}/read` | `HTTP` | `org_chat.mark_read` | `organization` | CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/comment-pins` | `HTTP` | `team_chat.comment_pins` | `organization+project` | project_access_guard | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/comments` | `HTTP` | `team_chat.comments` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/comments` | `HTTP` | `team_chat.post_comment` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/members` | `HTTP` | `team_chat.members` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/mentions/{message_id}/read` | `HTTP` | `team_chat.read_mention` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/messages` | `HTTP` | `team_chat.messages` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/messages` | `HTTP` | `team_chat.post_message` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/notifications` | `HTTP` | `team_chat.notifications` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/notifications/{task_id}/read` | `HTTP` | `team_chat.read_notification` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/tasks` | `HTTP` | `team_chat.tasks` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/tasks` | `HTTP` | `team_chat.create_task` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `PATCH` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/tasks/{task_id}` | `HTTP` | `team_chat.change_task` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/edit-locks` | `HTTP` | `team_locks.list_leases` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/edit-locks` | `HTTP` | `team_locks.acquire` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/edit-locks/release` | `HTTP` | `team_locks.release` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/events` | `SSE` | `team_realtime.events` | `organization+project` | project_access_guard | `migrate` |

## Realtime Contracts (SSE & WebSocket)

### `SSE` `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/managed/events`
- Handler: `openvietgis_api.adapters.http.managed_sync.stream_managed_events` (`apps/api/openvietgis_api/adapters/http/managed_sync.py:1079`)
- Classification: `migrate`
- Events: `heartbeat`, `session_revoked`
- PG NOTIFY channels: `ovg_team_collaboration`
- Cursor/replay params: `cursor`
- Revalidates access on stream: `False`

### `SSE` `/api/v1/organizations/{organization_id}/chat/events`
- Handler: `openvietgis_api.adapters.http.org_chat.events` (`apps/api/openvietgis_api/adapters/http/org_chat.py:987`)
- Classification: `migrate`
- Events: `chat`, `contacts`, `ready`, `revoked`, `unavailable`
- PG NOTIFY channels: `ovg_chat`
- Cursor/replay params: `cursor`
- Revalidates access on stream: `True`

### `SSE` `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/events`
- Handler: `openvietgis_api.adapters.http.team_realtime.events` (`apps/api/openvietgis_api/adapters/http/team_realtime.py:105`)
- Classification: `migrate`
- Events: `ready`, `revoked`, `unavailable`
- PG NOTIFY channels: `and`, `ovg_team_collaboration`
- Cursor/replay params: none
- Revalidates access on stream: `True`

### `WebSocket` `/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/presence`
- Handler: `openvietgis_api.adapters.http.team_presence.presence` (`apps/api/openvietgis_api/adapters/http/team_presence.py:104`)
- Classification: `migrate`
- Auth: Human session cookie (_authenticate_human_session) + strict Origin match against web_base_url + project read scope + scoped-user rejection
- Revalidation cadence: `5s`
- Limits: max_global_peers=`64`, max_room_peers=`40`, max_bytes=`2048`
- Close codes: `[1003, 1008, 1013]`
- Messages: in=`['cursor', 'view']`, out=`['presence']`


## Component, Connector, Module & Operation Contracts

### Domain & Persistence (`migrate`)
- `apps/api/openvietgis_api/adapters/persistence/collaboration_repository.py` — classes: 1, functions: 0
- `apps/api/openvietgis_api/domains/collaboration/__init__.py` — classes: 0, functions: 0
- `apps/api/openvietgis_api/domains/collaboration/activity_cursor.py` — classes: 1, functions: 3
- `apps/api/openvietgis_api/domains/collaboration/domain.py` — classes: 4, functions: 2

## Mapped Existing Test Coverage (16 files, 42 test IDs)

| Test File | Category | Test IDs | Sample Identifiers |
|---|---|---:|---|
| `apps/api/tests/integration/integration_hub/test_r029_session_auth.py` | `integration` | 4 | `test_managed_qgis_session_preserves_project_scope_and_edit_capability`, `test_real_pairing_credential_can_open_only_its_binding`, `test_managed_session_actor_isolation_and_epoch_authority`, `test_scoped_delta_does_not_reveal_hidden_feature_ids` |
| `apps/api/tests/integration/m3/test_collaboration_activity_feed.py` | `integration` | 1 | `test_fifty_commits_two_actor_activity_cutoff_and_comment_replay` |
| `apps/api/tests/integration/m3/test_collaboration_revision_chain.py` | `integration` | 1 | `test_replay_actor_and_rebase_exact_history` |
| `apps/api/tests/integration/spatial/test_feature_comments.py` | `integration` | 1 | `test_feature_comments_mentions_and_assignments` |
| `apps/api/tests/integration/spatial/test_org_chat.py` | `integration` | 2 | `test_org_chat_rooms_messages_and_contacts`, `test_org_chat_sse_filter` |
| `apps/api/tests/integration/spatial/test_team_chat.py` | `integration` | 1 | `test_team_chat_and_task_isolation` |
| `apps/api/tests/integration/spatial/test_team_locks.py` | `integration` | 1 | `test_feature_leases_enforce_core_writes_and_expire` |
| `apps/api/tests/integration/spatial/test_team_presence.py` | `integration` | 2 | `test_presence_payload`, `test_presence_access_and_broadcast` |
| `apps/api/tests/integration/spatial/test_team_realtime.py` | `integration` | 2 | `test_private_messages_conversion_and_notification`, `test_sse_pushes_committed_changes` |
| `apps/api/tests/unit/integration_hub/test_r029_managed_sync.py` | `unit` | 6 | `test_r029_desktop_tool_allowlist_blocks_arbitrary_execution`, `test_r029_request_models_forbid_extra_fields`, `test_r029_migration_0041_chains_from_0040`, `test_managed_client_session_bootstrap_delta_commit_and_commands` (+2 more) |
| `apps/api/tests/unit/test_activity_cursor.py` | `unit` | 2 | `test_activity_cursor_roundtrip_scope_expiry_and_order`, `test_activity_cursor_invalid_encoding` |
| `apps/web-next/team-collaboration-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `tools/smoke_org_chat_sse.py` | `smoke` | 1 | `script_main` |
| `tools/tests/test_managed_delta_contract.py` | `unit` | 3 | `test_aliases_same_cursor`, `test_reject_bad_envelope`, `test_reset_allows_rewind_without_changes` |
| `tools/tests/test_managed_snapshot_contract.py` | `unit` | 3 | `test_native_projected_coordinates_and_identity_preserved`, `test_reject_mixed_snapshot`, `test_reject_invalid_initial_page` |
| `tools/tests/test_qgis_managed_snapshots.py` | `unit` | 11 | `test_native_save_does_not_lose_diff`, `test_deletes_are_denied_by_default`, `test_global_cursor_is_not_advanced_by_write_receipt`, `test_native_geometry_wkt_preserves_projected_coordinates` (+7 more) |

## E11-AC1 Completion Evidence

- [x] All runtime routes / tools / commands / connectors / scripts extracted directly from source and live FastAPI app introspection (no guessed routes).
- [x] Every source artifact and operation carries an explicit classification (`migrate` | `retain` | `replace` | `approved_retire`).
- [x] Auth, CSRF, OAuth, and tenant/project isolation boundaries documented per operation.
- [x] Existing unit, integration, contract, E2E, and smoke test files and test IDs mapped in `contracts/engines/e11.json`.
