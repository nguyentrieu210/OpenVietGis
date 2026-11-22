# Implementation Plan: Unified Spatial Workspace
Branch feature/022-spatial-workspace | Parent R004-A | Spec: spec.md
## Summary
Implement an engine-neutral mode transition state machine, then wire it into the reviewed Spec 020 studio. CAD bridge and 3D renderer are separate follow-on slices.
## Technical Context
React/TypeScript/Vite; actual current 2D engine is MapLibre (ADR-006), not OpenLayers. Keep existing project/user/tenant and selection in host. No schema, authentication or module changes for this increment. New dependency-free shared workspace controller under packages/spatial-workspace; Node native test runner on VPS. Spec 020 currently untracked on shared main; do not edit or copy dirty files into this branch.
## Constitution Check
I/VI/VII: capability-driven adapters, no domain plugin dependencies; II/IV: native artifact intact and explicit georeference; III/IX: scope in all transitions, fail closed, existing server authorization mandatory; V: no mutation or draft discard; VIII: bounded workflow and measurable acceptance. No constitution amendments.
## Structure
```
packages/spatial-workspace/src/session.ts
packages/spatial-workspace/tests/session.test.mjs
apps/web-next/src/LiveApp.tsx (after 020 baseline)
apps/web-next/src/SpatialModeSwitch.tsx (after 020 baseline)
specs/022-spatial-workspace/contracts/mode-host.md
```
## Phases
0: confirm stable IDs, current engine and 020 dependency; 1: pure transition contract, tests; 2: integrate with actual reviewed shell; 3: verify CAD authorization and real 3D adapter capability in dedicated child slices. No dummy engine enabled as real capability. Browser checks and convergence required before merge. Keep rollout behind availability configuration and rollback by disabling new mode hooks.
