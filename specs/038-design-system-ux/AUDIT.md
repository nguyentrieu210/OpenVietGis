# UX standardization evidence
Baseline b1d2b69. Scope: canonical web-next GIS command/ribbon/toolbar surfaces.

## Findings and changes
- Default dock exposed many secondary icons at once, with horizontal overflow. Compact dock keeps core navigation, drawing, undo/redo, measurement, toolbox and publication entry controls visible; full toggle restores all docked controls and persists preference per workspace. Floating palette controls and customized layouts remain available. All registered commands retain ribbon/command-line access.
- Search moved to leading ribbon action. Alias, label and hint search preserved. Search gets focus when opened, Escape/explicit close restores trigger focus; narrow ribbon horizontally scrolls and the portal fits viewport.
- Shared disabled-reason helper used by menu, autocomplete, command feedback and command-backed toolbar tooltip. Visible command status and focus indicator; pressed state on toolbar toggles. These are interaction improvements, not a WCAG certification.
- GIS command global handlers and execution check inert ancestor; keyboard from another mode no longer consumed by hidden GIS command bar. Portal closes when GIS inactive. Ordinary buttons and unrelated editable fields no longer lose normal letter/Escape behavior.
- Existing plugin, permission, command and CAD/3D engine paths retained. No API mutation.

## Evidence
- Canonical TypeScript and production build passed after final edits.
- Chromium spatial-mode smoke against loopback preview and synthetic API: compact/full toggle, BUFFER discovery, search focus, Escape restoration, 390x844 menu/viewport containment, hidden GIS shortcut isolation, prior empty/configured 3D, CAD mocked version save, pending-save blocker, permission revocation and active-module transitions all passed, no captured page errors.
- Chromium core-toolkit smoke passed on implementation before final minor portal-close/aria refinement: measurement, visibility, mocked save/create and all 13 command workbench paths (LA, IMPORT, EDIT, QUERY, BUFFER, CLIP, DISSOLVE, PLOT, DXF, WMS, FIELD, REVIEW, OPS). Final build and spatial-mode smoke passed again.
- Architecture and documentation navigation passed. All evidence local, no production changes.

## Remaining
This is focused UX consolidation, not total design-system migration. Broader UI tokens, layer/attribute docks, individual CAD/3D panels, modal/focus traps, real touch devices, accessibility audit, real API interactions and independent user acceptance remain open. Compact mode can still scroll on small screens; full mode remains explicit. No CAD relay fidelity or live release claim.
