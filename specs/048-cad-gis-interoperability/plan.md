# Implementation Plan: R048 interoperability
**Branch**: `048-cad-gis-interoperability` | **Date**: 2026-11-24 | **Spec**: [spec](spec.md)

## Summary
Reuse existing project, ChangeSet, CAD vault, binding and realtime contracts. Build thin adapter and mapping semantics, not a competing integration engine. Canonical GIS state stays in PostGIS; original CAD files and native entities remain authoritative for CAD-only details. Per binding authority must be explicit; sync is reviewed and eventually consistent across document storage and feature state.

## Technical context
Python/FastAPI/PostGIS server; TS/React web-next + spatial-workspace/client SDK; separately versioned Rust/Iced/WASM OpenVietCAD; existing isolated HTTPS relay; QGIS/V8i via their existing connector profiles. Tests pytest/PostGIS, TS tests, Playwright actual WASM, Rust fixtures. No dependency upgrade implied. Reference budgets SC-001..007.

## Constitution check
Platform-first: extension of existing contracts; no domain hardcode. Open data: retain native bytes and portable exports. Tenant isolation: server policy every operation including replay. Spatial correctness: explicit CRS/control/tolerance. Version/audit: existing ChangeSet and CAD version receipts. Capability-driven, web-native: R031 adapter seam, no duplication. Workflow value: linked parcel/utility roundtrip before additional tools. Design conforms on paper; runtime compliance remains open.

## Sources / planned code locations
- apps/api/openvietgis_api/domains/cad/georeference.py and adapters/http/cad_documents.py; adapters/persistence/cad_repository.py; existing R006/R027/R029 components.
- apps/api/migrations/versions/ only additive migration after current head and review.
- packages/spatial-workspace/src/cad-editor.ts, cad-georeference.ts; apps/web-next/src/CadEditorPanel.tsx and SpatialWorkspace.tsx.
- Existing client SDK, integration hub and MCP command registry; tests under apps/api/tests/integration/spatial and apps/web-next.
- OpenVietCAD separate repo: source UI/runtime and CI changes isolated; no Rust compile on GIS host.

## Phases and gates
G0 inventory and pinned baselines. G1 independent CAD stability + safe read-only selection and mapping fixtures; integration adapter changes wait for R031 reviewed merge. G2 reviewed roundtrip and bidirectional commits. G3 delta/reconnect/conflict. G4 offline and AI/connector qualification. G5 performance, exact candidate, human acceptance and rollback. Each gate default-off until its evidence passes; no dual writes. Release not automatic. See [tasks](tasks.md) and [quickstart](quickstart.md).

## Parallel work boundaries
CAD team owns OpenVietCAD repo and benchmark/fixtures only; GIS backend owns mapping/transactions; UI team owns adapter UX after R031; QA owns fixture suite. R031/R027/R029 changes must be reconciled against merged main before implementation. Avoid edits to R031 branch and unrelated worktrees. Review on exact SHAs, no production data.
