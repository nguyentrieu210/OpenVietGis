# Quickstart Validation: GIS Workspace UX

## Prerequisites

- Parent R004 exact accepted integration baseline identified and this child slice reconciled onto it.
- R002-FE shared shell/tokens available or explicit ownership agreed.
- Supported Node 26 runtime and frozen dependencies.
- R004 reference Project/Map/Layer fixtures and real browser acceptance environment.
- Optional external basemap disabled for deterministic baseline.

## Validation scenarios

### 1. Desktop workspace structure

Open a prepared Project at 1440×900 and 1280×900.

**Expected**: Layer panel, Map, Inspector and Attribute Table are understandable in one workspace; no primary UUID/roadmap-code clutter; no page-level horizontal overflow.

### 2. Empty Project

Open a Project with no Map Layer references.

**Expected**: Clear empty state with Add Data guidance; no fake feature/extent.

### 3. Add existing Layer

Use Add Data with catalog Layers both absent and already on the Map.

**Expected**: Only valid add candidates are actionable; duplicate behavior is explained; adding/removing a Map reference does not mutate catalog source data.

### 4. Layer controls without drag

For two or more Layers:
- toggle visibility;
- set active;
- Move Up/Move Down;
- Zoom to Layer;
- remove Map reference.

**Expected**: All actions work through keyboard-accessible controls; Layer order/visibility matches parent R004 behavior.

### 5. Map ↔ Table ↔ Inspector synchronization

Select from map, then table; change page; collapse/reopen table; resize panels.

**Expected**: Current valid selection/focus agrees across surfaces and panel layout changes do not reset Map camera/valid selection.

### 6. Save state

Change a shared Map property, pan/zoom/select, save, then trigger stale/failed/unresolved fixtures.

**Expected**: Only shared changes set dirty state; success appears only after authoritative confirmation; conflicts/unresolved outcomes remain honest.

### 7. Technical Details

Use ordinary workspace with drawer closed, then open it.

**Expected**: UUIDs/revisions are absent from primary chrome but available, scoped and labeled in the drawer. Scope change clears stale technical content.

### 8. Responsive modes

Test 1440, 1280, 1024, 768, 414 and 375 px widths.

**Expected**:
- desktop full layout;
- tablet collapsible/switchable panels;
- mobile basic Map/Layer/feature inspection;
- no accidental page-wide horizontal overflow.

### 9. Accessibility

- Keyboard-only controls including non-drag Layer reorder.
- Named Map region.
- Visible focus.
- Attribute Table/Inspector text alternative.
- Reduced-motion behavior.
- Dialog/drawer focus return.

### 10. Visual regression

Capture deterministic Empty Workspace, Workspace with data, Layer panel, Table open/closed, Technical drawer open/closed, Dirty and Saved states using local/grid map background or masked map pixels.

### 11. Parent regression

Run all existing R004 unit/browser/performance suites before accepting this UX slice.

**Expected**: No parent authorization, CRS, bounded read, selection, save/conflict or performance regression.

## Required commands

From the exact supported runtime:

- `pnpm --filter @openvietgis/web test`
- `pnpm --filter @openvietgis/web lint`
- `pnpm --filter @openvietgis/web typecheck`
- `pnpm --filter @openvietgis/web build`
- targeted and existing R004 Playwright suites
- full root validation before convergence

Record exact parent R004 commit, child commit, browser/runtime and fixture profile.
