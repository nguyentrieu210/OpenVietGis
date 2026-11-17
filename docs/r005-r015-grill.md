# R005–R015 research and Grill decision register

Status: **Product owner approved the R005–R015 first-slice scope matrix on 2026-11-18**. Detailed implementation policy, reference data, benchmark and dependency gates remain open as identified below. Documentation only; not a feature spec, plan, or implementation.
Base: verified main ea6db79; Product constitution and ROADMAP remain authoritative.
Each owner must synchronize the current verified main before opening a feature branch. The dependency gates below are implementation gates, not permission to assume a draft contract is accepted.

## Proposed decisions

| Roadmap | Proposed first slice and boundaries | Required accepted prerequisite |
|---|---|---|
| R005 Import/Export | GeoPackage first-class; GeoJSON RFC 7946; ZIP Shapefile with explicit encoding/schema/loss preview. Validate all; immutable original with checksum and CRS provenance; atomically publish a new project-owned source and layer. No append/overwrite. Durable async job; export a fixed revision with explicit CRS, authorization and round-trip report. | R003/R004 (and R002 transitively) |
| R006 Editing | 2D point/line/polygon, geometry and typed attribute CRUD, one active editable source per changeset, basic vertex/segment snapping, local draft undo/redo, explicit Save/Cancel. Validate server-side, atomic revision-checked commit, preserve stale drafts, audit old/new values. No silent repair or advanced CAD. | R005 |
| R007 Queries | Typed visual attribute filters AND/OR/NOT, bounded spatial intersects/within/contains (drawn area or authorized reference layer), map/table selection and deterministic revision-aware paging. No arbitrary SQL, cross-project joins or unbounded results. | R004/R005 |
| R008 Processing | Split job framework (reuse R005 runner) from Buffer/Clip/Dissolve algorithm slices; immutable references to input revisions/parameters/CRS/engine, new output layer, cancel/retry/idempotency and atomic publication. No source mutation or arbitrary scripts. | R005/R007 |
| R009 Cartography | Renderer-neutral single/categorical/range styles, one-field labels, auto legend; one A4/A3 printable layout with north arrow, scale, attribution, CRS and title; PDF/PNG 150/300 DPI. Explicit scale accuracy and immutable export revision manifest. No claim of survey-grade precision or full print composer. | R004/R005 |
| R010 External Sources | Split WMS connector/security, GeoTIFF/COG, WMTS/XYZ, bounded WFS/OGC Features adapters. Remote sources read-only by default; explicit content/licensing/CRS/capabilities, no arbitrary URL proxy. Operator-managed allowlist and secret store, SSRF prevention, bounded parsers, scoped cache/revocation. | R003/R004/R005 |
| R011 Collaboration | Reuse R002 membership/permissions and R006 changesets. Revision conflicts require refresh/reapply, not last-write-wins; project activity and optional assigned post-commit review/comment against exact revision. No duplicate ACL, public sharing, CRDT or live cursors in first slice. Pre-publish review separate if needed. | R002/R006/R007 |
| R012 CAD | Split 2D DXF import/export with explicit entity/CRS/units/loss matrix from precision tools (numeric coordinates, distances/bearings and deterministic snaps). DWG separate licensed adapter gate, no commitment to native full compatibility. Reject unsupported/ungeoreferenced publication, preserve original evidence. | R005/R006/R007; individual slice may refine dependencies |
| R013 Field & Offline | Mobile field shell; one-project scoped expiring package with selected AOI/features, manifest/checksum/CRS/revisions; offline view, point capture and typed attribute edits; local outbox + idempotent sync through R006/R011 with conflict review. No blind merge or promise of remote wipe disconnected. Sensitive local storage requires a separately approved hardened profile. | R005/R006/R011 |
| R014 Modules | Internal versioned extension manifests, permission registry, command/capability registration, module-owned schema/migrations with core contract access. No unrestricted third-party code execution, no business-specific entities in core. Verify two distinct reference workflows (house numbering and a separate land/asset workflow), then consider public SDK. | R006/R007; add R008 only for module using processing |
| R015 Production | Separate Operations (required M3) from commercial billing (optional). First verified small-team self-host container profile; same versioned product and tenant contracts for later cloud/enterprise parity. Real migration/backup/restore, tenant tests, observability and resource safety. No billing until product need; entitlement distinct from authorization; no deletion on subscription change. | R002/R005/R006/R011 |

## Cross-feature contracts that must be settled before opening implementation

1. R002: explicit tenant/actor authorization at submit, run, publish, download; background service actor needs non-secret credential identity so current scope/revocation can be checked. No authz snapshot used as enduring authority.
2. R003: project-scoped source/layer distinction, stable IDs, detailed capability matrix, supported CRS/axis conventions and independently verified VN-2000 control points. Never invent projection parameters or infer all mislabelled axes.
3. R004: accepted map configuration revision, renderer-neutral commands and selection; no renderer internals persisted; personal navigation and shared configuration separated.
4. R005: immutable source assets and derived artifacts, common durable jobs, revision/provenance and failure-safe publication, format-specific loss matrix, independent round-trip.
5. R006: short atomic changesets, conflict and revision semantics, before/after evidence. Every downstream writer (processing/editing/sync/module) uses the accepted appropriate contract.
6. No silent data loss, hidden truncation, cross-tenant data, arbitrary SQL, unbounded job, or provider data egress. Revocation applies on the next protected operation.

## Proposed milestone verification

- M1: authorized import, visualize/select, save/reopen project, export; independently check counts/schema/CRS/geometry/nulls; deny cross-tenant access.
- M2: propose generic land/asset map correction and query workflow with verified reference data; edit and history plus print-ready output. Product owner must confirm actual reference workflow before opening dependent specs.
- M3: two members, distinct permissions, concurrent edit/revocation, deploy/upgrade, backup and isolated restore test. Enumerate release-enabled capabilities, not all advanced branches by default.

## Grill confirmations still required

- Confirm the first-slice matrix above, especially initial format/algorithm/CAD coverage versus broader ambitions (additional abilities remain on roadmap).
- Confirm M2 reference workflow: generic land/asset mapping/correction, not a hardcoded industry module.
- Confirm initial field offline policy: bounded ordinary PWA package for authorized low-sensitivity data; managed/hardened deployment profile for protected geography; no unproven secure-delete claims.
- Confirm M3 initial release profile: small-team self-host; cloud parity and paid billing separate gates.
- Confirm two reference modules for R014: house numbering and generic asset/land workflow, avoid introducing into core.

## Research/test gates (not accepted benchmarks)

Use reference data with known CRS/control points, 10,000 features for initial common cases, explicit complexity/runtime and budgets; independently compare against QGIS/OGR where appropriate. Use real DB/runtime roles, browser, worker, crash/retry, stale revision, revocation, direct-ID substitution and file/service hostile-input cases. Exact benchmark budgets, CRS tolerances, version matrices, resource limits, reference devices and retention stay as evidence/plan gates; no arbitrary guarantee.

## Workflow

This register consolidates proposed research boundaries. After approval record decisions, create separate feature specs from verified main using Spec Kit and mark downstream artifacts draft until their dependencies converge. Keep the roadmap updated only with evidence. Do not merge or implement downstream feature branches merely because the research is complete.
