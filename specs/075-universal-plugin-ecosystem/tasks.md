# Tasks — SPEC 075
All tasks initially unchecked. Implement all assigned tasks before consolidated CI for that scope; focused checks while editing allowed. Never mark done without evidence. F gates and each PLG-xxx-A/B/C/D required; blocking external credentials/devices/legal approvals stay open with evidence, not mock pass.
## Wave 0
- [ ] F001 Inventory canonical registry, manifest validators, migrations, commands, SDK, existing plugin IDs and 073/074 boundaries; publish reuse/extend/new map.
- [ ] F002 Freeze contract and golden fixtures; review ADR, compatibility, API and migration ownership.
- [ ] F003 Implement versioned release catalog, publisher identity, signatures, keys, digest and revocation.
- [ ] F004 Implement dependency solver, lock graph, host profiles and lifecycle idempotency.
- [ ] F005 Implement scoped broker, authorization, sandbox/governor, audit and lifecycle disposal.
- [ ] F006 Extend existing Web host and mobile/desktop declarative adapters, vi/en/accessibility.
- [ ] F007 Extend SDK and generated clients, docs, sample build/sign/validate/publish workflow.
- [ ] F008 Implement developer portal, moderation, public/private catalog, release channels and rollback.
- [ ] F009 Implement entitlement/grants/leases, checkout sandbox, signed webhook, ledger and reconciliation.
- [ ] F010 Implement tenant scoped persistence, migration, upgrade/recovery, export and uninstall data policy.
- [ ] F011 Qualify two reference plugins, existing house numbering/QGIS parity and cross-host E2E.
- [ ] F012 Run consolidated Wave 0 acceptance, supply chain, security, load and rollback; record exact candidate.

