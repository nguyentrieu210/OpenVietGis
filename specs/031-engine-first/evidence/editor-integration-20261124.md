# GIS runtime checkpoint — 2026-11-24

## Runtime changes
- Fix committed undo/redo dialog: the latest history entry must match the live
  layer revision, not a stale snapshot captured at dialog open. Match source
  and layer identity; require current project edit and layer edit capability.
  Existing server-side revision, ACL, idempotency and receipt checks remain.
- Add optional lazy Terra Draw 1.40.0 validation in the actual GIS create flow,
  using its upstream self-intersection validator for LineString and simple
  Polygon only. Flag VITE_OVG_TERRA_DRAFT_VALIDATION=1 enables the experiment.
  Default build is off. Existing sketch, typed input, CAD snapping, draft
  undo, attributes and commitChangeset remain authoritative. No dual writes.
  No revalidation of a locked uncertain-receipt retry. Invalid preview sends
  no changeset; valid geometry is not transformed.
- Add exact package integrity in pnpm lock. Upstream npm gitHead
  a598ae63393eceb5b31f88aeee7000bdf0f55f60; MIT license text retained in
  public/licenses/terra-draw-1.40.0.txt. No declared runtime dependencies.
  Full independent SBOM/security review remains open.
- Add a reproducible regression command and CI step, run on synthetic mocked
  APIs using a loopback server only.

## Verification
- Frozen dependency install, web-next typecheck and both flag-on/off
  /rewrite/ frontend builds passed. Enabled validation chunk ~1.7 KiB.
  Default build does not contain that chunk.
- Flag-on Chromium: no validator request at startup; self-crossing polygon
  rejected before any ChangeSet POST; valid polygon sends one POST with exact
  geometry, typed attributes and pinned revisions; validator loads once;
  no page errors.
- Default build Chromium: advanced editor (including committed undo/redo),
  core toolkit (map save, creation, 13 workbenches) and spatial cache all pass.
- Spatial Workspace 36/36 and spatial sync core 4/4 unit tests pass.
- Spatial cache mocked fixture: 2 cold pages; unchanged/changed F5 0 full
  pages; changed F5 one detail and one history read; restricted scope 2 pages.

Run from a clean checkout with dependencies and Chromium installed:
```bash
pnpm install --frozen-lockfile
pnpm exec playwright install chromium
bash tools/validate_engine_first.sh
```

This is an incremental pilot, not a replacement of the drawing engine.
G0/G1/G2 and full actual DB/QGIS/Windows/CAD/3D acceptance remain open.
No schema migration, live deployment or production data access.
