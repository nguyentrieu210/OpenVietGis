# Tasks: Unified Spatial Workspace
## Setup
- [x] T001 Create isolated worktree and map stable roadmap IDs in ROADMAP.md.
- [x] T002 Generate spec, plan, contract and validation artifacts in specs/022-spatial-workspace/.
## Foundation
- [x] T003 Test scope identity, transition safety, blocked edits/unavailable capabilities and stale completions in packages/spatial-workspace/tests/session.test.mjs.
- [x] T004 Implement engine-neutral session state and mode transition functions in packages/spatial-workspace/src/session.ts.
## US1 — Switch modes
- [ ] T005 [US1] Connect a capability-aware accessible mode switch in apps/web-next/src/SpatialModeSwitch.tsx after reviewed 020 baseline.
- [ ] T006 [US1] Bind active host context, draft guards, camera and selection in apps/web-next/src/LiveApp.tsx.
## US2 — Lifecycle
- [ ] T007 [US2] Bind lazy adapter loading, teardown, scope cancellation and error handling in apps/web-next/src/LiveApp.tsx.
- [ ] T008 [US2] Verify actual browser mode switching and failure recovery in apps/web-next/spatial-mode-smoke.mjs.
## US3 — Interoperability
- [ ] T009 [US3] Verify asset linkage, server authorization, CRS and real engine readiness in follow-on R012-A and R004-B contracts under specs/.
- [ ] T010 [US3] Validate reference building fixture in apps/web-next/spatial-mode-smoke.mjs once adapters are ready.
## Polish
- [ ] T011 Run unit evidence, typecheck, browser regressions, and document results in specs/022-spatial-workspace/evidence.md.
- [ ] T012 Run full convergence and review; gate promotion in specs/022-spatial-workspace/evidence.md.
Dependencies: T001→T002→T003→T004→T005→T006→T007→T008; T009→T010, after T007; T011–T012 after verification. MVP: foundational contract and US1; 020 merge prerequisite blocks host wiring.
