# Tasks: Unified Spatial Workspace
## Setup
- [x] T001 Create isolated worktree and map stable roadmap IDs in ROADMAP.md.
- [x] T002 Generate spec, plan, contract and validation artifacts in specs/022-spatial-workspace/.
## Foundation
- [x] T003 Test scope identity, transition safety, blocked edits/unavailable capabilities and stale completions in packages/spatial-workspace/tests/session.test.mjs.
- [x] T004 Implement engine-neutral session state and mode transition functions in packages/spatial-workspace/src/session.ts.
## US1 — Switch modes
- [x] T005 [US1] Connect capability-aware mode switch to apps/web-next from the committed preflight candidate; integrated host browser smoke passed.
- [ ] T006 [US1] Host context, draft guards, camera and persistent GIS selection wired; 3D pick identity displayed, bidirectional selection sync remains.
## US2 — Lifecycle
- [ ] T007 [US2] Coordinator bound to real GIS and 3D adapters and private CAD vault. Direct CAD engine adapter remains gated.
- [ ] T008 [US2] Integrated switching, CAD versions and revocation smoke passed in apps/web-next/spatial-mode-smoke.mjs; full failure-recovery regression remains.
## US3 — Interoperability
- [ ] T009 [US3] CAD protocol/georeference, actual 3D, project vault and server permission checks implemented/verified. Deployed CAD engine isolation and linked georeferenced geometry import remain gated.
- [x] T010 [US3] Reference polygon building fixture rendered in integrated browser with explicit height field and flat zero base.
## Polish
- [ ] T011 Run unit evidence, typecheck, browser regressions, and document results in specs/022-spatial-workspace/evidence.md.
- [ ] T012 Run full convergence and review; gate promotion in specs/022-spatial-workspace/evidence.md.
Dependencies: T001→T002→T003→T004→T005→T006→T007→T008; T009→T010, after T007; T011–T012 after verification. MVP: foundational contract and US1; 020 merge prerequisite blocks host wiring.
