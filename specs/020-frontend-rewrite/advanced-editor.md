# GIS Core — Advanced editor
- NEWLAYER: manual Point, LineString, or Polygon layers, native EPSG:4326 XY, typed schema (100 fields maximum). Source and layer created atomically with catalog/audit evidence; UUID request replay returns the original receipt.
- VERTEX: edit line/polygon rings on the map; marker drag, snap, insert midpoint, remove vertex, undo local draft. Over 150 vertices only selected vertex is draggable; all vertices remain displayed and selectable via dropdown.
- SPLIT: draw cutter, preview PostGIS ST_Split (2–16 parts), preserve original FID and clone original properties to new parts. Save as one revision-pinned changeset.
- MERGE: choose 2–16 features of the same layer, preview PostGIS union/line merge, reject disjoint output; retain first selected FID and properties, delete others atomically after explicit confirmation.
- BULK: exact attribute filter or all loaded objects, edit typed value or NULL, display count, explicit confirmation, maximum 2,000 per changeset; preserve unedited attribute keys, use stable field IDs.
- All mutations require project.edit; existing source/schema revision pins and CSRF remain authoritative. No test writes to production project data.
- 0020_manual_layers migration expands kind constraint only. Downgrade intentionally fails if manual sources remain rather than deleting them.
- Integrity: failed or ambiguous commits preserve the draft; ambiguous retry keeps the same operation ID and payload. Reference fixtures remain read-only.
