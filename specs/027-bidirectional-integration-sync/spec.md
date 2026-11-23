# Feature Specification: R027 — Bidirectional Integration Sync

**Feature Branch**: `feature/027-bidirectional-integration-sync`  
**Created**: 2026-11-23  
**Status**: Draft — specification initialized; no runtime implementation or production release  
**Input**: "Trình kết nối OpenVietGIS phải đọc/ghi hai chiều với phần mềm GIS/CAD và hệ thống ngoài, có đồng bộ, phân quyền, xung đột và khôi phục."

## Context, scope and dependencies

R026 Integration Hub supplies the project-scoped catalog, exchange profiles, safe exports and a two-step GeoPackage changeset preview/push candidate. R027 **extends** R026 with durable connection bindings, direction-aware sync, external revisions/cursors, secure delivery and conflict resolution. It MUST NOT falsely claim that R026 is already a live bilateral connector.

**Parents**: R002 (identity and tenant scope), R003 (spatial sources, CRS and revision pins), R005 (import/export), R006 (changesets and revert), R010 (external OGC sources), R011 (team reviews/activity), R013 (field/offline outbox), R014 (extension SDK), R026 (Integration Hub catalog/policy). R025 V8i remains a separately gated Windows bridge.

**MVP priority**: actual end-to-end QGIS connector using an HTTPS API and explicit human authorization; next a controlled external PostGIS bridge through a separate agent. AutoCAD/ArcGIS/MapInfo/MicroStation are adapter candidates until separately integrated and verified.

**Non-goals**: exposing internal PostgreSQL credentials, arbitrary bidirectional WMS/WMTS writes, direct browser access to external secrets, cloud COM/DCOM, silent schema migration, unapproved automatic deletion, generic desktop remote control, and guaranteed real-time updates without an active agent.

## User Scenarios & Testing

### User Story 1 — Connect and inspect both sides (P1)

A project manager connects an approved QGIS workspace/dataset to an OpenVietGIS project layer, authenticates via a supported secure flow, selects the authoritative IDs/fields and explicit CRS, then previews a bidirectional difference without writing any data.

**Independent test**: a fixture with 20 stable features edited on each side yields deterministic additions, updates, unchanged records, permission omissions and conflicts. No data is mutated.

**Acceptance scenarios**
1. Given organization/project rights and a verified QGIS adapter, when the manager connects and maps the data, then the system shows origin, scope, direction, CRS, external ID mapping, capabilities, last sync and disabled operations with reason.
2. Given a restricted field/feature/region or expired credential, when the manager previews, then unauthorized values are absent and writes remain blocked.
3. Given missing/unknown CRS or non-round-trippable geometry, when the user previews, then the affected changes are rejected with diagnostic details rather than silently reprojected.

### User Story 2 — Approve and synchronize edits in both directions (P1)

A user edits a parcel in QGIS; a permitted sync sends the change to OpenVietGIS via R006 changeset. A different permitted edit in OpenVietGIS can then flow back to QGIS. Both retain stable source identity, revision checks and auditable origin.

**Independent test**: edit one attribute and one polygon on each side, synchronize in both directions through the same binding, and verify final geometry/attributes, provenance and no echo-loop.

**Acceptance scenarios**
1. Given an active binding with `bidirectional_manual`, when two previewed, non-conflicting changes are explicitly approved, then each destination receives only authorized differences, and checkpoints advance only after durable acknowledgment.
2. Given a repeated request after timeout, when the same operation ID is retried, then the receiver returns the prior result and never applies the edit twice.
3. Given a change from OpenVietGIS that already originated from QGIS, when outbound events are processed, then origin-chain/cursor deduplication prevents a feedback loop.
4. Given a stale base revision or changed ACL, when commit is attempted, then the run is rejected, paused or flagged as conflict; it MUST NOT overwrite newer edits.

### User Story 3 — Resolve conflicts and recover (P1)

