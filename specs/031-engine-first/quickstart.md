# Validation runbook
1. Use clean isolated worktree pinned to baseline; record Git SHA, OS, Node/Python versions, browser, CPU/RAM/GPU, DB/GDAL/PROJ versions and dependency lock hashes.
2. Follow root README for pnpm/uv bootstrap and isolated Compose; use synthetic test tenants only; do not use production credentials or data.
3. Run existing repo CI-equivalent checks and capture failures before changing code. Locate current workflow commands in .github/workflows; do not assume old paths.
4. Fixture set: 10k and 100k vector features, polygon holes/multipart/invalid geometry, controlled VN-2000 points and explicit transformation, GeoPackage/GeoJSON/Shapefile, supported DXF, local GLB/3D Tiles, two tenants and viewer/editor accounts.
5. Baseline at least 30 warm samples per critical operation plus cold start, repeated same dataset and profile. Record p50/p95, network bytes/request count, peak memory, error rate and accuracy. Freeze tolerance and thresholds before testing replacement.
6. Test candidate off/on/shadow, authorization failures, stale revisions, cancel/reconnect, engine load failure, roundtrip and rollback. Store logs/results under evidence/ with exact candidate SHA and redacted identifiers.
7. Run independent review and R030 release gates. Never declare implementation complete from a spec, package install or mocked test alone.