## Wave 1 — GIS Professional
### PLG-001 Spatial Analysis
- [ ] PLG-001-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for buffer/overlay/spatial join with explicit CRS.
- [ ] PLG-001-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: buffer/overlay/spatial join with explicit CRS.
- [ ] PLG-001-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-001-D Execute oracle: compare polygon overlay area and attributes with independent fixtures; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-002 Topology Inspector
- [ ] PLG-002-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for validate adjacency, gaps, overlaps, connectivity.
- [ ] PLG-002-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: validate adjacency, gaps, overlaps, connectivity.
- [ ] PLG-002-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-002-D Execute oracle: find seeded overlap/gap and repair only approved features; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-003 Geometry Repair
- [ ] PLG-003-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for preview and apply valid geometry repairs.
- [ ] PLG-003-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: preview and apply valid geometry repairs.
- [ ] PLG-003-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-003-D Execute oracle: bow-tie fixture repaired with audit and original retained; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-004 Network Analysis
- [ ] PLG-004-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for graph routing, costs and restrictions.
- [ ] PLG-004-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: graph routing, costs and restrictions.
- [ ] PLG-004-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-004-D Execute oracle: known graph shortest path and unreachable node; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-005 Spatial Statistics
- [ ] PLG-005-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for density, clustering and spatial autocorrelation.
- [ ] PLG-005-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: density, clustering and spatial autocorrelation.
- [ ] PLG-005-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-005-D Execute oracle: reference statistic and reproducible seed; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-006 Geostatistics
- [ ] PLG-006-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for IDW, kriging and uncertainty surface.
- [ ] PLG-006-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: IDW, kriging and uncertainty surface.
- [ ] PLG-006-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-006-D Execute oracle: holdout error and variogram parameters recorded; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-007 Coordinate Transformer
- [ ] PLG-007-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for batch coordinate transforms and provenance.
- [ ] PLG-007-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: batch coordinate transforms and provenance.
- [ ] PLG-007-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-007-D Execute oracle: independent control points and reject missing CRS; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-008 Georeferencer
- [ ] PLG-008-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for GCP fitting, residuals, warp and export.
- [ ] PLG-008-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: GCP fitting, residuals, warp and export.
- [ ] PLG-008-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-008-D Execute oracle: known affine control points and residual report; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-009 Advanced Digitizing
- [ ] PLG-009-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for snapping, constraints and precise editing.
- [ ] PLG-009-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: snapping, constraints and precise editing.
- [ ] PLG-009-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-009-D Execute oracle: known geometry tolerance and undo/redo; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-010 Spatial ETL
- [ ] PLG-010-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for typed mapping, transform and scheduled import.
- [ ] PLG-010-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: typed mapping, transform and scheduled import.
- [ ] PLG-010-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-010-D Execute oracle: round-trip schema, restart and idempotent rerun; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 2 — AI & Autonomous GIS
### PLG-011 AI Copilot
- [ ] PLG-011-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for natural-language scoped tool invocation.
- [ ] PLG-011-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: natural-language scoped tool invocation.
- [ ] PLG-011-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-011-D Execute oracle: propose then confirm a mutation; deny cross-project tool; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-012 AI Agent Builder
- [ ] PLG-012-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for versioned agent prompts, tools and policies.
- [ ] PLG-012-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: versioned agent prompts, tools and policies.
- [ ] PLG-012-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-012-D Execute oracle: publish bounded agent and replay logged tool trace; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-013 Multi-Agent Studio
- [ ] PLG-013-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for DAG orchestration, budgets and cancellation.
- [ ] PLG-013-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: DAG orchestration, budgets and cancellation.
- [ ] PLG-013-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-013-D Execute oracle: parallel tasks converge and cancel without orphan jobs; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-014 AutoDigitizer
- [ ] PLG-014-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for extract vectors from georeferenced imagery.
- [ ] PLG-014-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: extract vectors from georeferenced imagery.
- [ ] PLG-014-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-014-D Execute oracle: labeled fixture precision/recall and review before commit; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-015 AI Object Detection
- [ ] PLG-015-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for detect and geolocate objects.
- [ ] PLG-015-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: detect and geolocate objects.
- [ ] PLG-015-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-015-D Execute oracle: holdout precision/recall and pixel-to-world control; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-016 AI Land Classification
- [ ] PLG-016-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for classify land cover using validated labels.
- [ ] PLG-016-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: classify land cover using validated labels.
- [ ] PLG-016-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-016-D Execute oracle: confusion matrix and reproducible model version; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-017 AI Change Detection
- [ ] PLG-017-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for compare georegistered imagery and detect changes.
- [ ] PLG-017-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: compare georegistered imagery and detect changes.
- [ ] PLG-017-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-017-D Execute oracle: known changed/unchanged regions with uncertainty; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-018 AI Data Quality
- [ ] PLG-018-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for detect schema and spatial anomalies.
- [ ] PLG-018-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: detect schema and spatial anomalies.
- [ ] PLG-018-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-018-D Execute oracle: seeded anomalous rows and false-positive report; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-019 AI Predictive Analytics
- [ ] PLG-019-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for fit spatial predictive models with uncertainty.
- [ ] PLG-019-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: fit spatial predictive models with uncertainty.
- [ ] PLG-019-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-019-D Execute oracle: spatial holdout baseline and no leakage; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-020 AI Report Generator
- [ ] PLG-020-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for grounded summaries from scoped data.
- [ ] PLG-020-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: grounded summaries from scoped data.
- [ ] PLG-020-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-020-D Execute oracle: every numerical assertion traces to reproducible query; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 3 — No-code & Low-code
### PLG-021 Form Builder
- [ ] PLG-021-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for schema forms, validation and conditional fields.
- [ ] PLG-021-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: schema forms, validation and conditional fields.
- [ ] PLG-021-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-021-D Execute oracle: submit valid form and reject invalid server-side; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-022 Dashboard Builder
- [ ] PLG-022-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for map/chart widgets and scoped filters.
- [ ] PLG-022-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: map/chart widgets and scoped filters.
- [ ] PLG-022-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-022-D Execute oracle: cross-filter consistent aggregates and permissions; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-023 Visual Model Builder
- [ ] PLG-023-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for typed processing DAG, run and replay.
- [ ] PLG-023-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: typed processing DAG, run and replay.
- [ ] PLG-023-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-023-D Execute oracle: execute buffer-to-overlay graph with saved outputs; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-024 Workflow Automation
- [ ] PLG-024-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for event/schedule triggers and retry.
- [ ] PLG-024-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: event/schedule triggers and retry.
- [ ] PLG-024-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-024-D Execute oracle: duplicate event yields one business effect; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-025 App Builder
- [ ] PLG-025-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for compose declarative workspaces and publish versions.
- [ ] PLG-025-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: compose declarative workspaces and publish versions.
- [ ] PLG-025-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-025-D Execute oracle: publish and open a scoped application on supported hosts; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-026 Report Designer
- [ ] PLG-026-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for templates, map frames, tables and exports.
- [ ] PLG-026-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: templates, map frames, tables and exports.
- [ ] PLG-026-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-026-D Execute oracle: render paginated PDF with correct totals and scale; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-027 Business Rules Engine
- [ ] PLG-027-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for versioned predicates and actions.
- [ ] PLG-027-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: versioned predicates and actions.
- [ ] PLG-027-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-027-D Execute oracle: same input yields same decisions and audit; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-028 Approval Workflow
- [ ] PLG-028-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for state transitions, reviewer roles and separation.
- [ ] PLG-028-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: state transitions, reviewer roles and separation.
- [ ] PLG-028-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-028-D Execute oracle: reject self-approval and stale transitions; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-029 Integration Builder
- [ ] PLG-029-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for approved connectors and field mapping.
- [ ] PLG-029-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: approved connectors and field mapping.
- [ ] PLG-029-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-029-D Execute oracle: map sandbox payload, handle rate limit and retries; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-030 Plugin Creator
- [ ] PLG-030-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for generate manifest and validated contribution bundle.
- [ ] PLG-030-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: generate manifest and validated contribution bundle.
- [ ] PLG-030-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-030-D Execute oracle: build, sign, install generated sample; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 4 — Remote Sensing
### PLG-031 Sentinel Explorer
- [ ] PLG-031-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for search authorized Sentinel products and load bands.
- [ ] PLG-031-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: search authorized Sentinel products and load bands.
- [ ] PLG-031-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-031-D Execute oracle: known STAC fixture, attribution and cloud filter; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-032 Landsat Explorer
- [ ] PLG-032-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for search Landsat scenes and bands.
- [ ] PLG-032-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: search Landsat scenes and bands.
- [ ] PLG-032-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-032-D Execute oracle: known scene metadata and georegistration; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-033 Vegetation Index
- [ ] PLG-033-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for NDVI, NDRE, EVI and masks.
- [ ] PLG-033-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: NDVI, NDRE, EVI and masks.
- [ ] PLG-033-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-033-D Execute oracle: pixel reference formulas with nodata and denominator guards; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-034 Water Index
- [ ] PLG-034-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for NDWI/MNDWI and thresholds.
- [ ] PLG-034-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: NDWI/MNDWI and thresholds.
- [ ] PLG-034-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-034-D Execute oracle: reference raster values and masks; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-035 Land Cover Analysis
- [ ] PLG-035-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for classify and summarize land cover.
- [ ] PLG-035-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: classify and summarize land cover.
- [ ] PLG-035-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-035-D Execute oracle: labeled reference area totals and class mapping; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-036 Time Series Analysis
- [ ] PLG-036-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for align timestamps, aggregate and detect trends.
- [ ] PLG-036-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: align timestamps, aggregate and detect trends.
- [ ] PLG-036-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-036-D Execute oracle: known seasonal series and missing-date handling; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-037 Raster Calculator
- [ ] PLG-037-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for bounded raster expressions and alignment.
- [ ] PLG-037-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: bounded raster expressions and alignment.
- [ ] PLG-037-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-037-D Execute oracle: pixel oracle and explicit resampling; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-038 Thermal Analysis
- [ ] PLG-038-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for calibrated surface temperature workflow.
- [ ] PLG-038-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: calibrated surface temperature workflow.
- [ ] PLG-038-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-038-D Execute oracle: reference calibration and units with QA masks; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-039 SAR Processing
- [ ] PLG-039-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for calibrate, filter and terrain-correct supported SAR.
- [ ] PLG-039-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: calibrate, filter and terrain-correct supported SAR.
- [ ] PLG-039-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-039-D Execute oracle: reference calibrated raster and provenance; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-040 Hyperspectral Analysis
- [ ] PLG-040-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for spectral signatures and band analysis.
- [ ] PLG-040-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: spectral signatures and band analysis.
- [ ] PLG-040-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-040-D Execute oracle: known spectral library and similarity reference; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 5 — CAD BIM Engineering
### PLG-041 OpenVietCAD Pro
- [ ] PLG-041-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for advanced DXF editing and preservation.
- [ ] PLG-041-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: advanced DXF editing and preservation.
- [ ] PLG-041-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-041-D Execute oracle: round-trip handles, layers, units and supported entities; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-042 BIM Viewer
- [ ] PLG-042-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for IFC model hierarchy and selection.
- [ ] PLG-042-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: IFC model hierarchy and selection.
- [ ] PLG-042-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-042-D Execute oracle: known IFC object GUID and coordinates; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-043 BIM-GIS Integration
- [ ] PLG-043-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for georeference IFC objects and link GIS.
- [ ] PLG-043-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: georeference IFC objects and link GIS.
- [ ] PLG-043-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-043-D Execute oracle: known transform control and ID mapping; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-044 Civil Engineering
- [ ] PLG-044-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for alignment and infrastructure design review.
- [ ] PLG-044-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: alignment and infrastructure design review.
- [ ] PLG-044-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-044-D Execute oracle: reference geometry and dimensional checks; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-045 Road Design
- [ ] PLG-045-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for horizontal/vertical alignment and stationing.
- [ ] PLG-045-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: horizontal/vertical alignment and stationing.
- [ ] PLG-045-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-045-D Execute oracle: known chainage and curves; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-046 Pipe Network Design
- [ ] PLG-046-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for network sizing and hydraulic input validation.
- [ ] PLG-046-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: network sizing and hydraulic input validation.
- [ ] PLG-046-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-046-D Execute oracle: known pipe connectivity and hydraulic reference case; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-047 Cross Section
- [ ] PLG-047-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for sample terrain and produce profiles.
- [ ] PLG-047-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: sample terrain and produce profiles.
- [ ] PLG-047-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-047-D Execute oracle: analytic surface elevation and station oracle; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-048 Earthwork Calculator
- [ ] PLG-048-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for cut/fill between surfaces.
- [ ] PLG-048-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: cut/fill between surfaces.
- [ ] PLG-048-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-048-D Execute oracle: analytic prism volume with units; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-049 Construction Quantity
- [ ] PLG-049-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for measure quantities by object and unit.
- [ ] PLG-049-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: measure quantities by object and unit.
- [ ] PLG-049-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-049-D Execute oracle: known bill of quantities reconciles to source; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-050 As-Built Manager
- [ ] PLG-050-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for version and compare as-built drawings.
- [ ] PLG-050-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: version and compare as-built drawings.
- [ ] PLG-050-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-050-D Execute oracle: approved revision diff and rollback history; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 6 — 3D LiDAR Drone
### PLG-051 Digital Twin
- [ ] PLG-051-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for link asset IDs to time-varying 3D state.
- [ ] PLG-051-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: link asset IDs to time-varying 3D state.
- [ ] PLG-051-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-051-D Execute oracle: known event updates correct asset with stale-state warning; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-052 Point Cloud Studio
- [ ] PLG-052-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for stream, clip and measure point clouds.
- [ ] PLG-052-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: stream, clip and measure point clouds.
- [ ] PLG-052-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-052-D Execute oracle: bounded LOD memory and known distance; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-053 LiDAR Classification
- [ ] PLG-053-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for classify supported point classes.
- [ ] PLG-053-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: classify supported point classes.
- [ ] PLG-053-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-053-D Execute oracle: labeled holdout metrics and class export; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-054 Drone Mapping
- [ ] PLG-054-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for ingest georeferenced orthomosaic and footprints.
- [ ] PLG-054-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: ingest georeferenced orthomosaic and footprints.
- [ ] PLG-054-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-054-D Execute oracle: control-point error and licensed output; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-055 Photogrammetry
- [ ] PLG-055-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for reconstruct from calibrated photos.
- [ ] PLG-055-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: reconstruct from calibrated photos.
- [ ] PLG-055-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-055-D Execute oracle: known GCP residual and reconstruction coverage; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-056 3D City Builder
- [ ] PLG-056-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for assemble terrain and building models.
- [ ] PLG-056-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: assemble terrain and building models.
- [ ] PLG-056-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-056-D Execute oracle: known heights and coordinates across LOD; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-057 Terrain Analysis
- [ ] PLG-057-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for slope, aspect, contours and drainage.
- [ ] PLG-057-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: slope, aspect, contours and drainage.
- [ ] PLG-057-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-057-D Execute oracle: analytic DEM reference values; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-058 Viewshed Analysis
- [ ] PLG-058-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for line-of-sight on elevation model.
- [ ] PLG-058-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: line-of-sight on elevation model.
- [ ] PLG-058-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-058-D Execute oracle: known obstruction and curvature assumptions; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-059 Solar Radiation
- [ ] PLG-059-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for estimate terrain solar exposure.
- [ ] PLG-059-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: estimate terrain solar exposure.
- [ ] PLG-059-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-059-D Execute oracle: reference time/location and unit benchmark; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-060 Underground GIS
- [ ] PLG-060-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for 3D utility depths and conflict visualization.
- [ ] PLG-060-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: 3D utility depths and conflict visualization.
- [ ] PLG-060-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-060-D Execute oracle: known crossing, depth and clearance test; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 7 — Agriculture
### PLG-061 AgriGIS
- [ ] PLG-061-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for farm boundaries, crop lots, operations and dashboard.
- [ ] PLG-061-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: farm boundaries, crop lots, operations and dashboard.
- [ ] PLG-061-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-061-D Execute oracle: farm season rollup reconciles to field records; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-062 Plantation Manager
- [ ] PLG-062-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for planting cohorts, stands and lifecycle.
- [ ] PLG-062-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: planting cohorts, stands and lifecycle.
- [ ] PLG-062-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-062-D Execute oracle: known stand counts and replacement history; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-063 Crop Monitoring
- [ ] PLG-063-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for field observations and vegetation trend.
- [ ] PLG-063-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: field observations and vegetation trend.
- [ ] PLG-063-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-063-D Execute oracle: time series linked to correct lot and season; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-064 Disease Mapping
- [ ] PLG-064-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for observations, quarantine zones and spread history.
- [ ] PLG-064-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: observations, quarantine zones and spread history.
- [ ] PLG-064-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-064-D Execute oracle: seeded outbreak map and role-restricted updates; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-065 Irrigation Manager
- [ ] PLG-065-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for zones, valves, pumps and irrigation schedules.
- [ ] PLG-065-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: zones, valves, pumps and irrigation schedules.
- [ ] PLG-065-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-065-D Execute oracle: water allocation and schedule conflict validation; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-066 Fertilizer Planner
- [ ] PLG-066-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for nutrient targets and application records.
- [ ] PLG-066-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: nutrient targets and application records.
- [ ] PLG-066-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-066-D Execute oracle: unit-balanced nutrient plan and approval; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-067 Harvest Planner
- [ ] PLG-067-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for cohort-based forecast and actual harvest.
- [ ] PLG-067-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: cohort-based forecast and actual harvest.
- [ ] PLG-067-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-067-D Execute oracle: forecast reconciles to cohorts and actuals; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-068 Soil Analysis
- [ ] PLG-068-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for sample locations and nutrient maps.
- [ ] PLG-068-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: sample locations and nutrient maps.
- [ ] PLG-068-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-068-D Execute oracle: sample units and interpolation provenance; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-069 Farm Machinery
- [ ] PLG-069-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for equipment and field operation coverage.
- [ ] PLG-069-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: equipment and field operation coverage.
- [ ] PLG-069-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-069-D Execute oracle: track-to-lot overlap and maintenance record; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-070 Agricultural Traceability
- [ ] PLG-070-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for lot-to-batch chain and recall lookup.
- [ ] PLG-070-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: lot-to-batch chain and recall lookup.
- [ ] PLG-070-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-070-D Execute oracle: trace synthetic carton back to harvest lot; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 8 — Infrastructure
### PLG-071 WaterGIS
- [ ] PLG-071-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for water network assets, valves and isolation analysis.
- [ ] PLG-071-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: water network assets, valves and isolation analysis.
- [ ] PLG-071-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-071-D Execute oracle: known connected network and isolated segment; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-072 PowerGIS
- [ ] PLG-072-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for electrical asset topology and outage mapping.
- [ ] PLG-072-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: electrical asset topology and outage mapping.
- [ ] PLG-072-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-072-D Execute oracle: known feeder connectivity, no live switching; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-073 TelecomGIS
- [ ] PLG-073-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for fiber routes, splices and capacity.
- [ ] PLG-073-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: fiber routes, splices and capacity.
- [ ] PLG-073-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-073-D Execute oracle: trace known fiber path and spare capacity; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-074 DrainageGIS
- [ ] PLG-074-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for drainage assets and catchments.
- [ ] PLG-074-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: drainage assets and catchments.
- [ ] PLG-074-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-074-D Execute oracle: known flow connectivity and outlet; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-075 Gas Network GIS
- [ ] PLG-075-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for pipe assets and inspection map.
- [ ] PLG-075-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: pipe assets and inspection map.
- [ ] PLG-075-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-075-D Execute oracle: known segment trace; no live control; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-076 Street Lighting
- [ ] PLG-076-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for lighting assets and maintenance.
- [ ] PLG-076-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: lighting assets and maintenance.
- [ ] PLG-076-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-076-D Execute oracle: fixture inventory and fault work order; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-077 Asset Management
- [ ] PLG-077-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for asset identity, lifecycle and inspections.
- [ ] PLG-077-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: asset identity, lifecycle and inspections.
- [ ] PLG-077-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-077-D Execute oracle: asset history and authorized transfer; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-078 Maintenance Manager
- [ ] PLG-078-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for work orders, priorities and recurring plans.
- [ ] PLG-078-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: work orders, priorities and recurring plans.
- [ ] PLG-078-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-078-D Execute oracle: complete job and preserve maintenance audit; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-079 IoT Monitoring
- [ ] PLG-079-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for authorized sensor feeds and alarms.
- [ ] PLG-079-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: authorized sensor feeds and alarms.
- [ ] PLG-079-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-079-D Execute oracle: duplicate/out-of-order telemetry and stale indicator; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-080 SCADA Integration
- [ ] PLG-080-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for read-only industrial telemetry gateway.
- [ ] PLG-080-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: read-only industrial telemetry gateway.
- [ ] PLG-080-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-080-D Execute oracle: simulator alarms and zero write commands; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 9 — Land Cadastral Planning
### PLG-081 LandGIS
- [ ] PLG-081-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for parcel inventory and authorized records.
- [ ] PLG-081-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: parcel inventory and authorized records.
- [ ] PLG-081-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-081-D Execute oracle: parcel query/export with PII role enforcement; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-082 Cadastral Mapping
- [ ] PLG-082-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for parcel map layout and topology checks.
- [ ] PLG-082-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: parcel map layout and topology checks.
- [ ] PLG-082-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-082-D Execute oracle: reference parcel topology, scale and legend; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-083 Parcel Editor
- [ ] PLG-083-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for split, merge and lineage.
- [ ] PLG-083-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: split, merge and lineage.
- [ ] PLG-083-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-083-D Execute oracle: area conservation within tolerance and revision audit; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-084 Land Registry
- [ ] PLG-084-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for document linkage and review lifecycle.
- [ ] PLG-084-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: document linkage and review lifecycle.
- [ ] PLG-084-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-084-D Execute oracle: restricted document access and approved record history; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-085 PlanningGIS
- [ ] PLG-085-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for planning layers and overlay queries.
- [ ] PLG-085-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: planning layers and overlay queries.
- [ ] PLG-085-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-085-D Execute oracle: known parcel-zone overlay and effective dates; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-086 Land Valuation
- [ ] PLG-086-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for scenario valuation using comparable inputs.
- [ ] PLG-086-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: scenario valuation using comparable inputs.
- [ ] PLG-086-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-086-D Execute oracle: reproducible calculation and uncertainty disclosure; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-087 Land Clearance
- [ ] PLG-087-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for affected parcels and case tracking.
- [ ] PLG-087-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: affected parcels and case tracking.
- [ ] PLG-087-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-087-D Execute oracle: affected area reconciliation and restricted records; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-088 Land Inventory
- [ ] PLG-088-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for land use summaries by geography.
- [ ] PLG-088-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: land use summaries by geography.
- [ ] PLG-088-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-088-D Execute oracle: known class totals reconcile to area; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-089 Land Change Monitoring
- [ ] PLG-089-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for temporal parcel/land use comparison.
- [ ] PLG-089-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: temporal parcel/land use comparison.
- [ ] PLG-089-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-089-D Execute oracle: known changed parcel and date provenance; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-090 Zoning Compliance
- [ ] PLG-090-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for rule-based scenario compliance checks.
- [ ] PLG-090-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: rule-based scenario compliance checks.
- [ ] PLG-090-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-090-D Execute oracle: versioned rule fixture and nonbinding result; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 10 — Smart City Government
### PLG-091 House Numbering
- [ ] PLG-091-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for reuse existing address proposal and approval.
- [ ] PLG-091-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: reuse existing address proposal and approval.
- [ ] PLG-091-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-091-D Execute oracle: existing approved reference workflow without duplicate IDs; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-092 Address Registry
- [ ] PLG-092-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for standardized address records and lookup.
- [ ] PLG-092-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: standardized address records and lookup.
- [ ] PLG-092-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-092-D Execute oracle: deduplicate known addresses and enforce publication scope; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-093 Administrative Boundary
- [ ] PLG-093-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for versioned boundary layers and hierarchy.
- [ ] PLG-093-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: versioned boundary layers and hierarchy.
- [ ] PLG-093-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-093-D Execute oracle: historical boundary fixture and valid parent relationships; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-094 Population GIS
- [ ] PLG-094-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for aggregate demographic layers.
- [ ] PLG-094-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: aggregate demographic layers.
- [ ] PLG-094-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-094-D Execute oracle: small-cell suppression and totals reconciliation; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-095 Smart City Dashboard
- [ ] PLG-095-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for scoped city indicators and alerts.
- [ ] PLG-095-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: scoped city indicators and alerts.
- [ ] PLG-095-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-095-D Execute oracle: known indicator totals with timestamps; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-096 Public Asset Manager
- [ ] PLG-096-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for public assets and inspection history.
- [ ] PLG-096-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: public assets and inspection history.
- [ ] PLG-096-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-096-D Execute oracle: inventory reconciliation and access controls; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-097 Citizen Reporting
- [ ] PLG-097-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for geolocated issue intake and assignment.
- [ ] PLG-097-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: geolocated issue intake and assignment.
- [ ] PLG-097-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-097-D Execute oracle: submit synthetic issue and track redacted status; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-098 Public Service GIS
- [ ] PLG-098-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for service coverage and accessibility.
- [ ] PLG-098-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: service coverage and accessibility.
- [ ] PLG-098-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-098-D Execute oracle: known service catchment and routing assumptions; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-099 Urban Planning 3D
- [ ] PLG-099-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for 3D planning scenario visualization.
- [ ] PLG-099-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: 3D planning scenario visualization.
- [ ] PLG-099-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-099-D Execute oracle: known height envelope and georeferencing; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-100 Waste Management
- [ ] PLG-100-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for collection routes, points and service records.
- [ ] PLG-100-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: collection routes, points and service records.
- [ ] PLG-100-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-100-D Execute oracle: known service route and missed stop report; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 11 — Environment
### PLG-101 ForestGIS
- [ ] PLG-101-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for forest inventory and cover monitoring.
- [ ] PLG-101-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: forest inventory and cover monitoring.
- [ ] PLG-101-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-101-D Execute oracle: known forest area and change reconciliation; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-102 Carbon Mapping
- [ ] PLG-102-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for estimate stocks from documented coefficients.
- [ ] PLG-102-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: estimate stocks from documented coefficients.
- [ ] PLG-102-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-102-D Execute oracle: reference arithmetic and uncertainty range; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-103 Biodiversity GIS
- [ ] PLG-103-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for species observations and restricted locations.
- [ ] PLG-103-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: species observations and restricted locations.
- [ ] PLG-103-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-103-D Execute oracle: sensitive coordinates hidden by role; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-104 Air Quality
- [ ] PLG-104-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for sensor maps and temporal aggregates.
- [ ] PLG-104-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: sensor maps and temporal aggregates.
- [ ] PLG-104-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-104-D Execute oracle: known units, timestamps and missing data; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-105 Water Quality
- [ ] PLG-105-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for sampling and indicator trends.
- [ ] PLG-105-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: sampling and indicator trends.
- [ ] PLG-105-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-105-D Execute oracle: reference units and threshold configuration; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-106 Flood Simulation
- [ ] PLG-106-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for bounded calibrated inundation model.
- [ ] PLG-106-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: bounded calibrated inundation model.
- [ ] PLG-106-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-106-D Execute oracle: independent benchmark extent/depth and model limits; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-107 Landslide Risk
- [ ] PLG-107-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for susceptibility mapping.
- [ ] PLG-107-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: susceptibility mapping.
- [ ] PLG-107-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-107-D Execute oracle: holdout benchmark and non-deterministic risk disclaimer; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-108 Wildfire Monitoring
- [ ] PLG-108-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for authorized fire observations and alerts.
- [ ] PLG-108-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: authorized fire observations and alerts.
- [ ] PLG-108-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-108-D Execute oracle: historical fixture and freshness/provenance; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-109 MiningGIS
- [ ] PLG-109-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for authorized concession and extraction footprints.
- [ ] PLG-109-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: authorized concession and extraction footprints.
- [ ] PLG-109-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-109-D Execute oracle: known boundary overlap and area calculation; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-110 Climate Risk
- [ ] PLG-110-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for scenario exposure mapping.
- [ ] PLG-110-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: scenario exposure mapping.
- [ ] PLG-110-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-110-D Execute oracle: documented scenario, period and uncertainty; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 12 — Transport Commerce
### PLG-111 LogisticsGIS
- [ ] PLG-111-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for shipment locations and network visibility.
- [ ] PLG-111-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: shipment locations and network visibility.
- [ ] PLG-111-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-111-D Execute oracle: synthetic shipment status and scoped map; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-112 Fleet Management
- [ ] PLG-112-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for authorized vehicle tracks and status.
- [ ] PLG-112-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: authorized vehicle tracks and status.
- [ ] PLG-112-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-112-D Execute oracle: out-of-order GPS and retention enforcement; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-113 Route Optimization
- [ ] PLG-113-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for capacity/time-window routing.
- [ ] PLG-113-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: capacity/time-window routing.
- [ ] PLG-113-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-113-D Execute oracle: known feasible benchmark and objective value; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-114 Traffic Analysis
- [ ] PLG-114-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for aggregate traffic flows and congestion.
- [ ] PLG-114-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: aggregate traffic flows and congestion.
- [ ] PLG-114-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-114-D Execute oracle: known temporal aggregates and units; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-115 Public Transport
- [ ] PLG-115-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for routes, stops and schedules.
- [ ] PLG-115-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: routes, stops and schedules.
- [ ] PLG-115-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-115-D Execute oracle: GTFS fixture valid trips and stop sequence; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-116 Delivery Management
- [ ] PLG-116-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for delivery stops and proof records.
- [ ] PLG-116-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: delivery stops and proof records.
- [ ] PLG-116-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-116-D Execute oracle: synthetic delivery completion and access rules; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-117 Retail Location
- [ ] PLG-117-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for catchment and site scoring.
- [ ] PLG-117-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: catchment and site scoring.
- [ ] PLG-117-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-117-D Execute oracle: known catchment and transparent weights; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-118 RealEstateGIS
- [ ] PLG-118-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for authorized property inventory and comparisons.
- [ ] PLG-118-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: authorized property inventory and comparisons.
- [ ] PLG-118-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-118-D Execute oracle: filter/compare with source dates and private fields; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-119 Market Intelligence
- [ ] PLG-119-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for aggregated spatial market indicators.
- [ ] PLG-119-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: aggregated spatial market indicators.
- [ ] PLG-119-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-119-D Execute oracle: reproducible aggregates with privacy threshold; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-120 TourismGIS
- [ ] PLG-120-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for points of interest and itineraries.
- [ ] PLG-120-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: points of interest and itineraries.
- [ ] PLG-120-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-120-D Execute oracle: known route and licensed POI attribution; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 13 — Mobile Survey Field
### PLG-121 SurveyGIS
- [ ] PLG-121-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for field survey forms, features and review.
- [ ] PLG-121-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: field survey forms, features and review.
- [ ] PLG-121-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (M; W/D management).
- [ ] PLG-121-D Execute oracle: offline capture, sync and conflict resolution; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-122 GNSS Professional
- [ ] PLG-122-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for supported RTK/NMEA/NTRIP adapters.
- [ ] PLG-122-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: supported RTK/NMEA/NTRIP adapters.
- [ ] PLG-122-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (M; W/D management).
- [ ] PLG-122-D Execute oracle: real device fix metadata and independent control points; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-123 Offline Field Mapping
- [ ] PLG-123-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for licensed scoped offline packages.
- [ ] PLG-123-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: licensed scoped offline packages.
- [ ] PLG-123-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (M; W/D management).
- [ ] PLG-123-D Execute oracle: airplane-mode edit and replay without duplicate; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-124 Field Task Manager
- [ ] PLG-124-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for assign field tasks with map context.
- [ ] PLG-124-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: assign field tasks with map context.
- [ ] PLG-124-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (M; W/D management).
- [ ] PLG-124-D Execute oracle: task state and permission across devices; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-125 QR Asset Tracking
- [ ] PLG-125-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for scan asset code and scoped lookup.
- [ ] PLG-125-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: scan asset code and scoped lookup.
- [ ] PLG-125-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (M; W/D management).
- [ ] PLG-125-D Execute oracle: duplicate/unknown QR handling and no token disclosure; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-126 Photo Mapping
- [ ] PLG-126-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for geotag photos with consent.
- [ ] PLG-126-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: geotag photos with consent.
- [ ] PLG-126-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (M; W/D management).
- [ ] PLG-126-D Execute oracle: capture and sync with configurable metadata stripping; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-127 Field Inspection
- [ ] PLG-127-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for inspection checklists and evidence.
- [ ] PLG-127-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: inspection checklists and evidence.
- [ ] PLG-127-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (M; W/D management).
- [ ] PLG-127-D Execute oracle: signed-off inspection history and export; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-128 Workforce Tracking
- [ ] PLG-128-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for consented work-session location collection.
- [ ] PLG-128-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: consented work-session location collection.
- [ ] PLG-128-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (M; W/D management).
- [ ] PLG-128-D Execute oracle: stop/revoke collection and retention verified; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-129 Mobile Data Collector
- [ ] PLG-129-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for schema-driven offline data collection.
- [ ] PLG-129-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: schema-driven offline data collection.
- [ ] PLG-129-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (M; W/D management).
- [ ] PLG-129-D Execute oracle: valid submission and conflict replay after restart; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-130 AR Survey
- [ ] PLG-130-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for device-supported AR measurements.
- [ ] PLG-130-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: device-supported AR measurements.
- [ ] PLG-130-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (M; W/D management).
- [ ] PLG-130-D Execute oracle: real device calibration, uncertainty and fallback; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 14 — Enterprise Integration
### PLG-131 QGIS Connector
- [ ] PLG-131-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for reuse bidirectional sync and plugin registry.
- [ ] PLG-131-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: reuse bidirectional sync and plugin registry.
- [ ] PLG-131-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-131-D Execute oracle: revision conflict and round-trip geometry; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-132 ArcGIS Connector
- [ ] PLG-132-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for authorized service adapter.
- [ ] PLG-132-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: authorized service adapter.
- [ ] PLG-132-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-132-D Execute oracle: supported fixture import and explicit capability; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-133 MapInfo Connector
- [ ] PLG-133-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for TAB/MIF data interchange.
- [ ] PLG-133-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: TAB/MIF data interchange.
- [ ] PLG-133-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-133-D Execute oracle: schema/CRS/geometry round-trip for supported subset; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-134 AutoCAD Connector
- [ ] PLG-134-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for authorized CAD interchange.
- [ ] PLG-134-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: authorized CAD interchange.
- [ ] PLG-134-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-134-D Execute oracle: DXF entity mapping and loss report; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-135 ERP Connector
- [ ] PLG-135-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for authorized mapping of business records.
- [ ] PLG-135-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: authorized mapping of business records.
- [ ] PLG-135-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-135-D Execute oracle: sandbox idempotent sync and scoped credentials; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-136 PostGIS Manager
- [ ] PLG-136-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for scoped source administration via broker.
- [ ] PLG-136-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: scoped source administration via broker.
- [ ] PLG-136-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-136-D Execute oracle: deny arbitrary SQL and cross-tenant reads; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-137 Cloud Storage Connector
- [ ] PLG-137-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for authorized object store access.
- [ ] PLG-137-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: authorized object store access.
- [ ] PLG-137-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-137-D Execute oracle: scoped signed URLs and expiry test; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-138 OGC Service Manager
- [ ] PLG-138-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for publish/consume qualified OGC services.
- [ ] PLG-138-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: publish/consume qualified OGC services.
- [ ] PLG-138-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-138-D Execute oracle: protocol fixture and authorization enforcement; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-139 Realtime Data Stream
- [ ] PLG-139-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for event ingestion and replay.
- [ ] PLG-139-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: event ingestion and replay.
- [ ] PLG-139-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-139-D Execute oracle: duplicate/out-of-order and backpressure; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-140 Data Marketplace
- [ ] PLG-140-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for licensed dataset discovery and distribution.
- [ ] PLG-140-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: licensed dataset discovery and distribution.
- [ ] PLG-140-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M capability-gated).
- [ ] PLG-140-D Execute oracle: license grant, provenance and download permission; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.

