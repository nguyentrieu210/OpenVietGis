# Cross-engine verification — 2026-11-24

Tested R031 parent 8bbc3e0 (same tree as VPS 5b88236).
No production project data used or production deployment performed.

## Real database
A separate loopback-only disposable PostgreSQL container was started using
the pinned PostGIS image sha256:fe440a27c6567aefa932e988962e5827bb2c29e4b97dac2ca5268f5eafe10262.
Core migrations through 0042 and house-numbering migrations through hn_0010 applied.
Runtime PostgreSQL 18.7, PostGIS 3.7.0, GEOS 3.15.0, PROJ 9.9.0 (network OFF).
Selected pytest collection 47 cases; progress reached 100% with all pass dots
and no failures/errors:
- 4 processing integrity, 1 CAD documents, 1 CAD georef, 8 change evidence,
  30 CRS controls, 2 manual editor, 1 processing models.
- CRS observations include the registered VN-2000 operations; largest observed
  forward/inverse EPSG:6960 residual about 1.56 m, not a survey accuracy guarantee.
- Existing PostGIS/GDAL architecture retained.

## Windows native QGIS
Exact remote 8bbc3e0 cloned separately; QGIS 3.44.15 Python, offscreen profile.
tools/smoke_qgis_native.py completed successfully:
- 8,000 points build/snapshot/native edit: 0.79 s on this host.
- Real GeoPackage OGR provider reopen, edit/create, no FID leakage.
- Native clean delta updates; dirty layer and cursor preserved.
- Retry tick isolation; deferred delete/cancel/confirm; concurrent local edits.
- Plugin controls, browser login and default deny.
This is native client smoke with synthetic fixtures, not a real two-client
authenticated network soak test.

## Cesium
Real Chromium Cesium tile smoke passed using synthetic API fixtures:
2,238 visible red tile pixels, scoped tile paths, compatibility switch,
permission revocation and no external hosts/page errors.

## CAD
Public relay observed pointing to immutable /ovg-cad-2bc3537/ release.
Previous test hardcoded obsolete /ovg-cad-2026.46/ and blocked the real asset.
Updated smoke allowlist to exact new path, optionally explicit reviewed path
via OVG_CAD_RELEASE_PATH. Origin restriction unchanged.
Verified public NOTICE.txt responds 200 and identifies GPL-3.0,
corresponding source archive and commit 2bc3537346ff2a844039685186a1b32da7149d6f.
Update UI notice link to this exact artifact. Typecheck and build passed.
Corrected real DWG smoke reached real CAD canvas and single toolbar;
saved v2 receipt and DWG signature/immutable original assertions passed.
Historical read-only reopen was reached but not completed within outer
90-second timeout. No full CAD acceptance claimed.
Two older CAD/georef/linkage fixture smokes also failed: obsolete 3D renderer
selector and subsequently stale vault workflow assumptions. Diagnostic edits
were reverted; these remain open regression tasks.

## Release caveat
Main remained 9c71db0. Read-only frontend live marker changed during work
(14eec68 to d044442); exact release/main alignment remains unverified.
No CI run for R031 parent was observed previously. Do not claim R031 complete,
delete legacy paths, or promote to production solely from this checkpoint.
