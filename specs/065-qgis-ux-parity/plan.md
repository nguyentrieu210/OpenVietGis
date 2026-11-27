# Implementation Plan: QGIS UX & Functional Parity
**Branch**: 065-qgis-ux-parity | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

## Summary
Deliver 8 waves with 71 explicit matrix checks. Start with audit baseline and common shell contracts, then layer/dock, style, attributes, editing, processing, output, and cross-engine completion. Each wave independently testable, reviewable and reversible.

## Technical context
- Frontend: existing React/TypeScript, apps/web-next, existing CSS/token system. Use existing QgisWorkspace, LayerProperties, CoreMapTools, DockLayout and typed client SDK rather than rebuilding.
- Backend: Rust API Axum/Tokio and PostgreSQL/PostGIS; Rust MCP in apps/mcp-rust. Keep QGIS plugin/desktop adapter and existing GIS engines. Do not rewrite API unless a verified capability gap requires an additive contract.
- Runtime: MapLibre 2D, Cesium 3D, OpenCAD WASM relay; existing ModeCoordinator. Same canonical frontend output, no GuestStudio.
- Tests: Vitest/Node tests, Playwright tests/e2e, Rust cargo test, real PostGIS integration fixtures, QGIS desktop comparison.
- Proposed benchmark profile: 4 vCPU/8 GB RAM test workstation, Chromium stable, desktop 1440×900; 1k/35k/100k vector objects, 20 layers, 2 concurrent users, raster fixture 2048×2048. Record actual machine/RTT/cache; run cold/warm at least 30 samples per scenario and p50/p95. Browser and QGIS benchmark environment recorded separately. Targets from spec SC-005; 60 fps is aspirational, not assumed.
- Security: server authz, source revisions, CSRF and tenant scope; client cannot grant capability. No production credentials in evidence.

## Constitution check
PASS design intent, implementation not yet accepted: platform-first (no domain logic in shell); open data and interop; tenant isolation; explicit CRS; audit and versioning; capability-driven commands; web-native engine separation; real tests and rollback. Re-check at each wave. ADR required if changing shared persistence, geometry engine, expression semantics or provider strategy.

## Workstream structure
### Wave 1: Application Shell & điều hướng — US1
- Owner boundary: apps/web-next/src/LiveApp.tsx, SpatialWorkspace.tsx, QgisWorkspace.tsx, CoreCommandBar.tsx, hotkeys.ts.
- Scope: SHELL-01, SHELL-02, SHELL-03, SHELL-04, SHELL-05, SHELL-06, SHELL-07, SHELL-08.
- Before editing: compare source implementation and QGIS fixture, mark proven gaps.
- Deliverable: code + targeted test + evidence + updated feature-matrix status; no partial parity claim.
- Gate: relevant unit/integration/real browser tests, prior wave regression and permissions. Only dependent tasks wait; independent workstreams may run parallel without editing same files.

### Wave 2: Dock, Browser & Layers — US2
- Owner boundary: apps/web-next/src/DockLayout.tsx, Splitter.tsx, QgisWorkspace.tsx, layer-tree.ts, CatalogManager.tsx, DsmPanes.tsx.
- Scope: DOCK-01, DOCK-02, DOCK-03, DOCK-04, DOCK-05, DOCK-06, DOCK-07, DOCK-08, DOCK-09, DOCK-10.
- Before editing: compare source implementation and QGIS fixture, mark proven gaps.
- Deliverable: code + targeted test + evidence + updated feature-matrix status; no partial parity claim.
- Gate: relevant unit/integration/real browser tests, prior wave regression and permissions. Only dependent tasks wait; independent workstreams may run parallel without editing same files.

### Wave 3: Layer Properties, Symbology, Labels — US3
- Owner boundary: apps/web-next/src/LayerProperties.tsx, style-io.ts, label-expr.ts, graduated.ts, ProfessionalTools.tsx.
- Scope: STYLE-01, STYLE-02, STYLE-03, STYLE-04, STYLE-05, STYLE-06, STYLE-07, STYLE-08, STYLE-09.
- Before editing: compare source implementation and QGIS fixture, mark proven gaps.
- Deliverable: code + targeted test + evidence + updated feature-matrix status; no partial parity claim.
- Gate: relevant unit/integration/real browser tests, prior wave regression and permissions. Only dependent tasks wait; independent workstreams may run parallel without editing same files.

