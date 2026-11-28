# 066 cleanup-state closure — T010/T011/T012/T013

- T010: pnpm workspace and lock contain no apps/web importer; production Docker, root web scripts and Playwright consumers point only at web-next. Frozen offline install, typecheck, unit/build and architecture gates pass after retirement. Python backend/migration commands are intentionally retained because Rust still embeds/reference-tests Python and the migration role is Python; they are not stale commands.
- T011: the proven duplicate frontend implementation was removed. Application UI remains under web-next/workbenches; reusable client and spatial engine contracts remain in client-sdk/spatial-workspace. Architecture tests enforce the direction. No speculative directory churn was performed where no duplicate ownership was proven.
- T012: current architecture and agent instructions now name only the canonical frontend. Historical specs/QA are preserved unchanged and indexed by docs/archive/legacy-frontend-history.md so old path evidence remains traceable.
- T013: git ls-files contains no dist/build/coverage/node_modules/cache/target, source-map or pyc outputs. Git/Docker ignore contracts cover generated outputs; generated Cesium public assets remain explicitly ignored and reproducible from the pinned package.
- New repository-state tests make these cleanup properties executable instead of relying on prose.
