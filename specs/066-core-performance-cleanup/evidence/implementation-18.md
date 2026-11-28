# 066 implementation slice 18 — CAD dirty-state recovery, bounded viewport and native edit controls

Parent branch candidate before this slice: 56d0daab07c0499d306a1b65e50608dbd86655eb.

## T017 — CAD route restore and accurate dirty state
- Internal DXF editor marks the workspace busy only while the document is dirty or a save is active. A clean opened document can switch GIS -> CAD -> GIS without a false unsaved-change gate.
- Unsaved native DXF drafts are stored in IndexedDB under an exact account/organization/project/document/version key, pinned to the server SHA-256 and limited to 4 MiB / seven days. A stale SHA/version/scope draft is rejected.
- Autosave is delayed 250 ms; an exact successful server save deletes the previous draft, advances the pinned version/SHA, resets undo/redo and establishes the new clean baseline.
- Crash-like browser-page replacement is exercised without editor cleanup. The persisted version-2 draft is found on the new page, CAD mode is restored, the exact document auto-opens, the edited X2=25 coordinate is restored and can be saved as version 3.
- Session expiry/actor transition clears both spatial cache and CAD drafts belonging to the previous actor. Focused offline Chromium suite: 4/4 PASS.
- Native browser smoke PASS: create DXF, draw LINE/CIRCLE, save v2, clean mode switch, edit, undo, redo, scoped draft recovery, save v3 and preserve v1 bytes.

This closes T017. The saved workspace mode remains a UI preference only; authorization is revalidated before CAD activation.

## T030 — CAD selection/snap/culling/undo/autosave profile
- Native DXF supports bounded 50-step undo/redo history and viewport culling before SVG rendering. Source DXF text is rewritten only for supported editable entities; imported unsupported/synthetic CAD remains fail-closed/read-only.
- Exact native-DXF unit suite PASS 4/4, including source preservation, unsafe input rejection and exact 100k viewport-culling result.
- Synthetic CPU benchmark (Node 26.13.1, explicit GC where available) verifies exactly 101 visible lines at 10k/100k/1m. Final p95: 145.73 ms / 163.96 ms / 770.39 ms. The 1m number is a CPU lab result, not an interactive frame-rate claim.
- Existing smart screen snap is profiled against 100,000 segments: midpoint result is correct; median 43.06 ms, p95 165.39 ms.
- Production web build, including TypeScript, PASS; offline static shell generated successfully.
- This closes T030's profile/accuracy requirement. Whole-browser/GPU/load soak remains T044.

No production service, database or user CAD document was modified.