Two collaborators edit the same feature or schema while disconnected. The manager reviews field-level and geometry-level differences, selects an explicit resolution or postpones it, and can recover safely after partial failure.

**Independent test**: both ends edit the same property, one edits geometry, one deletes a feature, and a connection drops halfway; no silent overwrite/deletion occurs, and retries resume exactly once.

**Acceptance scenarios**
1. A concurrent edit produces a conflict with both revisions, geometry/field diffs and traceable actors without leaking fields unauthorized to the reviewer.
2. Deletion is blocked by default; enabling it requires a separate explicitly approved policy and per-run review of impacted IDs/count.
3. An interrupted batch resumes from acknowledged operations without replaying completed changes, and the operator can stop/pause the binding.
4. Revocation immediately blocks new reads, writes and retries; queued payloads are held or purged per security policy rather than delivered after revocation.

### User Story 4 — Controlled automation and observability (P2)

A manager opts into event-driven or scheduled synchronization with understandable cadence, queue status, failure counts, last successful checkpoint and retry diagnostics.

**Independent test**: with an active QGIS agent, two authorized changes are delivered without a manual refresh; when the agent goes offline the UI displays `offline` and pending work, and reconciliation resumes only after authorization recheck.

**Acceptance scenarios**
1. The UI distinguishes connected, offline, paused, unauthorized, conflict, degraded and unsupported adapter states.
2. Event delivery may be at-least-once but applying a change is idempotent.
3. Automatic mode only handles safe create/update operations within an explicitly approved binding policy; conflicted records and deletes go to review.

### User Story 5 — Additional provider adapters (P3)

Authorized implementers can add external PostGIS bridge, ArcGIS, AutoCAD, MapInfo and V8i adapters behind capability negotiation without modifying R006 core logic. Only independently tested providers become `verified`.

## Edge cases

- External IDs reused or missing; two source rows map to one destination; feature split/merge; deleted then recreated object.
- Geometry Z/M, multipart, curves, topology invalidity, precision/tolerance mismatch, EPSG:4756/5899 vs WGS84, transformation outside area of use.
- Schema rename, field type conversion, null vs absent value, domain/code-list mismatch, attachment or styling loss.
- Concurrent connector jobs, cursor gap/expired event retention, partial page, out-of-order events, revoked credentials while queued.
- Origin echo-loop, repeated webhook, delayed agent reconnect, clock drift (timestamps MUST NOT determine conflict precedence).
- Connector unavailable/licensing not installed; provider read-only; plugin version upgrade invalidates mapping.
- Large layers, rate limit, bounded batches, backend restart and network partitions.

## Functional Requirements

