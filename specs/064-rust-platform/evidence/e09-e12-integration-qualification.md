# Spec 064 E09–E12 integration evidence

Integration base: `6e3b74c`. Frozen Python reference: `3e78ac1`.
Candidate policy changes: `20233af` and `f7e19b8`.

| Engine | Integration state | Remaining acceptance evidence |
| --- | --- | --- |
| E09 Raster, terrain, survey | The 8 inventoried HTTP operations remain in FastAPI/GDAL. Existing raster policy and pixel tests ran with host GDAL 3.10.3. | Complete project-level API integration tests. Qualify native kernels and confirm CRS, NoData, scale/offset and resource parity before changing routing. |
| E10 CAD, 3D, model and scene | The 30 inventoried HTTP operations remain in FastAPI. Existing TypeScript workspace and MicroStation adapter files match the frozen reference. | Exercise CAD round trips and model/scene revisions through project HTTP endpoints, plus browser/WASM, Cesium and the actual Windows adapter. |
| E11 Realtime | The integration base includes the staged PostGIS outbox and the scoped `ProjectRead::replay_spatial` repository method. The Rust HTTP application exposes no E11 replay or streaming route and does not produce outbox records. Existing Python SSE/WS and collaboration surfaces remain authoritative. | Wire transactional producers, authorized replay and client reconnect first. Verify revocation, cursor state, idempotency and lease/chat/task behavior against the frozen contracts. |
| E12 QGIS and external sources | The integration branch incorporates the bounded pure Rust COG policy. It validates range/encoding/media/ETag metadata before exposing the expected body length and then verifies the received byte count. | Native HTTPS transport, live authorization and connector lifecycle remain in Python. Native response parity requires real PostGIS and QGIS/Windows consumer fixtures, guarded route wiring and rollback. |

No E09–E12 public route ownership changed. The Python application, spatial workspace and
MicroStation adapter source trees match the frozen reference for this branch. There is no
representative native endpoint traffic to compare for these slices. No latency, throughput
or RSS gains are claimed, and acceptance and cutover remain open.

## Observed validation

- **248 Python checks passed** with pinned API dependencies. Includes external provider/COG/raster,
  the 14 shared E12 cases, COGO, DXF integrity, native CAD delta, V8i capability and bridge tests.
- **13 Node checks passed** for CAD georeferencing, 3D footprints, mesh picking and tilesets.
- **6 standalone Rust 1.85.1 tests passed** for range policy, including the 14 shared cases
  and short/oversized body rejection after successful header validation. Rust formatted
  and Python test file passed Ruff.
- **E11 PostGIS SQL probe passed** in a disposable migrated database: rolled-back events
  invisible, bounded retention, tenant RLS, denied runtime inserts/append, and removal via
  candidate rollback. Rust/PostGIS replay integration requires the dedicated CI job.
- The combined E09/E10 project API run exceeded the remote command deadline. The unit
  and SQL results above do not certify full HTTP, desktop, browser or gateway parity.
