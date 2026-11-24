# R048 implementation evidence — 2026-11-24

## Pinned sources
- GIS main 9c71db0e565df45af9b18dee2b44fdcc6e160fb8. R031 PR #21 draft, head c3732b0dc4147f4be3768522b52e1730a6e37243; reviewed merge gate is not met.
- CAD engine artifact 2bc3537346ff2a844039685186a1b32da7149d6f. Independent CAD stabilization branch openvietcad/r048-stability at b312b95c9ef54501037899de4753d00595f4c295 (local); new CI is configured but not run on GitHub.
- R006: domains/editing/domain.py, adapters/persistence/editing_repository.py, adapters/http/editing.py, migration 0009_editing_changesets, ADR-007.
- R027: domains/integration_hub/bidirectional.py and sync_diff.py, adapters/http/integration_bindings.py. R029: adapters/http/managed_sync.py, migration 0041_r029_managed_realtime_client; reuse existing semantics.
- CAD: adapters/cad/dxf_codec.py (bounded 2D DXF, homogeneous family, explicit loss), adapters/http/cad_documents.py, adapters/persistence/cad_repository.py, domains/cad/georeference.py, packages/spatial-workspace/src/cad-editor.ts and cad-georeference.ts. Existing selection relay protocol has no entity-delta capability; document serialization is not per-entity realtime. Current georeference uses explicit local controls and only supported grids, not general survey-grade datum conversion.
- R031 adapter contract remains draft. No edits to R031 or shared adapter in this branch.

## Executed
- 71 tests passed in 5.56 s: new synthetic R048 fixture plus DXF integrity, editing domain/replay/rebase, R029 managed sync and sync diff unit tests.
- 6 passed in 19.49 s (includes four new fixture tests already counted in the 71): versioned CAD documents + version-pinned georeference/linkage/3D + new fixture, real disposable PostGIS container, isolated artifact directory. Container removed after test. 2 library deprecation warnings.
- Synthetic projected rectangle and independent held-out point in paired local VN-2000/web controls, 1 mm tolerance, area 24 m². This is not a real surveyed datum validation. Nonzero Z rejected; unsupported text and excluded secondary family explicitly reported. DXF roundtrip compares normalized closed ring and units. Current exporter may repeat closing vertex with closed bit; normalization is explicit.
- CAD actual WASM standalone drawing/edit/undo/redo, DXF numeric comparison, BOX DWG save/reopen bounds, endpoint query, viewport pick, trim/extend/offset/rotate/scale passed. Real isolated relay handle selection, rejection and read-only save guard passed. See CAD ci/ovg/evidence.
- WASM unique raw 188634517 / gzip 59439256 bytes, original digests unchanged. Local nginx transport negotiation/fallback and cache headers passed after fixing asset modes. Shell runtime branding test passed (fresh vi, saved en, system en), preserves saved preferences; internal canvas theme remains compiled. Existing source/release/license checksums pass; new release CI and rollback/human acceptance open.

## Task status
Complete bounded work: T001 inventory, T002 capability inventory, T003 fixture set (GIS DXF/CRS and CAD DWG fixtures), T006 representative CAD regression, T008 CI cache and artifact budgets configuration, T016 synthetic independent controls and unsupported Z tests. All other tasks retain unchecked status. T005 partial: local transfer/ready measurements, worker per-request timings and network reference p95 not qualified. T007 partial: shell runtime only. T010 partial: existing artifact checksum and regressions pass, release rollback not qualified. T011/T015/T019/T021/T022/T028/T038/T039/T040/T041 partial evidence only, not acceptance. T004 and shared-adapter implementation gates remain blocked by reviewed R031 merge, not by lack of user permission. No production deployment or changes to shared runtime.
