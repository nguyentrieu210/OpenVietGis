# 066 implementation slice 01 — route-aware entry / offline precache
Baseline code: 5bfc83826161acd83b98a925e85690cebde53ae1 (spec-only d716ba7 used for reproducible baseline build). Node 26.13.1, pnpm 12.21.0; isolated VPS worktrees, same locked dependencies. Candidate source is the commit containing this evidence.
- T002/T004 partial: source-inventory.mjs pinned inventory (2271 tracked files, 14 workflows, 13 root legacy script references); runtime roles and complete dependency graph remain pending.
- T014/T015 partial: route-first lazy page; separate SessionEntry from authenticated workspace; five optional panels deferred. Public landing no workspace/optional chunks in network or service worker cache. Direct authenticated CAD/GIS/3D mount and screenshot matrix remain pending.
- T018: manifest and Chromium network evidence: initial landing static dependency closure 903464 -> 109559 gzip bytes (-87.9%); optional workbenches excluded. No speculative prefetch.
- T022/T025 partial: static manifest closure excludes unrelated dynamic modules; offline install assets 35 -> 10; raw 3940266 -> 328491 bytes (-91.7%). Gzip 1118353 -> 104178 (-90.7%). HTML/SW and arbitrary non-hashed mjs revalidate in repo Nginx config. No live CDN change or upgrade from a previous worker tested.
Validation:
- pnpm --filter @openvietgis/web-next build: PASS (includes tsc, Vite, offline-shell generation).
- pnpm --filter @openvietgis/web-next test: 65 Vitest + 26 Node tests PASS.
- pnpm exec playwright test --config playwright.offline-client.config.ts: 2 PASS (public entry isolation; actual browser offline draft/disconnect/reload and sync receipt).
- nginx -t: PASS with repository config in isolated wrapper substituting loopback for Docker DNS; no live reload.
- git diff --check: PASS.
All size measurements are manifest/build artifact metrics, not field CWV, transfer timing, API/GIS/CAD or CPU/RAM measurements. No tenant data, live service, or legacy source removed. Remaining tasks are pending; 065 is active and must be reconciled before merge.
