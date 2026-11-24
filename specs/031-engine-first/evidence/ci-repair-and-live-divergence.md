# CI repair and live divergence — 2026-11-24

Candidate 82114ff; prior run 45364328293 backend reported 363 passed,
17 failed, 4 skipped. Scope validation had passed. Repairs:
- Correct the PostgreSQL dollar-quoted DO block in historical collaboration
  downgrade. Preserve the explicit refusal when private messages exist.
- Test failed downgrade against the exact prior revision set, not an obsolete
  hardcoded head; verify retained history and FORCE RLS after rollback.
- List retained extension records under existing project read authorization
  and activation existence, without invoking mutation-only manifest validation.
  Unactivated unrelated extensions remain 404; verify no leaked records.
- WebSocket presence uses its own origin/cookie/project and scoped-member
  authorization and periodic revalidation. Do not attach HTTP-only Request
  dependency. Added restricted-member denial to existing outsider/origin tests.
- QGIS roundtrip test forwards explicit allow_deletes to real endpoint.
- RLS full-schema assertion includes the five immutable 3D tables; original
  fixture loops still check their seeded R003 tables.
- Install system GDAL, numpy and Python bindings on both CI jobs, matching
  the raster workers' /usr/bin/python3 interpreter.

On separate disposable PostGIS databases: first targeted set 31/33 pass,
remaining two repaired and rerun 2/2 pass; six raster/terrain/scene tests
pass with local GDAL 3.10.3. These are targeted tests, not a complete CI pass.

Read-only live provenance: frontend marker 2290e89af92d2669c24cb9c78e99dd070331fc28
exists in /opt/openvietgis, tree 1d27efbb400164df9053486223b8c6e7e2d19cec.
It is 43 commits ahead and 12 behind GitHub main edb5175 by ancestry;
some changes have equivalent content under distinct commit IDs. Compared
with R031 82114ff it has 43 unique ancestral commits, R031 29. A nonmutating
git merge-tree trial reports 29 conflicting paths, including managed_sync,
QGIS plugin, spatial workspace, Cesium, CI and binary plugin artifacts.
Retain both sources, reconcile with the ongoing live development stream
before promotion. Never replace live blindly from R031. No production
mutation or merge to main. Signed orchestration git-audit attempt failed
to return a receipt; independent certification remains absent.
