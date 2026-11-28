# 066 implementation slice 18 — CAD dirty-state recovery, culling and edit controls

Parent candidate: 56d0daab07c0499d306a1b65e50608dbd86655eb.

## T017 — restore CAD route and dirty state accurately
- Native DXF editor now reports busy only while the document is actually dirty or saving. Merely opening an untouched document does not block GIS/CAD mode switching.
- Dirty DXF drafts are stored in IndexedDB under account/org/project/document/version with source SHA-256, 4 MiB maximum and seven-day expiry. Drafts are rejected on scope/version/SHA mismatch.
- Session actor changes, explicit logout and session expiry clear that actor's CAD drafts alongside spatial cache invalidation.
- Browser smoke creates v1, edits/saves v2, verifies a clean editor switches GIS→CAD without warning, edits v2, undo/redo, waits for scoped draft persistence, closes the tab without running beforeunload cleanup, opens a new tab in the same browser context, automatically reopens v2 and restores the dirty draft, saves v3, then verifies the draft is deleted.
- Original source version bytes remain unchanged and the saved v3 geometry contains the intended coordinate edit.
- This closes T017's untouched/dirty/recovery acceptance.

## T030 — native CAD selection/culling/undo/autosave profile
- Native DXF viewport rendering now filters shapes against the current SVG view before creating DOM nodes. Selection options use the same visible set.
- Undo and redo retain a bounded history of 50 text states. Edits preserve the parser's fail-closed behavior and source structure.
- Existing snap math remains shared from spatial-workspace; a 100k-segment smart-snap CPU stress fixture returns the expected midpoint.
- Node CAD regression PASS 4/4: create/edit source preservation, imported synthetic CAD read-only behavior, unsafe/unsupported fail-closed behavior and 100k-shape exact viewport culling.
- TypeScript PASS on the exact working tree. Browser native DXF smoke PASS end-to-end.
- Synthetic CPU benchmark (Node 26.13.1, explicit GC when available): culling p95 120.08 ms at 10k shapes, 443.34 ms at 100k, 2867.29 ms at 1m; smart snap over 100k segments p95 641.08 ms. Exact visible IDs and snap result are asserted.
- The 1m shape case is a stress ceiling outside the native editor's 4 MiB DXF input limit and is not presented as a production frame-time target. Final browser/GPU/load qualification remains T044.
- This closes T030's profile/evidence requirement without claiming unlimited CAD scale.

No production CAD service or stored drawing was modified.
