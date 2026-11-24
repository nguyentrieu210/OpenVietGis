# OpenVietGIS v1.0 — Unified product release

**Decision (2026-11-24):** v1.0 is the single target release. It absorbs the entire previously proposed v1.1 scope. There is no separate reduced-scope v1.0 product or parallel v1.1 implementation target. Historical specs and versioned artifacts remain traceable; existing live deployments are retained only for compatibility and rollback until a verified replacement is available. Never delete user data, migrations, history, credentials or recovery artifacts to consolidate versions.

**Status:** Development candidate; not stable, not production-certified. Existing implementation and historical evidence are inputs, not automatic acceptance. The current connector package retains its own 0.9.x version until separately accepted.

## User outcomes and mandatory acceptance

| Track | Required outcome before stable v1.0 |
| --- | --- |
| Desktop identity | Login from QGIS via system browser (Google or supported provider), OAuth authorization code + PKCE, explicit user consent, scoped expiring credentials, secure storage, tenant/project picker, logout and revocation. Existing pairing remains migration/recovery-only. |
| GIS workspace | Real 2D import/edit/save/reopen/export across supported formats; snapping, topology, spatial join/query, history, CRS and independent geometry/attribute comparison. |
| Realtime | QGIS/Web bidirectional changes, stable operation IDs, revision checks, ordered deltas, session ownership, permission changes, multi-layer pagination, reconnect, conflict recovery without duplicate writes or skipped changes. |
| Offline | Durable local drafts/outbox, restart recovery, no plaintext secrets, replay and revocation tests; no silent loss of unsent edits. |
| Processing | Buffer/Clip/Dissolve, spatial join and Model Builder with reproducible outputs and provenance. |
| CAD/3D | Supported DXF roundtrip, precision tools/COGO, documented geometry/unit loss, safe GIS/CAD/3D lifecycle, georeferencing, terrain and model asset access. No unsupported native DWG or survey-grade claims. |
| Raster | Authorized Sentinel-2/raster, NDVI and change analysis; cloud/nodata handling, CRS, reproducible results and license/provenance. |
| AI/MCP | Real authorized connection, project scope, explicit approval of writes, tool allowlist, audit, denial and revocation; no arbitrary execution or bypass. |
| Plugins | Versioned SDK/manifest, project activation, permissions, signed/reviewed distribution policy, marketplace and compatibility checks; no automatic execution of untrusted code. |
| Team | Chat, tasks, presence and feature history; two-user conflict/revocation tests and tenant isolation. |
| Basemaps | Source registry, provider license and attribution, Vietnam boundary/name review across zoom/export/share; unverified sources must not be represented as approved. |
| Operations | Exact candidate CI, real web-next E2E with PostGIS, reproducible clean install and migration, restore drill, rollback, error monitoring, security and license review. |

## Release rule

One pinned source SHA, dependency locks, frontend/backend build IDs, schema version, connector checksum and deployment profile. All in-scope tracks must satisfy their mandatory acceptance and reviewed evidence; keep unfinished features labeled experimental, and do not tag stable until the unified scope passes. Release requires independent user acceptance, including real Windows QGIS. No gate bypass, production-data testing, or broad certification by historical unit tests.

## Existing references

The [roadmap](../../ROADMAP.md), [release policy](../../docs/community/RELEASE_POLICY.md), [acceptance matrix](../../docs/community/ACCEPTANCE_MATRIX.md), [full product audit](../018-full-product-audit/spec.md), and [managed GIS client](../029-managed-realtime-gis-client/spec.md) remain applicable. This release scope supersedes the earlier informal split, not the safety and acceptance requirements of these specs.