# Consolidation findings — 2026-11-24

Initial source baseline: GitHub main 27f0b35. Local main 3c25f3b was 19 commits behind, zero ahead. Operational working checkout 96897a2 was not main. The public web symlink resolved to v2026.11.24-5d7c3b9-qgis-v1; its RELEASE_COMMIT marker is 5d7c3b9e63ef2c407ebf32add54eae7d6dcfca36. This is a marker, not proof of all running code. API service working directory /opt/openvietgis; public MCP working directory release 5baade9; separate staging MCP path 65b1198. Live deployment remains heterogeneous and unverified.

## Local candidate selection
- Baseline audit 40d5dbc (Spec 031).
- Merged 7b34685: includes public QGIS 1.0 changes through 5d7c3b9 and two subsequent pairing/snapshot corrections (fee2f93, 7b34685). Migration 0042 introduced; no live migration performed here.
- Merged 22e79a8: GIS/CAD/3D mode accessibility and empty 3D workspace fix.
- Cherry-picked 022541e to f5c0037: MCP instructions preserve authorization, consent and revision checks. Did not merge its old parent history.
- Parallel QGIS branch f886514 overlaps newer QGIS implementation. Retain for review; do not force-merge or delete. All historical branches and active worktrees preserved.

Candidate f5c0037 passed frozen offline dependency install, web-next build/typecheck, 42 targeted QGIS/spatial/audit tests, 34 MCP tests, single migration graph head 0042_qgis_pairing_scope. Rebuilt QGIS ZIP byte-matched shipped ZIP d3e57786695ff82f0356ee3a618829487988d2b84ebb09eea781175457af4093. Frontend main bundle ~1.98 MB minified; code splitting remains a performance backlog. Not full CI, native QGIS or database migration certification.

The read-only [ref snapshot](evidence/refs.json) contains 72 local/remote refs, 30 worktrees (4 dirty), 29 refs with non-ancestor commits relative to f5c0037; counts are point-in-time and include audit worktrees. Ancestry inclusion does not authorize deletion or prove feature completeness. No branch deletion, push, main modification or deployment in this slice.

## One-source release procedure
1. Designate reviewed immutable SHA; make clean isolated checkout and verify lockfiles. Use apps/web-next for public release; keep apps/web as compatibility/testing source until Spec 033/037 and parity evidence.
2. Build frontend from that checkout, build API and MCP deployables from same SHA, regenerate and compare QGIS plugin. Record artifact hashes, image digests, migration graph and actual tested schema separately. Use release_consolidation.py for local source/lock/web/connector inventory; it does not validate backend binary provenance or migration execution.
3. Execute full exact-SHA CI, real web-next E2E, native Windows QGIS and isolated migration, restore, rollback, security/license tests. Attach independent evidence to Spec 030 and existing release gate.
4. With maintainer approval only, publish candidate, migrate with backup and tested recovery, atomically switch public components using immutable artifacts. Verify observed live build IDs, health, consent and smoke flows. Rollback follows validated recovery plan; no blind schema downgrade.
5. Archive obsolete branches only after ownership, ancestry, feature parity and retention review. Keep active worktrees and recovery artifacts.
