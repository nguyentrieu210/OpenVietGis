# Live-source convergence candidate

Parents: GitHub R031 6f78a1ed42bb1be0f7a2dfdc870c94c6570485d4
and VPS live frontend source 0aa2cfaba69bbad47896f5e315f066f56b7f7143.
Live source was pinned from RELEASE_COMMIT; no production write or deploy.
31 conflicting paths resolved in an isolated checkout. Preserve the latest
live coherent managed-session actor/epoch authorization, delta coalescing,
snapshot loader, desktop plugin, workspace navigation, survey terrain and TIN
implementation. Preserve both CI jobs, documentation, dependency entries,
R031 revision-aware undo, optional Terra validation, source-link invalidation,
CAD notice and version-pinned smoke, exact map-save assertions. The 100-mutation
test now checks exact IDs per layer/alias at the final coalesced revision and
replay identity; the live test still covers stale-page reset and cursor ack.

Frozen lockfile install and typecheck passed, Spatial Workspace 44/44,
scope/architecture/docs and Ruff passed, 49/49 scoped/delta/snapshot unit tests.
Ten selected real PostGIS integration tests passed including the 100-mutation
case and prior presence/lifecycle repairs. On isolated mock browser build:
Terra validation, Core Toolkit, CAD bidirectional linkage passed; CAD direct
georef passed after waiting for the upload input to be enabled. Advanced
Editor printed all passing assertions but exceeded the outer 42s deadline
during teardown; Cesium tile screenshot was interrupted by the same deadline.
Both must be rerun with an adequate bounded timeout; no full CI or release
certification yet. No engine deletion.
