# Implementation Plan: GIS Workspace UX

**Branch**: `spec/017-gis-workspace-ux` | **Date**: 2026-11-21 | **Spec**: [spec.md](spec.md)

**Input**: Child frontend slice of [R004](../../ROADMAP.md#r004), using the parent Map/Layer/selection/configuration/capability contracts. Planning may complete now; implementation is gated until an accepted R004 integration baseline is identified.

## Summary

Turn the parent R004 Map Workspace into a professional, information-dense GIS workstation without changing map-domain semantics. Keep a stable map canvas in the center; put Layer management left, Inspector right, a resizable/collapsible Attribute Table below and command controls above. Add a capability-aware “Add data” entry point for existing catalog Layers and downstream import handoff, explicit saved/unsaved state, a secondary technical details drawer, adaptive desktop/tablet/mobile presentation, non-drag alternatives, map text/table accessibility and deterministic screenshot coverage that does not depend on external map tiles.

## Technical Context

**Language/Version**: TypeScript 6; Node.js 26; React 19.

**Primary Dependencies**: Parent R004 baseline uses React 19, React Router 7.18.5, TanStack Query 5.107.1, MapLibre GL 6.18.0 behind the accepted MapEngine boundary, and `@openvietgis/client-sdk`. This child slice must use the R002-FE shared UI tokens/primitives if merged first. No new heavy UI framework is planned.

**Storage**: Parent R004 shared map configuration remains server-owned. Personal panel layout preferences may use non-sensitive browser storage; they must not contain tenant data, feature payloads, authorization snapshots or provider secrets.

**Testing**: Existing R004 unit tests for workspace/save/commands, TypeScript/ESLint/Vite, real Playwright map workspace/performance tests, new responsive/accessibility/browser scenarios, deterministic screenshot regression using the local/grid or normalized map surface.

**Target Platform**: Desktop reference workspace at 1280×900 and larger; adaptive tablet from 768–1279 px; mobile under 768 px limited to basic viewer/layer/feature inspection.

**Project Type**: Existing professional GIS web SPA.

**Performance Goals**: Panel collapse/resize/tab changes must not recreate the Map engine or reset camera/valid selection. UI-only interactions target p95 <100 ms after data is available. Attribute presentation remains bounded; if future row counts exceed 100 simultaneously rendered rows, use measured windowing rather than unbounded DOM. Preserve parent R004 p95 map-open/select-inspector budgets.

**Constraints**: Parent R004 semantics remain authoritative; no map-engine internals in business UI; no import semantics owned by this slice; local background remains usable offline from third-party tile services; WCAG 2.1 AA baseline for primary non-canvas controls; non-drag equivalents for layer reordering and panel-resize affordances; light theme; Vietnamese-first.

**Scale/Scope**: Layer Tree, Map canvas chrome, Inspector, Attribute Table container, Add Data dialog/handoff, save-state indicator, command integration, technical drawer and responsive panel layout. Editing/query/processing/cartography/CAD/offline remain outside scope.

## Dependency Gate

Implementation MUST NOT begin from the historical c5cc5a9 scaffold. Before implementation:

1. Parent R004 must have an accepted integration/convergence checkpoint with final Map/Layer/selection/configuration/command contracts.
2. This branch must be rebased/transplanted onto that accepted baseline without overwriting active R004 coordinator work.
3. Contract differences between this plan and accepted R004 must be reconciled in documentation before runtime changes.
4. If R002-FE has merged, reuse its AppShell/design tokens/primitives; otherwise coordinate shared-UI ownership before creating duplicate infrastructure.
5. R005 import capability is optional; Add Data must degrade to existing Layer selection when R005 is absent.

## Constitution Check

**Pre-design gate: PASS subject to Dependency Gate above.**

- **Platform-first/module isolation**: all UX is Core GIS workflow; no business module semantics.
- **Open data/interoperability**: Add Data does not redefine import/export.
- **Tenant isolation**: technical drawer and Layer/Add Data presentation remain bounded by current authorized data; hidden controls are not security.
- **Spatial correctness**: CRS and feature selection facts come only from accepted parent contracts.
- **Version/audit**: saved/unsaved UI mirrors parent map configuration revision semantics; no new revision domain.
- **Capability-driven architecture**: actions derive from accepted capabilities/permissions and command identities.
- **Web-native/engine-agnostic**: MapLibre remains behind MapEngine; panel/UI state does not persist renderer objects.
- **Workflow value**: optimizes complete open-layer-select-inspect-save workflow rather than adding unrelated tools.

**Post-design gate: PASS.** No Constitution deviation is requested.

## Project Structure

### Documentation (this feature)

```text
specs/017-gis-workspace-ux/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── workspace-layout.md
│   ├── add-data-handoff.md
│   ├── save-and-technical-state.md
│   └── accessibility-responsive.md
├── checklists/requirements.md
└── tasks.md
```

### Source Code (target after R004 reconciliation)

```text
apps/web/src/
├── ui/                              # reuse R002-FE shared tokens/primitives
├── app/shell/                       # reuse AppShell/CommandPalette
└── features/map/
    ├── MapWorkspace.tsx
    ├── WorkspaceLayout.tsx
    ├── LayerTree.tsx
    ├── AddDataDialog.tsx
    ├── FeatureInspector.tsx
    ├── AttributeTable.tsx
    ├── SaveStateIndicator.tsx
    ├── TechnicalDetailsDrawer.tsx
    ├── CommandBar.tsx
    ├── engine.ts
    ├── maplibre-engine.ts
    ├── workspace.ts
    └── map.css

tests/e2e/
├── map-workspace.spec.ts
├── map-workspace-responsive.spec.ts
├── map-workspace-accessibility.spec.ts
└── map-workspace-visual.spec.ts
```

**Structure Decision**: Extend the accepted R004 feature directory rather than creating a second map application. Shared product chrome/tokens come from R002-FE. Map engine lifetime is owned by a stable canvas component; panel layout is sibling UI state so resizing does not remount the engine.

## Delivery and Integration

1. Reconcile with accepted parent R004 exact commit and current command/capability contracts.
2. Reuse or establish shared UI tokens/primitives from R002-FE.
3. Lock a stable WorkspaceLayout around the existing MapWorkspace without changing map-domain behavior.
4. Improve LayerTree and AddDataDialog using current catalog/capability reads.
5. Integrate Inspector/Table panel lifecycle and keyboard/non-drag controls.
6. Add save-state indicator + leave-warning semantics against existing parent save state machine.
7. Add TechnicalDetailsDrawer as a secondary authorized view.
8. Add tablet/mobile presentation and map accessibility alternatives.
9. Run existing R004 functional/performance suites first, then new browser/accessibility/visual gates, full root validation and independent review.
10. Converge only after parent R004 human/open acceptance obligations remain honestly represented.

## Complexity Tracking

No Constitution violation. The only sequencing complexity is the explicit parent-R004 dependency gate; duplicating or bypassing that gate would be more complex and unsafe.
