# Quickstart — R027 Bidirectional Integration Sync

**Current state:** Foundation Slice A has branch-only implementation of disabled QGIS binding drafts and a pure three-way diff planner, with isolated PostGIS tests. **No R027 production routes or live two-way synchronization are deployed.** The broader sync endpoints below remain proposals. See [implementation evidence](evidence.md).

## Intended user flow (QGIS first)

1. Organization/project manager opens **Tích hợp → Trình kết nối**, selects **QGIS**, sees verified capabilities and permission requirements.
2. Install an independently reviewed QGIS plugin/agent that authenticates over HTTPS; never place a service token in the QGIS project file or browser.
3. Create a **disabled** binding for one OpenVietGIS layer and a chosen external dataset. Choose explicit EPSG/CRS transform, external ID and field mapping.
4. Preview inbound and outbound changes. Review creates, updates, conflicts, unauthorized fields, geometry/CRS loss and deletes.
5. Explicitly activate a manual binding and approve an exact preview digest. OpenVietGIS writes use R006 changesets; QGIS applies a conditional update and returns durable acknowledgments.
6. Inspect run ledger and resolve conflicts; pause/revoke at any time. Enable automation only after a separate approval, with creates/updates only.

## Contract discovery

Proposed OpenAPI 3.1 document: [contracts/sync-api.yaml](contracts/sync-api.yaml). Endpoint prefix: `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub`. Proposed operations: `/bindings`, `/bindings/{binding_id}/sync/preview`, `/bindings/{binding_id}/sync/commit`, `/bindings/{binding_id}/runs`, `/runs/{run_id}/ack`, and conflict resolution.

Only API behavior actually implemented in a release should be exposed by a runtime OpenAPI document. Never point user/agent at speculative endpoints expecting them to work.

## Acceptance fixtures

- **F1 normal round-trip**: 20 features, known stable IDs, two geometry edits and two field edits across two sides; no echo changes.
- **F2 concurrent edit**: same feature changed independently; conflict until approved, no lost edit.
- **F3 permission**: user can edit only authorized feature/field; no unauthorized data exposed in preview or worker logs.
- **F4 CRS**: EPSG:4326 and verified VN-2000 control points; unknown/unverified CRS fails closed.
- **F5 failures**: agent unavailable, retry same idempotency key 100x, network loss after target commit before receipt, revoked credential and cursor gap.
- **F6 deletes**: delete candidate must be quarantined, never automatically applied by default.
- **F7 tenant isolation**: real PostgreSQL/PostGIS with cross-tenant, RLS and service principal ceiling test.

## Development guardrails

Use `feature/027-bidirectional-integration-sync`; start from checked `main` and build on actual R026/R006/R013 contracts. Do not connect to or alter real cadastral data, issue production credentials, open internal database port or deploy an unreviewed bridge. Stage with synthetic datasets, then record test commands, exit codes, release SHA and rollback results under a future `evidence.md`.

**Definition of done**: R027 FR/SC coverage, signed-off ADR, real QGIS acceptance, security review, real PostGIS/RLS and recovery drills, compatibility checks, reviewed PR and verified production release. Spec Kit initialization does not fulfill these gates.