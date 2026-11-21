# Feature Specification: GIS Workspace UX

**Feature Branch**: `spec/017-gis-workspace-ux`

**Created**: 2026-11-21

**Status**: Draft — specification/design only until parent R004 converges

**Input**: Child slice of [ROADMAP.md — R004 Map Workspace](../../ROADMAP.md#r004). The product owner approved a professional GIS workstation experience with a persistent application shell, left layer tree, central map canvas, right inspector, collapsible/resizable attribute table, top command area, capability-aware controls, clear unsaved/saved state, technical details hidden by default, responsive panel behavior, accessible map alternatives, and direct “Add data” entry points that hand off to available catalog/import capabilities. This slice refines user-facing UX and MUST reuse the parent R004 map/configuration/capability contracts rather than redefining them.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Open a professional GIS workspace without UI clutter (Priority: P1)

A GIS-capable user opens a Project Map and immediately understands the workspace: Layers on the left, map in the center, details on the right, attributes below, and commands above. Internal UUIDs, roadmap codes and low-level revision identifiers are not part of the primary visual hierarchy.

**Why this priority**: R004 becomes useful only when the map workspace is coherent enough for sustained professional work rather than a collection of technical screens.

**Independent Test**: Open a prepared project with multiple readable layers at a desktop reference width, identify each major workspace region, perform pan/zoom, choose a layer and inspect a feature without opening a separate technical page or reading an internal identifier.

**Acceptance Scenarios**:

1. **Given** an authorized Project Map with readable layers, **When** the user opens it, **Then** the Layer panel, map canvas, Inspector and Attribute Table are available in one workspace with the project name and current save state clearly visible.
2. **Given** the user has no need for implementation metadata, **When** the workspace loads, **Then** organization/project/source UUIDs, roadmap IDs and raw backend status codes are absent from the primary workspace.
3. **Given** an authorized Project has no map layers, **When** the workspace opens, **Then** the user sees an intentional empty state with clear actions rather than an empty unexplained canvas.
4. **Given** a map or layer is temporarily loading, empty, stale, unavailable, unsupported or partially displayed, **When** the user views the workspace, **Then** that state is distinguishable without raw internal error text.

---

### User Story 2 - Organize layers and add data from where work happens (Priority: P1)

A user can activate, show/hide, reorder, zoom to and remove Map Layer references from the Layer panel. From the same workspace they can add an existing authorized catalog Layer and, when an import capability is available, enter the import flow without first understanding Source/Layer internals.

**Why this priority**: Layer organization is the most frequent map-management task, and the current separation between catalog and map makes basic work unnecessarily technical.

**Independent Test**: In a project with multiple catalog layers, add an available layer, toggle visibility, reorder it, set it active, zoom to it and remove the map reference; verify the underlying catalog layer remains intact and duplicate add attempts are explained.

**Acceptance Scenarios**:

1. **Given** authorized catalog Layers not yet on the Map, **When** the user selects “Add data”, **Then** existing addable Layers are presented by human-readable name and current usable state.
2. **Given** all currently authorized Layers are already referenced by the Map, **When** the user opens “Add data”, **Then** the workspace explains that no existing Layer remains to add and offers only applicable next actions.
3. **Given** a Layer is already on the Map, **When** the user attempts to add it again, **Then** the UI prevents accidental duplication or explicitly explains the result before creating a duplicate reference.
4. **Given** a supported Map Layer, **When** the user toggles visibility, changes order, selects it as active, zooms to it or removes its Map reference, **Then** the workspace updates consistently without deleting the underlying catalog Layer/source.
5. **Given** the project has a supported import capability from another roadmap slice, **When** the user chooses Import from “Add data”, **Then** the workspace hands off to that flow and returns to the Map with an addable/imported result when available; this child slice does not redefine import validation or publication semantics.

---

### User Story 3 - Inspect map and attributes as one synchronized workspace (Priority: P1)

A user selects features from the map or Attribute Table, sees the same selection reflected across both surfaces, and inspects typed attributes and relevant CRS facts in the Inspector without losing map context.

**Why this priority**: The value of a GIS workspace is the reliable connection between geometry and underlying records.

**Independent Test**: Select features from the canvas and table, move to another table page, focus one selected feature, collapse/reopen the table and verify selection/focus remain coherent for the current valid layer/revision context.

**Acceptance Scenarios**:

1. **Given** a readable active Layer, **When** a feature is selected on the map, **Then** the same feature is represented as selected in the table and its details appear in the Inspector.
2. **Given** a row is selected in the table, **When** the feature has valid geometry, **Then** the map can highlight and optionally zoom to that feature.
3. **Given** the Attribute Table is resized or collapsed, **When** it is reopened, **Then** the current valid active Layer and selection remain understandable and the Map is not reset.
4. **Given** a feature has null, empty or zero values, **When** the Inspector displays attributes, **Then** those values remain distinguishable.
5. **Given** the active Layer or pinned source revision changes, **When** prior selection becomes invalid, **Then** stale selection is cleared or explicitly marked rather than silently mapped to another record.

---

### User Story 4 - Understand local changes and save shared map state deliberately (Priority: P1)

A Project editor can tell whether the map contains unsaved shared-configuration changes, explicitly save them, see confirmation of the committed map version, and receive a warning before navigating away with unsaved changes. Temporary navigation/selection remains local.

**Why this priority**: Shared map configuration and personal exploration must not be confused; silent autosave would create collaboration and audit problems.

**Independent Test**: Change layer order/visibility, observe unsaved state, pan and select without changing the shared-state indicator, save the configuration, verify a saved confirmation, then create another unsaved change and attempt to leave the workspace.

**Acceptance Scenarios**:

1. **Given** only transient pan/zoom/selection changes, **When** the user continues working, **Then** the workspace does not incorrectly mark shared configuration as changed.
2. **Given** a shared configuration property changes, **When** the user has not saved, **Then** a clear “Có thay đổi chưa lưu” state is shown.
3. **Given** an authorized editor explicitly saves, **When** the save is confirmed, **Then** the UI reports the current map version in user-facing terms and clears the unsaved state.
4. **Given** unsaved shared changes exist, **When** the user attempts to leave or change project context, **Then** the UI warns about those unsaved changes before discarding them.
5. **Given** a stale, denied or ambiguous save result, **When** reconciliation is required, **Then** the workspace does not falsely show a successful save.

---

### User Story 5 - Use advanced details without exposing them to everyone (Priority: P2)

A power user or developer can open a technical details drawer for the current Project/Map/Layer and inspect identifiers, CRS, revision and capability diagnostics, while ordinary users can work without seeing those values.

**Why this priority**: GIS support and debugging need technical metadata, but displaying it inline makes routine work confusing.

**Independent Test**: Use the ordinary workspace without technical values visible, then open the technical drawer and confirm the relevant scoped identifiers/revisions/capabilities are available and clearly labeled.

**Acceptance Scenarios**:

1. **Given** the technical drawer is closed, **When** the user performs ordinary map work, **Then** raw IDs and diagnostic revision metadata are not shown in primary controls.
2. **Given** the technical drawer is opened, **When** a current Layer is active, **Then** the drawer can show the permitted Organization, Project, Map, Layer/Source IDs, CRS/revision facts and capability status needed for support.
3. **Given** a user lacks authority to inspect a protected resource, **When** technical details are requested, **Then** the drawer does not disclose identifiers or metadata beyond current authorization.

---

### User Story 6 - Keep the workspace usable on smaller screens and by keyboard (Priority: P2)

A desktop user gets the full multi-panel workspace; a tablet user can collapse or switch panels while keeping the map usable; a mobile user can view the map, layers and feature details for the basic viewer scope without requiring full desktop editing parity.

**Why this priority**: The GIS workspace is desktop-first but must not become unusable at practical tablet/mobile widths.

**Independent Test**: Exercise the same prepared Map at desktop, tablet and mobile reference widths; verify desktop panels, tablet collapse behavior, mobile basic viewer path and keyboard focus for non-canvas controls.

**Acceptance Scenarios**:

1. **Given** a wide desktop viewport, **When** the workspace opens, **Then** the Layer panel, Map, Inspector and Attribute Table can coexist without page-level horizontal overflow.
2. **Given** a tablet viewport, **When** the user needs more map area, **Then** side panels can collapse or switch without destroying map/session context.
3. **Given** a mobile viewport, **When** the user opens the Map, **Then** basic viewing, layer visibility and feature inspection remain available; advanced desktop parity is not implied.
4. **Given** keyboard navigation, **When** the user moves among toolbar, panels, table and dialogs, **Then** focus is visible and core non-canvas actions have accessible names.
5. **Given** the interactive map canvas is not directly readable by assistive technology, **When** the user requires a text alternative, **Then** the workspace provides a named map region plus synchronized feature/table information for the active context.

### Edge Cases

- New Project has no Layer references and no import capability.
- Authorized catalog has Layers, but all are already on the Map.
- Layer becomes retired, stale, unavailable or unauthorized while visible.
- Optional external basemap fails while internal layers remain available.
- Long layer names or many layers exceed panel height.
- More than 100 table rows or features require paging/virtualized presentation.
- Selected feature is outside the viewport or has no renderable geometry.
- Source revision changes while Inspector/Table are open.
- Save commits but response is lost, or a stale save is rejected.
- User switches organization/project while delayed map/table results are returning.
- Browser refresh occurs with panels resized/collapsed.
- Narrow viewport, browser zoom or long Vietnamese labels create layout pressure.
- Drag/reorder is unavailable to a keyboard-only user; equivalent non-drag controls must exist.
- External map tiles are unavailable or produce non-deterministic visual output during screenshot testing.
- Add-data action hands off to an unavailable downstream import capability.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The GIS workspace MUST present a stable professional layout with a primary Layer panel, central Map, Inspector, collapsible/resizable Attribute Table and command area at desktop reference widths.
- **FR-002**: The workspace MUST keep Project name, current user-facing mode and saved/unsaved state visible without exposing raw internal identifiers as primary context.
- **FR-003**: Organization/Project/Layer/Source UUIDs, internal roadmap IDs, raw enum values and low-level revision identifiers MUST be hidden from ordinary primary workflow.
- **FR-004**: The workspace MUST provide an explicit technical/details surface for permitted IDs, CRS, revision and capability diagnostics.
- **FR-005**: A Project with no map layers MUST present an explanatory empty state with applicable actions such as Add existing Layer, Import data when available, or guidance.
- **FR-006**: The Layer panel MUST support active-layer selection, visibility, deterministic ordering, Zoom to Layer and removal of a Map Layer reference for applicable Layers.
- **FR-007**: Removing a Map Layer reference MUST NOT imply deletion of the underlying catalog Layer or source.
- **FR-008**: The Add Data entry point MUST present authorized existing catalog Layers that are not already referenced by the Map, using human-readable names and usable states.
- **FR-009**: Duplicate layer-reference behavior MUST be prevented by default or explicitly explained before creation; accidental silent duplication is not acceptable.
- **FR-010**: When a downstream import capability is available, Add Data MAY hand off to it, but this child slice MUST NOT redefine import validation, job, provenance, CRS or publication behavior owned by R005.
- **FR-011**: Loading, usable, empty, unsupported, unavailable, stale, unauthorized and budget-limited Layer states MUST be distinguishable using user-facing language.
- **FR-012**: Capability absence and permission absence MUST be represented as different causes; unsupported actions MAY be shown disabled with explanation, while unauthorized actions SHOULD be omitted from ordinary flows where appropriate.
- **FR-013**: Frontend capability/permission presentation MUST NOT replace server-side authorization.
- **FR-014**: Map and table selection MUST remain synchronized for the current valid active Layer and pinned data context.
- **FR-015**: The Inspector MUST present typed attribute values and relevant CRS/context facts while preserving null, empty and zero distinctions.
- **FR-016**: The Attribute Table MUST be collapsible and resizable at desktop widths without remounting or resetting the Map solely because its panel geometry changes.
- **FR-017**: Paging, filtering or long-table presentation MUST NOT require rendering an unbounded number of rows at once.
- **FR-018**: Selection and focused-feature state MUST remain valid across panel collapse/resize and valid table page changes, and MUST be invalidated when its authoritative context becomes stale.
- **FR-019**: Shared-configuration changes MUST produce a visible unsaved state distinct from transient personal exploration.
- **FR-020**: The workspace MUST NOT silently autosave shared Map configuration.
- **FR-021**: An authorized editor MUST explicitly invoke Save for shared Map configuration changes.
- **FR-022**: Successful save confirmation MUST use user-facing map-version terminology; low-level revision details MAY be available in technical details.
- **FR-023**: The workspace MUST warn before navigation/project-context change would discard unsaved shared configuration changes.
- **FR-024**: Stale, denied, failed or ambiguous saves MUST preserve an honest unresolved/error state and MUST NOT be presented as successful.
- **FR-025**: Personal UI preferences such as collapsed panels, panel widths and selected workspace tab MAY persist locally but MUST remain separate from shared Map configuration.
- **FR-026**: The toolbar, context actions, command search and keyboard entry points for the same map command MUST converge on the same observable operation and availability semantics.
- **FR-027**: Initial command search scope MUST include applicable actions such as open project/map, Add Data, open a Layer, Zoom to Layer, clear/zoom selection, save map, and navigate to available import/export or access-management surfaces.
- **FR-028**: The workspace MUST use Vietnamese as the default user-facing language while retaining established GIS standards/terms where translation would reduce clarity.
- **FR-029**: The workspace MUST use one coherent light-theme visual system in this slice; dark mode is outside initial scope.
- **FR-030**: Desktop layout MUST support the full multi-panel experience; tablet layout MUST allow panel collapse/switching; mobile scope MUST support basic map viewing, layer visibility and feature inspection without claiming advanced desktop parity.
- **FR-031**: Primary non-canvas interactions MUST expose visible keyboard focus and accessible names.
- **FR-032**: The Map canvas MUST be exposed as a named region and the active context MUST have a text/table alternative sufficient to inspect key feature facts without relying solely on canvas pixels.
- **FR-033**: Drag-based layer reordering MUST have an equivalent non-drag interaction for users who cannot use precision dragging.
- **FR-034**: The workspace MUST respect reduced-motion preferences and MUST NOT use motion as the sole indicator of state.
- **FR-035**: External map imagery/tiles MUST NOT be required for basic workspace usability; local/internal map work MUST remain functional when optional basemaps fail.
- **FR-036**: Visual regression acceptance for deterministic workspace chrome MUST not depend on live third-party tile pixels.
- **FR-037**: The child slice MUST reuse parent R004 Map/Layer/selection/configuration/command semantics and MUST NOT couple business UI to a particular rendering engine's internal object model.
- **FR-038**: The child slice MUST preserve the distinction between shared Map state and local Workspace Session state established by R004.
- **FR-039**: The child slice MUST not expand into geometry editing, advanced query, processing, cartography, CAD, offline sync or business-specific modules; those remain owned by their roadmap capabilities.
- **FR-040**: Canonical workspace navigation SHOULD integrate with the authenticated app shell defined by the R002 frontend child slice while preserving supported legacy deep links during migration.

### Key Entities

- **Workspace Layout State**: Personal presentation state for side-panel visibility, panel widths, lower-table height and active workspace tab; separate from shared Map configuration.
- **Layer Panel Item**: User-facing representation of a Map Layer reference, including human-readable name, active/visible state, order and permitted capability/status cues.
- **Add Data Choice**: An authorized existing catalog Layer or an available downstream data-ingestion entry point; it does not itself redefine source/import semantics.
- **Workspace Selection View**: The synchronized visible representation of the parent R004 selection/focus state across Map, Table and Inspector.
- **Save State Indicator**: User-facing state describing clean, unsaved, saving, saved, conflict/error or unresolved outcome for shared Map configuration.
- **Technical Details View**: Secondary scoped presentation of permitted identifiers, CRS, revisions and capability diagnostics.
- **Responsive Workspace Mode**: The desktop, tablet or mobile presentation mode controlling how panels are simultaneously visible while preserving the same authorized map context.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: At the desktop reference width, a prepared Project Map exposes Layer panel, Map, Inspector and Attribute Table in one workspace with no page-level horizontal overflow.
- **SC-002**: In observed task testing, at least 4 of 5 GIS-capable first-time users can open a prepared Project, identify the active Layer, inspect an expected feature and determine whether shared changes are saved within five minutes using only a one-sentence task.
- **SC-003**: In 100% of agreed map-to-table and table-to-map acceptance scenarios, the selected stable feature identities and focused feature agree across Map, Table and Inspector for the current valid context.
- **SC-004**: In 100% of tested shared-state scenarios, transient pan/zoom/selection alone does not create a false unsaved indicator, while every tested shared configuration change does.
- **SC-005**: In 100% of tested successful saves, the UI reaches an explicit saved state only after authoritative confirmation; stale/denied/ambiguous outcomes never appear as successful.
- **SC-006**: A user can perform visibility toggle, active-layer selection, Zoom to Layer and remove-reference actions using keyboard-accessible controls without requiring drag input.
- **SC-007**: At 768 px and wider reference widths, no primary workspace panel/navigation control causes page-level horizontal overflow; at mobile reference width, basic view/layer/inspect flows remain usable.
- **SC-008**: Accessibility checks find a named Map region, visible focus for primary non-canvas controls, accessible names for icon-only actions, and a synchronized table/list path to inspect active feature data.
- **SC-009**: A terminology audit finds no roadmap IDs, raw backend enum labels or unlabeled UUIDs in the primary workspace chrome, Layer panel or normal save-state messaging.
- **SC-010**: Screenshot regression covers deterministic states for Empty Workspace, Workspace with data, Layer panel, Attribute Table collapsed/expanded, Technical Details closed/open and Unsaved/Saved indicators without relying on live external map-tile pixels.
- **SC-011**: Resizing/collapsing panels and switching ordinary workspace tabs does not reset the current map camera or valid selection solely because UI layout changed.
- **SC-012**: Parent R004 authorization, CRS, bounded-read, selection, save/conflict and recovery acceptance evidence remains valid; this UX slice introduces no alternate client-owned semantics.

## Assumptions

- Parent R004 remains the authority for one-Map-per-Project semantics, Layer references, bounded render/read behavior, selection, saved configuration, command identity, capabilities and revision/conflict behavior.
- This child spec may be planned while R004 is in progress, but implementation MUST remain gated until the integrating owner identifies an accepted R004 contract baseline and reconciles any differences.
- R005 owns import/export semantics. This child slice only defines discoverable entry points/handoffs when those capabilities exist.
- The R002 frontend child slice owns the global authenticated app shell, account/session controls and canonical `/app` navigation. This slice owns the Project Map workspace inside that shell.
- Desktop is the primary professional GIS experience. Tablet is adaptive; mobile initial scope is basic viewer/layer/feature inspection.
- A local blank/grid or otherwise internally available map background remains sufficient for core usability. Optional licensed basemaps are enhancements, not a dependency for acceptance.
- Exact panel sizes, typography scale, interaction timings and bundle/performance budgets will be decided in planning from measured baseline and the approved design-system direction rather than invented in this specification.
- Dark mode, advanced styling, editing, query builder, processing, cartographic composition, CAD, offline and business modules are outside this child slice unless surfaced only as capability-aware navigation entry points.
