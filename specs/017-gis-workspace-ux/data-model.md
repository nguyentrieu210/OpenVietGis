# Data Model: GIS Workspace UX

This slice adds no new server persistence model. It defines client-side presentation state layered over parent R004 entities.

## WorkspaceLayoutPreference

Non-sensitive personal browser preference.

- `leftPanelCollapsed`: boolean
- `rightPanelCollapsed`: boolean
- `attributeTableCollapsed`: boolean
- `leftPanelWidth`: bounded CSS pixel value
- `rightPanelWidth`: bounded CSS pixel value
- `attributeTableHeight`: bounded CSS pixel value

### Rules

- Contains no tenant feature data, authorization state, tokens or Map configuration.
- Invalid/out-of-range stored values fall back to safe defaults.
- Layout preference failure cannot block Map access.
- Preference changes do not mark shared Map configuration dirty.

## WorkspaceResponsiveMode

- `desktop`: full panel composition
- `tablet`: collapsible/switchable side panels
- `mobile`: basic viewer/layers/feature details

### Rules

- Responsive mode changes presentation only; it does not change authority or parent Map state.
- Valid camera/selection survives mode changes unless parent R004 context itself changes.

## LayerPanelItemView

Presentation of a parent R004 Map Layer reference.

- stable `layerId`
- human-readable `name`
- `visible`
- `active`
- `order`
- user-facing `availability`
- applicable command availability/reason

### Rules

- Stable Layer ID is used internally but UUID is not the ordinary label.
- Permission absence and capability absence remain distinct.
- Remove action removes the Map reference only.

## AddDataCandidateView

- catalog Layer identity
- human-readable name
- already-on-map flag
- current addability/availability
- optional downstream import entry availability

### Rules

- Only currently authorized candidates are displayed.
- Existing Layer add does not duplicate/delete source data.
- Import entry is a handoff; no import state is duplicated here.

## SaveIndicatorState

Presentation over the parent R004 save machine:

- `clean`
- `dirty`
- `saving`
- `saved`
- `conflict`
- `failed`
- `unresolved`

### Rules

- Transient camera/selection/panel layout changes do not set `dirty`.
- Only authoritative save confirmation reaches `saved`.
- Low-level revision may be available in Technical Details; normal message uses “Phiên bản bản đồ”.

## TechnicalDetailsView

Read-only secondary presentation of currently permitted facts:

- organization/project/map IDs
- active Layer/source IDs
- configuration/content/schema revisions
- CRS facts
- capability/availability facts

### Rules

- Never widens authorization.
- Secrets/provider credentials are excluded.
- Drawer closes/clears on scope change before stale metadata can appear in a new context.

## AccessibleMapSummary

Textual context synchronized with the visible/active map state.

- project/map name
- active Layer name
- visible Layer count
- selected feature count
- optional focused feature summary
- current loading/partial/unavailable state

### Rules

- Must not claim information not supported by current authorized data.
- Complements, not replaces, the visual map.
