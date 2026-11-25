# Experimental editor draft adapter — checkpoint

The existing CoreMapTools owns command entry, coordinate input, ortho/snap, draft
undo, field schema validation and the explicit commitChangeset action. The candidate
only prepares an in-memory ChangeSet-shaped create proposal. It does not call an API,
change project data, replace the legacy editor or implement authorization.
The host must validate schema and the server remains the authoritative ACL/revision gate.
Only WGS84 Point, LineString and simple single-ring Polygon are attempted. Topological
validity (including self-intersection), CAD snapping, holes and multipart are not
accepted by this slice; server validation and comparison remain required.

- Node unit tests: 5/5 passed, stale scope/revision, revoked flag, coordinate limits,
  stable operation ID, immutable retry payload and dispose/late callback.
- Chromium: Terra Draw output passed through the draft adapter; exact geometry,
  revision and feature ID preserved; stale and revoked candidate rejected.
- 20 synchronous start/stop cycles: draft layers cleared on every cycle;
  observed p95 11.30 ms (local lifecycle calls only, not rendering latency).
- All page/console errors are checked by smoke harness. Existing production runtime
  and dependency manifests unchanged.
- No real backend authorization or save/reopen test in this PoC; G0/G1/G2 remain open.

Run: node --test specs/031-engine-first/poc/terra-draw/draft-contract.test.mjs
and node specs/031-engine-first/poc/terra-draw/smoke.mjs after isolated npm ci.