### Wave 4: Attribute Table, Form & Selection — US4
- Owner boundary: apps/web-next/src/QgisWorkspace.tsx, FeatureInfo.tsx, ProfessionalTools.tsx, AdvancedEditor.tsx.
- Scope: TABLE-01, TABLE-02, TABLE-03, TABLE-04, TABLE-05, TABLE-06, TABLE-07, TABLE-08.
- Before editing: compare source implementation and QGIS fixture, mark proven gaps.
- Deliverable: code + targeted test + evidence + updated feature-matrix status; no partial parity claim.
- Gate: relevant unit/integration/real browser tests, prior wave regression and permissions. Only dependent tasks wait; independent workstreams may run parallel without editing same files.

### Wave 5: Digitizing, Snapping & Topology — US5
- Owner boundary: apps/web-next/src/CoreMapTools.tsx, AdvancedEditor.tsx, SnapMath.ts, ProfessionalTools.tsx; apps/api-rust/src/editing.rs.
- Scope: EDIT-01, EDIT-02, EDIT-03, EDIT-04, EDIT-05, EDIT-06, EDIT-07, EDIT-08, EDIT-09.
- Before editing: compare source implementation and QGIS fixture, mark proven gaps.
- Deliverable: code + targeted test + evidence + updated feature-matrix status; no partial parity claim.
- Gate: relevant unit/integration/real browser tests, prior wave regression and permissions. Only dependent tasks wait; independent workstreams may run parallel without editing same files.

### Wave 6: Processing, Raster & Analysis — US6
- Owner boundary: apps/web-next/src/ProcessingToolbox.tsx, ProcessingModeler.tsx, RasterWorkbench.tsx, RasterReportWorkbench.tsx, TerrainEngineering.tsx; apps/api-rust/src/processing.rs.
- Scope: PROC-01, PROC-02, PROC-03, PROC-04, PROC-05, PROC-06, PROC-07, PROC-08, PROC-09, PROC-10.
- Before editing: compare source implementation and QGIS fixture, mark proven gaps.
- Deliverable: code + targeted test + evidence + updated feature-matrix status; no partial parity claim.
- Gate: relevant unit/integration/real browser tests, prior wave regression and permissions. Only dependent tasks wait; independent workstreams may run parallel without editing same files.

### Wave 7: Print Layout, CRS & interoperability — US7
- Owner boundary: apps/web-next/src/LayoutDesigner.tsx, PrintDesigner.tsx, LayoutRender.ts, ExportDialog.tsx, TransferTool.tsx, QgisBindingManager.tsx, crs-display.ts.
- Scope: OUTPUT-01, OUTPUT-02, OUTPUT-03, OUTPUT-04, OUTPUT-05, OUTPUT-06, OUTPUT-07, OUTPUT-08.
- Before editing: compare source implementation and QGIS fixture, mark proven gaps.
- Deliverable: code + targeted test + evidence + updated feature-matrix status; no partial parity claim.
- Gate: relevant unit/integration/real browser tests, prior wave regression and permissions. Only dependent tasks wait; independent workstreams may run parallel without editing same files.

### Wave 8: CAD/3D, collaboration, AI & cross-cutting — US8
- Owner boundary: apps/web-next/src/SpatialWorkspace.tsx, CadEditorPanel.tsx, MapChat.tsx, OrgChat.tsx, AiCopilotPanel.tsx, ai-copilot/; apps/mcp-rust/.
- Scope: COLLAB-01, COLLAB-02, COLLAB-03, COLLAB-04, COLLAB-05, COLLAB-06, COLLAB-07, COLLAB-08, COLLAB-09.
- Before editing: compare source implementation and QGIS fixture, mark proven gaps.
- Deliverable: code + targeted test + evidence + updated feature-matrix status; no partial parity claim.
- Gate: relevant unit/integration/real browser tests, prior wave regression and permissions. Only dependent tasks wait; independent workstreams may run parallel without editing same files.

## Release order and rollback
W0 audit baseline → W1 shell/command foundations → W2 docks/layers → W3 properties/expressions → W4 table/form → W5 digitizing/topology → W6 processing → W7 outputs → W8 cross-engine & final QA. P1 bugs in later waves may be pulled forward if dependencies pass. Shared contracts stabilize before parallel engine-specific changes. Each wave separate PR, no broad reset or force push. Rollout to DEV, real browser acceptance, artifact hash and GitHub CI check, deploy already-built artifact, smoke public and authenticated route, keep prior compatible web image. API changes additive and rollback-tested; no DB reset. Stop promotion (not development) on failed gate; document exact blocker. No deploy in spec-only phase.

## Evidence locations
`specs/065-qgis-ux-parity/evidence/<wave>/<id>.md` (future), screenshot paths/fixture checksums/test reports in CI artifact; never include tenant/private data.
