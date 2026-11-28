# 066 implementation slice 11 — route isolation, cache lifecycle and current Rust parity

Candidate parent: 267826cee5a4e9f9ede3f9e005025467f984bc9c.

## Frontend/session/cache acceptance
- Route/engine isolation browser gate PASS:
  - saved CAD reload mounts CAD without GIS layer/map/raster requests or a MapLibre canvas; GIS initializes only after explicit switch;
  - default GIS performs no CAD-document request and does not fetch the optional Cesium chunk;
  - public landing does not fetch SessionEntry/LiveApp or optional engines;
  - QGIS connect, auth completion, marketplace and developer entry routes do not bootstrap the authenticated spatial workspace.
- Protected authorization remains account/org/project scoped. Simultaneous same-scope checks share two HTTP requests; a settled check is never reused; actor/org/project/client changes isolate flights; invalidation aborts stale results; malformed/denied results fail closed.
- Session actor transitions now clear the previous actor's spatial IndexedDB cache as well as invalidating permission flights. Browser acceptance inserts an actor-1 cache row, forces /me to 401 on focus, observes landing, then verifies cache row count is zero.
- Revision reconnect acceptance replays every missed changeset in order, reads only the surviving changed feature, deletes removed features without refetch, falls back to full reload on a retained-history gap, and drops a completed replay if the workspace generation changed.
- Cross-tab authorization invalidation from slice 08 remains active: revocation clears in-memory rows and scoped IndexedDB, invalidates authorization flights and revalidates before protected state can be shown.
- Service-worker upgrade test verifies only prior ovg-offline-shell-* caches are removed; unrelated CacheStorage and IndexedDB survive. Public precache remains static-only; verified offline package/draft/sync receipt survives disconnect/reload.

Validation on this working tree before commit:
- pnpm web-next test: 21 Vitest files / 103 tests PASS plus 40 Node tests PASS.
- pnpm web-next build: PASS.
- offline Playwright suite: 8 Chromium tests PASS.
- Targeted route/engine suite: 6/6 PASS.
- git diff --check: PASS.

## Origin/cache header evidence
Isolated Nginx with the candidate dist:
- /index.html: Cache-Control no-cache, ETag present.
- /offline-shell-sw.js: Cache-Control no-cache, ETag present.
- hashed JS asset: Cache-Control public, max-age=31536000, immutable, ETag present, conditional request returned HTTP 304.
- private tile gateway performs auth_request before cache lookup, keys cache by tenant/authz/layer revision, does not cache 401/403/404, and sends private, no-store, must-revalidate to clients.
External CDN/Cloudflare account configuration is not observable from repository evidence, so T025 remains open.

## Rust parity revalidation on current 066 source
Dedicated loopback PostGIS only. The existing parity cluster on 54330 has max_connections=40; a second isolated cluster on 127.0.0.1:54331 uses max_connections=120 so listener-cap tests can exercise the application semaphore rather than exhausting PostgreSQL first. The benchmark guard was extended only to the two dedicated loopback ports 54330/54331; non-loopback or arbitrary ports remain rejected.

PASS results:
- Core differential HTTP: 288/288.
- E06 cartography read/write.
- E07 transfer parity and retained GeoPackage/Shapefile boundary.
- E09 raster catalog/content.
- E10 3D, CAD document/DXF/georef, model download, scene writes, tilesets.
- E11 managed realtime, org chat, presence, team, realtime, 40-listener cap for Rust/FastAPI/managed streams.
- E11 durable outbox/storage integration; staged replay endpoint also passes when intentionally built with feature e11-candidate. The staging replay route remains feature-gated; normal managed-sync is the production delta/resume path.
- E12 profiles/catalog/adapter transfer.
- E13 download and package list/revoke.
- E14 activation reads/history/writes/full parity.
- E15 house-numbering reads/writes and full parity: 375 cases PASS.
- E18 ops readiness/entitlement/privileged-role fail-fast/recovery.

## Runtime defect found and fixed
E13 field-package creation exposed a real runtime incompatibility: the production Debian bookworm image runs system Python 3.11, while canonical embedded Python domain sources include PEP 695 type aliases requiring Python 3.12+. The Rust field worker now creates a private Python-3.11-compatible copy of the embedded source; canonical Python source remains unchanged. The recursive CoordinateTree alias is reduced to tuple only in the worker copy; its uses are postponed annotations. Import reproduction in the production runtime image passes. Focused Rust compatibility unit passes.

A non-root production-image parity run of E13 Field & Offline then passes end-to-end:
create validation, package 201, list, verified GeoPackage download, snapshot, idempotent sync replay, operation-id conflict, out-of-AOI rejection, stale conflict, revoke, and post-revoke denial.

The parity wrapper used a separate writable rust-artifacts directory because the Python oracle and non-root Rust candidate cannot safely share root-owned artifact directories. Production already owns /var/lib/openvietgis/artifacts as UID 10001.

No production deployment or legacy deletion is authorized by this evidence.