- **FR-001**: One project-scoped registry MUST declare provider, adapter version, execution status, binding ID, user-visible direction and capabilities; planned ≠ verified.
- **FR-002**: Binding creation and activation MUST require an authorized human with project management rights; per-operation authorization MUST additionally check current org/project/layer/feature/field/region restrictions and service credential ceiling.
- **FR-003**: All connections MUST use secure per-binding credentials/secret references stored server-side or on the trusted local agent, never in browser bundles, partner iframe, QGIS project file, repo or audit log.
- **FR-004**: System MUST support `pull_only`, `push_only`, `bidirectional_manual`, `bidirectional_auto`; new bindings default to disabled, `bidirectional_manual` with all writes denied until explicit grant.
- **FR-005**: A read-only preview MUST show create/update/delete candidates, match keys, schema/CRS/geometry validation, losses, unavailable permissions, conflicts and exact base revisions on both sides.
- **FR-006**: Source identity mapping MUST use stable external IDs and a durable mapping journal, never name-based guessing; no automatic merges of ambiguous IDs.
- **FR-007**: Every OpenVietGIS write MUST go via R006 revision-pinned changesets and existing review/permission gates; no connector may mutate tenant tables directly.
- **FR-008**: Every outbound write MUST require a receiver supporting versioned conditional update/acknowledgment or a trusted adapter with equivalent verified semantics; otherwise downgrade to preview/export-only.
- **FR-009**: Every operation MUST have unique idempotency ID and immutable origin chain; every delivered event MUST have ordered sequence/cursor, retry state and durable acknowledgment.
- **FR-010**: Conflicts MUST halt the affected feature (not necessarily the whole healthy batch); user may choose local, remote or explicit field merge only after permissions/revision recheck. Geometry conflicts require explicit geometry selection/review.
- **FR-011**: Deletes MUST be denied by default and NEVER automatically replayed. If separately enabled, use tombstone/quarantine, impact preview, explicit approval and reversible retention policy.
- **FR-012**: Per-binding pause, revoke, resume and credential rotation MUST stop unauthorized work immediately; incomplete transactions MUST remain recoverable and auditable.
- **FR-013**: System MUST use explicit, verified source/target CRS, known transform pipeline and reproducible round-trip tolerance tests; reject unknown or unsupported CAD entities without lossless promises.
- **FR-014**: System MUST provide a durable sync-run ledger showing source revision, target revision, counts, applied/skipped/conflicted/rejected entries, errors, approval actor and timestamps.
- **FR-015**: Provider-side change capture MUST truthfully distinguish native events vs polling vs manual snapshots, with documented gap detection and recovery.
- **FR-016**: Connector UI MUST label unsupported providers as `planned`/`candidate` and disable execution with a reason; no fake login, success, live connection or sync state.
- **FR-017**: No activation/deployment until threat model, real PostGIS tenant/RLS tests, credential scope/revocation tests, disaster/rollback drill and actual QGIS desktop acceptance pass.
- **FR-018**: Reuse R010 SSRF/allowlist for outbound remote requests; restrict endpoint origins, DNS rebinding, redirects, egress and payload size; rate-limit tenant/binding throughput.

## Key Entities

- **ConnectorBinding**: tenant/project + layer/provider/device, authorization and lifecycle.
- **SyncProfile**: direction, allowed fields/operations, mapping, CRS transformations, conflict/delete policies.
- **ExternalIdentityMap**: immutable association of provider dataset ID/external feature ID and OpenVietGIS feature ID.
- **SyncCursor**: monotonic source/target offsets, gaps and last ack.
- **ChangeEnvelope / SyncOperation**: operation ID, origin chain, revisions, change digest and status.
- **SyncRun / SyncConflict / SyncApproval**: durable execution, resolution and audit.
- **ConnectorCredentialRef**: encrypted/managed secret reference, metadata, rotation/revocation (never raw secrets in API).

## Success Criteria

- **SC-001**: In a 20-feature QGIS acceptance fixture, every permitted add/update is accurately synchronized both ways in manual mode, with zero silent overwrites and zero unauthorized field disclosures.
- **SC-002**: 100 repeated deliveries of the same operation result in exactly one applied state transition and one corresponding change identity.
- **SC-003**: Cross-tenant, revoked, and GIS-scoped denial suites pass 100% on real PostgreSQL/PostGIS with a signed-off security review.
- **SC-004**: A 1,000-feature test batch completes with every record accounted for (applied/rejected/conflicted/skipped); benchmark execution duration and payload/memory budgets are recorded before production gate.
- **SC-005**: Killing agent/network midway and restarting yields zero duplicated edits and no lost acknowledged changes; queued unacknowledged work resumes safely or is explicitly quarantined.
- **SC-006**: All destructive candidate changes surface a review state; automated deletion count is zero by default.

## Assumptions and Decisions

- MVP is QGIS-first, authenticated over HTTPS; no direct connection to internal OpenVietGIS PostGIS.
- First implementation uses manual preview/approval, auto-sync is a later independently gated slice.
- Only a real provider agent/plugin may claim bidirectional edits; generic file exchange is snapshot import/export, not two-way sync.
- R026 catalog and two-step GeoPackage candidate remain in place and are never silently bypassed.
- Actual external provider APIs/licenses, review policy and numerical SLA must be independently verified before enabling each adapter.
