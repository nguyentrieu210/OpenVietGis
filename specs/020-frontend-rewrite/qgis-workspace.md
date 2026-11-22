# QGIS-inspired desktop workspace — 2026-11-23

The GIS workspace presents compact menu and icon toolbars, a fixed left browser/Layer dock, a resizable map canvas, an attribute table bottom dock and an object inspector right dock. The command prompt remains available. This is a QGIS-inspired interface rather than a QGIS clone.

## Layer behavior
- Actual GIS layers are visible in a compact tree. Plugin contributions may create parent nodes (house-number survey layers); child visibility and mixed parent checkbox state are independent of layer selection.
- Click layer name to select; separate checkbox controls visibility. Double-click opens properties; right click opens zoom, table, visibility, ordering and symbology actions.
- Drag-and-drop and up/down order use actual rendering order; SAVE sends the existing revisioned server map configuration. Layer icons use declared geometry type and actual style.
- Filter matches normalized Vietnamese names. Expanded symbol nodes show declared family/CRS/edit availability. The browser lists actual project sources via the SDK, and links to existing catalog and external connections.
- Attribute table reads existing authorized loaded features, can scope to a layer, filter and paginate. It is not a new unrestricted backend query.
- Map status shows live coordinates, approximate screen scale, zoom and display CRS. Dock widths resize MapLibre via ResizeObserver.

## Verification
TypeScript and production build passed. Staged Playwright smoke verified selection/visibility, right-click table and search, drag order and up/down, group expansion and partial visibility, dock resize, mobile recovery, Core/plugin isolation, and all existing workbench/command regression checks. Tests use fixture APIs; no production survey mutation. Production assets published atomically after backup. 