## Wave 15 — Vietnam Professional
### PLG-141 VN-2000 Professional
- [ ] PLG-141-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for regional transforms using approved parameters.
- [ ] PLG-141-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: regional transforms using approved parameters.
- [ ] PLG-141-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-141-D Execute oracle: independent control points per enabled region; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-142 VietTools
- [ ] PLG-142-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for Vietnamese workflow command collection.
- [ ] PLG-142-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: Vietnamese workflow command collection.
- [ ] PLG-142-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-142-D Execute oracle: invoke existing commands without duplicate algorithms; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-143 VietMap Layout
- [ ] PLG-143-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for localized map templates.
- [ ] PLG-143-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: localized map templates.
- [ ] PLG-143-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-143-D Execute oracle: known scale, legend, font and export; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-144 VietLand Records
- [ ] PLG-144-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for localized authorized land record forms.
- [ ] PLG-144-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: localized authorized land record forms.
- [ ] PLG-144-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-144-D Execute oracle: required fields, review and PII policy; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-145 VietPlanning
- [ ] PLG-145-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for localized planning schemas and symbols.
- [ ] PLG-145-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: localized planning schemas and symbols.
- [ ] PLG-145-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-145-D Execute oracle: effective-date overlay and source provenance; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-146 VietAddress
- [ ] PLG-146-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for Vietnamese address normalization.
- [ ] PLG-146-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: Vietnamese address normalization.
- [ ] PLG-146-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-146-D Execute oracle: labeled historic/current address fixture; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-147 VietAdministrative
- [ ] PLG-147-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for effective-dated administrative hierarchy.
- [ ] PLG-147-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: effective-dated administrative hierarchy.
- [ ] PLG-147-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-147-D Execute oracle: known split/merge fixture with historical query; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-148 VietSurvey
- [ ] PLG-148-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for localized survey workflow and field exports.
- [ ] PLG-148-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: localized survey workflow and field exports.
- [ ] PLG-148-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-148-D Execute oracle: control-point report and interoperable output; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-149 VietMap Symbols
- [ ] PLG-149-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for licensed symbol catalog.
- [ ] PLG-149-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: licensed symbol catalog.
- [ ] PLG-149-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-149-D Execute oracle: render known legend and preserve attribution; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
### PLG-150 VietPublic GIS
- [ ] PLG-150-A Inventory/reuse map; define typed data, command contracts, permissions, dependency, fixture and domain limits for publish authorized public maps.
- [ ] PLG-150-B Implement actual backend/adapter, migrations if needed, validation, errors, persistence/output and cancellation: publish authorized public maps.
- [ ] PLG-150-C Implement compatible host contributions, lifecycle, accessibility/vi-en, docs, export and explicit unavailable states (W/D; M field subset).
- [ ] PLG-150-D Execute oracle: redaction, review and revoked publication; add tenant/permission, tamper, lifecycle, regression and profile tests with evidence.
## Integrated release
- [ ] G001 Resolve migration/SDK/manifest convergence on merged candidate; run full CI and independent review.
- [ ] G002 Execute 150 domain oracles plus profile/negative tests; document all open blockers.
- [ ] G003 Rehearse staging rollout, backup/restore, rollback, SBOM/license, security and performance gates.
- [ ] G004 Publish signed capability matrix and evidence; promote only explicitly authorized accepted profile.
