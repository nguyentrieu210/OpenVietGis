# Tasks: Unified Spatial Workspace
## Setup
- [x] T001 Create isolated worktree and map stable roadmap IDs in ROADMAP.md.
- [x] T002 Generate spec, plan, contract and validation artifacts in specs/022-spatial-workspace/.
## Foundation
- [x] T003 Test scope identity, transition safety, blocked edits/unavailable capabilities and stale completions in packages/spatial-workspace/tests/session.test.mjs.
- [x] T004 Implement engine-neutral session state and mode transition functions in packages/spatial-workspace/src/session.ts.
## US1 — Switch modes
- [ ] T005 [US1] Connect the implemented capability-aware accessible mode switch `packages/spatial-workspace/src/ModeSwitch.tsx` to apps/web-next after reviewed 020 baseline. Component Chromium fixture passed; host not wired.
- [ ] T006 [US1] Bind active host context, draft guards, camera and selection in apps/web-next/src/LiveApp.tsx.
## US2 — Lifecycle
- [ ] T007 [US2] Bind implemented `packages/spatial-workspace/src/coordinator.ts` to real adapters in apps/web-next/src/LiveApp.tsx; lifecycle unit suite passed, host wiring remains.
- [ ] T008 [US2] Verify actual browser mode switching and failure recovery in apps/web-next/spatial-mode-smoke.mjs.
## US3 — Interoperability
- [ ] T009 [US3] Adapter candidate implemented: `packages/spatial-workspace/src/cad-georeference.ts`, `cad-embed.ts`, `footprint3d.ts`, `maplibre3d.ts`; see `contracts/cad-3d-adapters.md`. CAD protocol fixture and real MapLibre 3D fixture passed. Project asset linkage, deployed CAD engine isolation and server authorization still pending.
- [ ] T010 [US3] Validate reference building fixture in apps/web-next/spatial-mode-smoke.mjs once adapters are ready.
## Polish
- [ ] T011 Run unit evidence, typecheck, browser regressions, and document results in specs/022-spatial-workspace/evidence.md.
- [ ] T012 Run full convergence and review; gate promotion in specs/022-spatial-workspace/evidence.md.
Dependencies: T001→T002→T003→T004→T005→T006→T007→T008; T009→T010, after T007; T011–T012 after verification. MVP: foundational contract and US1; 020 merge prerequisite blocks host wiring.
