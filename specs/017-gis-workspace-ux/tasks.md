---
description: "Dependency-ordered implementation tasks for OpenVietGIS GIS Workspace UX"
---

# Tasks: GIS Workspace UX

**Input**: Design documents from `/specs/017-gis-workspace-ux/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Required by the specification for layout stability, selection synchronization, save-state honesty, responsive behavior, accessibility and deterministic visual regression.

## Phase 1: Dependency & Integration Gate

**Purpose**: Prevent implementation on a stale or concurrently owned R004 contract baseline.

- [ ] T001 Verify and record the exact accepted/converged parent R004 commit in `specs/017-gis-workspace-ux/quickstart.md`; STOP implementation if no accepted R004 baseline exists
- [ ] T002 Reconcile `specs/017-gis-workspace-ux/spec.md`, `plan.md`, and `contracts/` against the accepted parent R004 Map/Layer/selection/configuration/command contracts and document any required adjustment in `specs/017-gis-workspace-ux/research.md`
- [ ] T003 Rebase/transplant this child slice onto the accepted R004 baseline in the isolated implementation worktree and record the exact base in `specs/017-gis-workspace-ux/quickstart.md`
- [ ] T004 Determine whether R002-FE shared UI tokens/primitives are already merged; record reuse/ownership decision in `specs/017-gis-workspace-ux/research.md` and do not create a duplicate design system
- [ ] T005 Record existing R004 web unit, Playwright, performance and bundle baselines in `specs/017-gis-workspace-ux/quickstart.md`

**Checkpoint**: No runtime task below may start before T001–T005 are complete.

## Phase 2: Foundational Workspace Presentation State

**Purpose**: Add UI-only layout state without changing parent Map semantics.

- [ ] T006 [P] Add failing tests for `WorkspaceLayoutPreference.leftPanelCollapsed`, `rightPanelCollapsed`, `attributeTableCollapsed`, `leftPanelWidth`, `rightPanelWidth`, and `attributeTableHeight` from `specs/017-gis-workspace-ux/data-model.md` in `apps/web/src/features/map/workspace-layout.test.ts`
- [ ] T007 Implement bounded non-sensitive `WorkspaceLayoutPreference` persistence from `specs/017-gis-workspace-ux/data-model.md` in `apps/web/src/features/map/workspace-layout.ts`
- [ ] T008 [P] Implement the `WorkspaceResponsiveMode` desktop/tablet/mobile resolver from `specs/017-gis-workspace-ux/data-model.md` with tests in `apps/web/src/features/map/responsive-mode.ts` and `apps/web/src/features/map/responsive-mode.test.ts`
- [ ] T009 [P] Add failing tests for user-facing Layer availability/status mapping that distinguishes capability, permission, stale, unsupported, unavailable and partial states in `apps/web/src/features/map/layer-view.test.ts`
- [ ] T010 Implement `LayerPanelItemView.layerId`, `name`, `visible`, `active`, `order`, `availability`, and command availability/reason from `specs/017-gis-workspace-ux/data-model.md` in `apps/web/src/features/map/layer-view.ts`
- [ ] T011 Add the stable panel composition container in `apps/web/src/features/map/WorkspaceLayout.tsx` so sibling panel changes do not own/recreate the Map engine
- [ ] T012 Add desktop/tablet/mobile grid, splitter, panel and deterministic local-map chrome styles in `apps/web/src/features/map/map.css`
- [ ] T013 [P] Add accessible panel-collapse/splitter primitives (including keyboard alternatives) in `apps/web/src/ui/primitives/ResizablePanel.tsx` or the accepted shared UI primitive path
- [ ] T014 Integrate the accepted shared UI tokens/primitives into map workspace chrome without changing Map domain state in `apps/web/src/features/map/MapWorkspace.tsx`

**Checkpoint**: Layout state is local-only, tested, and Map engine ownership is stable.

## Phase 3: User Story 1 — Open a professional GIS workspace without UI clutter (Priority: P1) MVP

**Goal**: Coherent Layer/Map/Inspector/Table workspace with human-readable context and explicit empty/loading states.

**Independent Test**: Open prepared and empty Projects at desktop width; identify each workspace region and inspect a feature without seeing primary UUID/roadmap clutter.

- [ ] T015 [P] [US1] Add failing desktop workspace/empty-state Playwright assertions in `tests/e2e/map-workspace.spec.ts`
- [ ] T016 [P] [US1] Add deterministic chrome screenshot cases for empty and populated workspace using local/grid or masked map pixels in `tests/e2e/map-workspace-visual.spec.ts`
- [ ] T017 [US1] Refactor `apps/web/src/features/map/MapWorkspace.tsx` to render through `WorkspaceLayout.tsx` while keeping the existing Map engine instance stable
- [ ] T018 [US1] Replace raw organization/project/revision context in primary chrome with human-readable project/mode/save labels in `apps/web/src/features/map/MapWorkspace.tsx`
- [ ] T019 [US1] Implement explicit empty/loading/unsupported/unavailable/stale/partial workspace states in `apps/web/src/features/map/MapWorkspace.tsx` and `apps/web/src/features/map/layer-view.ts`
- [ ] T020 [US1] Ensure primary map header/sidebar labels are Vietnamese-first and contain no roadmap IDs in `apps/web/src/features/map/MapWorkspace.tsx`, `LayerTree.tsx`, `AttributeTable.tsx`, and `FeatureInspector.tsx`

**Checkpoint**: US1 provides the usable workstation shell independently of Add Data or advanced technical details.

## Phase 4: User Story 2 — Organize layers and add data from where work happens (Priority: P1)

**Goal**: Layer management and Add Data entry point for existing catalog Layers, with optional R005 import handoff.

**Independent Test**: Add an existing Layer, visibility/order/active/zoom/remove it, confirm source remains intact, and verify duplicate/no-candidate states.

- [ ] T021 [P] [US2] Add failing unit tests for `AddDataCandidateView` authorized/addable/already-present behavior from `specs/017-gis-workspace-ux/data-model.md` in `apps/web/src/features/map/add-data.test.ts`
- [ ] T022 [P] [US2] Add failing browser coverage for add-existing-Layer, duplicate prevention, reorder, visibility, active Layer, Zoom to Layer and remove-reference in `tests/e2e/map-workspace.spec.ts`
- [ ] T023 [US2] Implement `AddDataCandidateView` mapping and catalog filtering in `apps/web/src/features/map/add-data.ts`
- [ ] T024 [US2] Implement the capability-aware Add Data dialog/handoff defined by `contracts/add-data-handoff.md` in `apps/web/src/features/map/AddDataDialog.tsx`
- [ ] T025 [US2] Refactor `apps/web/src/features/map/LayerTree.tsx` to expose active, visibility, order, Zoom to Layer, remove-reference, and Move Up/Move Down controls using stable Layer identities
- [ ] T026 [US2] If accepted R005 import route/capability exists on the implementation baseline, wire only the canonical handoff from `apps/web/src/features/map/AddDataDialog.tsx`; otherwise render the documented unavailable/omitted state without inventing import behavior
- [ ] T027 [US2] Register Add Data, Zoom to Layer, Move Up/Down, visibility and remove-reference commands through the accepted map command registry in `apps/web/src/features/map/commands.ts`

**Checkpoint**: Layer organization and Add Data work without requiring users to understand Source/Layer internals.

## Phase 5: User Story 3 — Inspect map and attributes as one synchronized workspace (Priority: P1)

**Goal**: Stable synchronized selection across Map, Table and Inspector while panels collapse/resize.

**Independent Test**: Select from map/table, page/collapse/resize, then confirm stable IDs/focus remain synchronized until authoritative context changes.

- [ ] T028 [P] [US3] Add failing unit tests covering `AccessibleMapSummary.project/map name`, `active Layer name`, `visible Layer count`, `selected feature count`, focused summary and state fields from `specs/017-gis-workspace-ux/data-model.md` in `apps/web/src/features/map/accessibility-summary.test.ts`
- [ ] T029 [P] [US3] Add browser coverage proving table collapse/resize does not reset camera or valid selection in `tests/e2e/map-workspace.spec.ts`
- [ ] T030 [US3] Implement synchronized `AccessibleMapSummary` in `apps/web/src/features/map/accessibility-summary.ts` and surface it near the named map region in `MapWorkspace.tsx`
- [ ] T031 [US3] Refactor `apps/web/src/features/map/AttributeTable.tsx` into the bottom panel contract while preserving parent bounded paging and stable selection IDs
- [ ] T032 [US3] Refactor `apps/web/src/features/map/FeatureInspector.tsx` into the right contextual panel while preserving typed null/empty/zero semantics
- [ ] T033 [US3] Ensure `apps/web/src/features/map/MapWorkspace.tsx` keeps active Layer/selection/focus across pure layout changes and invalidates only on parent R004 context/revision rules
- [ ] T034 [US3] Profile current bounded Attribute Table DOM size; only if accepted simultaneous rows exceed 100, implement measured windowing in `apps/web/src/features/map/AttributeTable.tsx` and record evidence in `specs/017-gis-workspace-ux/quickstart.md`

**Checkpoint**: Map/Table/Inspector remain one logical inspection workspace.

## Phase 6: User Story 4 — Understand local changes and save shared map state deliberately (Priority: P1)

**Goal**: Honest clean/dirty/saving/saved/conflict/failed/unresolved presentation and unsaved-navigation warning.

**Independent Test**: Shared config changes mark dirty; pan/zoom/selection/layout do not; successful save confirms only after authority; stale/unresolved outcomes remain honest.

- [ ] T035 [P] [US4] Add failing unit tests for every `SaveIndicatorState` value from `specs/017-gis-workspace-ux/data-model.md` and parent save outcomes in `apps/web/src/features/map/save-indicator.test.ts`
- [ ] T036 [P] [US4] Add Playwright scenarios for dirty-vs-transient state, leave warning, successful save, stale conflict and unresolved acknowledgement in `tests/e2e/map-workspace.spec.ts`
- [ ] T037 [US4] Implement user-facing save-state mapping in `apps/web/src/features/map/SaveStateIndicator.tsx` using `contracts/save-and-technical-state.md`
- [ ] T038 [US4] Integrate `SaveStateIndicator.tsx` with the accepted parent save state machine in `apps/web/src/features/map/MapWorkspace.tsx` without changing revision/CAS semantics
- [ ] T039 [US4] Add scoped unsaved-shared-change navigation protection in `apps/web/src/features/map/unsaved-navigation.ts` and integrate it with project/context navigation in `MapWorkspace.tsx`
- [ ] T040 [US4] Add deterministic dirty/saved/conflict screenshot states in `tests/e2e/map-workspace-visual.spec.ts`

**Checkpoint**: Users can distinguish personal exploration from shared Map changes and never see false save success.

## Phase 7: User Story 5 — Use advanced details without exposing them to everyone (Priority: P2)

**Goal**: Secondary authorized technical drawer for IDs/CRS/revisions/capabilities.

**Independent Test**: Ordinary chrome has no raw IDs; drawer shows permitted facts and clears on scope change.

- [ ] T041 [P] [US5] Add failing component tests for `TechnicalDetailsView` organization/project/map IDs, Layer/source IDs, revisions, CRS and capability fields from `specs/017-gis-workspace-ux/data-model.md` in `apps/web/src/features/map/technical-details.test.ts`
- [ ] T042 [US5] Implement authorized `TechnicalDetailsView` mapping in `apps/web/src/features/map/technical-details.ts`
- [ ] T043 [US5] Implement `TechnicalDetailsDrawer.tsx` with accessible open/close/focus return in `apps/web/src/features/map/TechnicalDetailsDrawer.tsx`
- [ ] T044 [US5] Clear drawer content before organization/project/active-Layer scope changes can expose stale technical metadata in `apps/web/src/features/map/MapWorkspace.tsx`
- [ ] T045 [US5] Add closed/open deterministic drawer screenshot coverage in `tests/e2e/map-workspace-visual.spec.ts`

**Checkpoint**: Technical support metadata is available without cluttering routine GIS work.

## Phase 8: User Story 6 — Smaller screens and keyboard accessibility (Priority: P2)

**Goal**: Desktop full layout, tablet collapse/switch, mobile basic viewer, non-drag controls and map text alternative.

**Independent Test**: 1440/1280/1024/768/414/375 widths plus keyboard-only interaction and reduced-motion checks.

- [ ] T046 [P] [US6] Add responsive browser matrix for 1440, 1280, 1024, 768, 414 and 375 px in `tests/e2e/map-workspace-responsive.spec.ts`
- [ ] T047 [P] [US6] Add keyboard/accessibility browser checks for named map region, focus visibility, Move Up/Down, panel controls, table semantics and drawer focus return in `tests/e2e/map-workspace-accessibility.spec.ts`
- [ ] T048 [US6] Implement desktop/tablet/mobile presentation from `WorkspaceResponsiveMode` in `apps/web/src/features/map/WorkspaceLayout.tsx` and `apps/web/src/features/map/map.css`
- [ ] T049 [US6] Keep map keyboard pan/zoom and visible navigation controls enabled through the MapEngine/MapLibre adapter in `apps/web/src/features/map/maplibre-engine.ts` without leaking renderer internals into business components
- [ ] T050 [US6] Add named map region + synchronized textual/table alternative in `apps/web/src/features/map/MapWorkspace.tsx` and `apps/web/src/features/map/accessibility-summary.ts`
- [ ] T051 [US6] Implement keyboard/non-drag layer reorder and accessible labels/states in `apps/web/src/features/map/LayerTree.tsx`
- [ ] T052 [US6] Implement keyboard-accessible panel resize/collapse behavior and reduced-motion handling in `apps/web/src/ui/primitives/ResizablePanel.tsx` and `apps/web/src/features/map/map.css`

**Checkpoint**: All six user stories are demonstrable without claiming full mobile desktop parity.

## Phase 9: Polish & Cross-Cutting Validation

- [ ] T053 [P] Compare final bundle/UI interaction metrics and parent R004 map-open/select-inspector benchmarks with T005 baseline; record results in `specs/017-gis-workspace-ux/quickstart.md`
- [ ] T054 Run every quickstart scenario, including Add Data fallback, layout stability, technical drawer, responsive widths and accessibility, and record exact browser/runtime/commit evidence in `specs/017-gis-workspace-ux/quickstart.md`
- [ ] T055 Run all existing parent R004 unit/browser/performance tests before new UX suites; record any regression resolution in `specs/017-gis-workspace-ux/quickstart.md`
- [ ] T056 Run web test/lint/typecheck/build, all new Playwright suites and full repository validation; record final evidence in `specs/017-gis-workspace-ux/quickstart.md`
- [ ] T057 Perform independent code review and Spec Kit convergence against `specs/017-gis-workspace-ux/spec.md`; keep parent R004 human/external acceptance obligations honest in `specs/017-gis-workspace-ux/quickstart.md`

## Dependencies & Execution Order

- T001–T005 are a hard gate. No implementation task may bypass them.
- Phase 2 blocks all user stories.
- US1 establishes WorkspaceLayout and should integrate before US2–US6.
- After US1, US2 (Layer/Add Data), US4 (Save indicator) and US5 (Technical drawer) can proceed largely in parallel on distinct files.
- US3 depends on the stable layout but not on US2/US5.
- US6 integrates all panels and should finalize after core interactions from US2–US5 exist.
- Phase 9 requires all selected stories and existing parent R004 regression suites.

## Parallel Opportunities

- T006/T008/T009/T013 can be prepared in parallel after the dependency gate.
- T015/T016 can run in parallel with T017–T020 implementation.
- After US1, US2, US4 and US5 can be assigned to separate workers with MapWorkspace integration coordinated by one owner.
- T028/T029 can run in parallel.
- T046/T047 can be prepared in parallel before final responsive integration.

## Implementation Strategy

**MVP**: Dependency Gate + Foundation + US1 + US2 + US3 + US4. This delivers a coherent professional map workspace with Layer management, synchronized inspection and honest save state.

US5 and US6 then add technical support metadata, responsive/adaptive behavior and the full accessibility layer.

This tasks file authorizes no runtime work until the parent R004 dependency gate is actually satisfied.
