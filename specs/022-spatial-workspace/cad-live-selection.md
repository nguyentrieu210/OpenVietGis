# 034 · CAD Live Selection Bridge (bounded, capability negotiated)

## Workflow
The user selects an entity inside an embedded CAD editor and sees the matching GIS source objects, without typing a DXF handle. A GIS-origin selection should request focus on that CAD handle only if the embedded engine advertises native selection support.

## Safety and scope
- CAD iframe remains on a dedicated origin, accepts strict origin/source/channel messages, and receives only drawing bytes and opaque CAD handles; no GIS access token or project id.
- Native selection events are untrusted input. Host validates DXF handles, then rechecks against the existing project-scoped immutable document/version/handle links API. Never infer feature IDs from geometry or a user-supplied handle.
- Select/focus capability is separately advertised by engine; legacy deployed OpenCAD Studio 2026.46 does not yet support this protocol and stays on manual handle lookup. Do not claim native selection works in production until new source revision is reviewed, compiled, staged and tested.
- Do not change current save protocol or bypass dirty/saving state. Switching GIS/3D requires clean editor, project rights and live link validation. Multi-link selection is explicit.
- No new database migration, production write or main merge. Rollback by disabling negotiated selection in the candidate relay/host.

## Acceptance
- [x] Optional `selection` capability is passed engine -> relay -> host with existing protocol version, without misrepresenting old engines.
- [x] Native `ocs:selection` and host `ocs:select` messages are validated, origin/channel/version checked, and never authorize data access.
- [x] UI finds exact linked source after selected CAD handle and offers GIS/3D actions; stale queries cannot change selection; no fabricated links.
- [x] Test supported mock, unsupported engine, malicious window message, readonly/dirty behavior and no-regression save flow.

## Protocol and release gate
- The CAD engine advertises `selection` as an optional extra capability at `ocs:ready`. It may then emit `ocs:selection` with `{name, handle}` (single 1–32 hex DXF handle or null to clear), and must accept an `ocs:select` message containing one handle to focus/select exactly that native entity. The relay translates these to `selection` / `select` on a separate origin with the existing 128-bit channel. It never upgrades unsupported releases. Events are limited to a single selected entity, no bulk editing, no DXF-to-GIS inference and no implicit saves.
- Engine **must** implement/ship the two native messages, with real click-to-entity mapping, tab/file version validation, and native tests before it may claim `selection`. The currently pinned OpenCAD Studio 2026.46 lacks these messages. 034 is the tested host+relay/UI implementation plus a mock engine protocol; direct production click-to-select is still blocked by engine release/source verification, TLS/origin review and deploy approval. UI says so and keeps the manual DXF handle workflow.
- Do not deploy the relay script ahead of a compatible engine/host rollout without review. It remains additive; old embed saves work unchanged.

## Evidence (2026-11-23)
- Local cross-origin Chromium fixture using **real candidate relay.js** and a protocol-speaking simulated engine: initial host focus, valid entity selection, scoped handle requery, other/invalid/cleared handle, dirty action block, delayed-response cancellation, fake-window message ignored, permission revocation and explicit legacy fallback all passed. No external requests.
- Integrated SpatialWorkspace test against real PostGIS-produced DXF + GLB captured from a fresh 0034 database: GIS -> 3D highlight -> CAD vault -> embedded CAD mock selection -> GIS exact source, including unknown handle clearing and old Feature 033 stale/revoked checks. Chromium/SwiftShader rendered generated GLB. Full GIS/CAD source still authenticated only in API test, browser uses recorded API replies.
- Existing application CAD revision-save/georeference smoke passed with legacy relay protocol. Dedicated PostGIS CAD import/reverse link integration 1/1 passed; 36/36 spatial workspace tests, strict package/web TypeScript, production candidate Vite build, relay `node --check`, Ruff and `git diff --check` passed.
- Feature branch only. No migration, release rebuild, deployment, main merge or production data changed. Native CAD engine not updated.

## Native source follow-on (candidate, not released)
- Separate CAD VPS checkout `/opt/opencadstudio`, branch `feature/034-cad-native-selection`, commit `775dac9`. Rust embed bridge now emits a single uppercase DXF handle on settled selection, receives a validated `ocs:select` for the loaded tab, and rejects hidden/out-of-space targets. A native pure-function test for handle parsing/selection passed 3/3. Full WASM check/build/real engine browser proof is a separate release gate and was still being verified at this checkpoint.
- The GIS relay now forwards the loaded filename along with the handle, required by the native tab pin, and caps native handles at 16 hex digits. Legacy manual lookup remains for pinned old releases. No main merge/deployment.

## Native build verification update — 2026-11-23
- CAD source `775dac9` was checked with a real stable Rust toolchain:
  `cargo check --locked --target wasm32-unknown-unknown --bin OpenCADStudio` **passed**.
- Real source-derived 3D/GIS browser tests and mocked OpenCAD selection bridge
  remain passing. Actual **rebuilt WASM browser click-to-select is not yet
  verified**: the isolated release-bundle attempt was stopped due to memory
  pressure on the CAD VPS, avoiding impact on running services.
- This branch must not be treated as a deployable native CAD release. Use a
  dedicated build runner and test real click/selection, then provenance/CSP
  gates and deployment approval.
