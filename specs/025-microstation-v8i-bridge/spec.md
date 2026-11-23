# Feature Specification: R025 — MicroStation V8i Bridge

**Feature Branch:** `feature/025-microstation-v8i-bridge`
**Created:** 2026-11-23
**Status:** Implementation candidate / Windows integration pending
**Input:** Connect OpenVietGIS to MicroStation V8i for Vietnamese cadastral datasets, without recurring manual DGN import/export.

## User Scenarios & Testing

### US1 — Inspect a running V8i session (P1)
A cadastral technician running a licensed MicroStation V8i on Windows can check whether the local connector can attach to a running session and identify the active drawing and model. No cloud-side privileges are needed for local probe.

**Independent test:** Run `python bridge.py probe` on a supported Windows test machine; connection status, active DGN/model metadata appear, with no edits.

**Acceptance:**
1. Given MicroStation V8i is not running, when probed, the bridge returns an actionable offline diagnostic without launching or modifying it.
2. Given an accessible running V8i instance, when probed, the bridge reports the active file/model and marks editing and parcel interpretation as **not verified**.

### US2 — Produce a cadastral-safe read-only snapshot (P1)
The technician can turn a typed element snapshot, derived from a V8i COM adapter or offline fixture, into a validated versioned exchange envelope for OpenVietGIS. The user explicitly sets the drawing CRS, including the appropriate VN-2000 zone; it is never guessed.

**Independent test:** Validate the supplied synthetic parcel fixture and diff two revisions without accessing V8i or the production API.

**Acceptance:**
1. Given valid geometry and explicit CRS, when exported, IDs and geometry remain stable in the snapshot.
2. Given missing CRS, duplicate element IDs, invalid geometry or unsupported feature types, when validated, the operation fails closed.
3. Given a disappeared element, when compared, the output is a deletion **candidate** requiring human confirmation, not a delete command.
4. Given unsupported CAD entities, they are counted and reported, not silently converted or discarded.

### US3 — Pair agent to a single OpenVietGIS project (P2)
An authorized project administrator can provision a device-scoped connector and revoke it; the Windows agent makes outbound HTTPS connections only to the configured OpenVietGIS host.

**Independent test:** Authorized project read, project mismatch refusal, revoked/expired credentials refusal, and read-only preview are tested against real isolated database/HTTP API.

**Acceptance:**
1. A project-scoped device cannot read or write other projects/tenants.
2. Pairing secrets are single-use/rotatable, stored securely on device, not exposed in logs; production agent does not reuse partner `project.read` for writes.
3. The server stores source document/version and element mapping, with audit and idempotency; no automatic DGN write-back.

### US4 — Controlled synchronization and CAD write-back (P3, later release)
Detect delta revisions from V8i; preview and confirm modifications; optional write-back only with a separate role, conflict checks, backups and supported V8i license/COM interoperability.

**Independent test:** Two concurrent edits produce a conflict and never silently overwrite either version; rollback restores original DGN from tested backup.

## Functional Requirements

- **FR-001** Windows bridge process communicates with a **running, licensed** V8i through documented automation when installed; no Bentley binary or closed SDK bundled.
- **FR-002** Exchange envelope v1 includes `schema_version`, `source`, explicit `source_crs`, and stable CAD element identifiers scoped by drawing/model; coordinates are in source CRS, never falsely declared WGS84.
- **FR-003** Bridge must detect unsupported/unreadable geometry, and differentiate unsupported CAD objects from empty data.
- **FR-004** Default read-only; no direct remote COM/DCOM exposure, no opening Windows inbound public ports, no automatic write-back or deletion.
- **FR-005** Existing R010 external-sources WMS/WFS/COG **read-only** API remains unchanged; new bridge adapter uses existing organization/project identity, permission, changeset, revision and audit concepts via separately reviewed endpoint.
- **FR-006** No server API write endpoint or UI enablement before service credential, rate limit, idempotency, tenant/RLS and secure pairing are integration-tested.
- **FR-007** Explicit project CRS mapping for VN-2000, field mapping for cadastral attributes and controlled topology/area validation.
- **FR-008** Avoid shipping copyrighted V8i SDK headers, DLLs, typelibs or sample cadastral personal records.
- **FR-009** Fixture mode runs on Linux CI and never claims V8i interoperability.
- **FR-010** Per-connection capability status reflects actual verified operations (probe / snapshot / sync preview / write-back); unsupported actions remain disabled.

## Key Entities

- **DeviceConnection:** organization/project, device public ID, label, last_seen, status, scoped credential reference and revocation state.
- **DrawingBinding:** local CAD drawing fingerprint, V8i model name, explicit source CRS, project layer reference, last reviewed revision.
- **CadElementMapping:** source drawing/model/element ID to OVG feature identity, geometry/attributes digest, last synced revision.
- **SyncBatch:** idempotency key, before/after digests, proposed operations, validation warnings, approval state and audit references.

## Edge Cases

- V8i closed/unlicensed, COM not registered, 32-bit compatibility, active document switched, drawings with multiple models/reference attachments.
- V8i-specific XAttributes, complex chains, text labels, levels, cells, dimension entities and cadastral attributes in **external** FAMIS/gCadas/ViLIS database.
- Mixed/missing project-zone CRS, units, coordinate order, non-closed parcels, self-intersections, topology gaps/overlaps.
- Stale object handles, source drawing renamed/moved, element IDs reused, concurrent on-disk edits, offline replay, partial batches.
- Credential leak/revocation, organization mismatch, very large drawings, unsafe outgoing URLs and sensitive cadastral data in diagnostic logs.

## Success Criteria

- **SC-001** Fixture contract/geometry/diff suite passes in Linux CI, with zero silent edits.
- **SC-002** On an authorized Windows + V8i test machine, attach and report source drawing/model; no V8i installer or SDK is distributed.
- **SC-003** Pilot 100-parcel dataset validated against V8i source, with zero missing elements after accounting for explicitly unsupported CAD types, zero silent CRS transformations and documented attribute coverage.
- **SC-004** P2 data sync passes tenant isolation, idempotency, revoke and audit tests on PostGIS before any production write.
- **SC-005** Two-way synchronization remains **off** until conflict, DGN backup/recovery and licensing are independently proven.

## Assumptions / Out of Scope

- Windows V8i installation/license and real .DGN cadastral test file are prerequisites for Windows-specific acceptance. No such runtime is available on the Linux VPS.
- Connecting to V8i through COM is distinct from parsing arbitrary DGN V8 binary data on the server. No claim of built-in DGN V8 native import.
- This feature does not guarantee access to external geodatabase schemas or proprietary Bentley SDK.
- No changes to live data, migrations or production deployment in this first slice.
