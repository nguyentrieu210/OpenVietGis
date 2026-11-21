# Implementation Plan: House Numbering Plugin
**Branch**: `spec/019-house-numbering-plugin` | **Date**: 2026-11-21 | **Spec**: [spec.md](spec.md)
**Input**: Child R014, [Grill](../../docs/grill-house-numbering-plugin.md).

## Summary
Build official project-scoped module, preserving legacy workflows but consuming Core spatial, identity, changes, processing, field and export contracts. Deliver daily address inspection/assignment first, then three-source reconciliation, field and handover. Split domain computation from persist/UI so pure rules can be tested now without claiming integrated runtime.

## Technical Context
- **Language/Version**: existing Python 3.13 and TypeScript/React; Node 26/pnpm baseline.
- **Dependencies**: FastAPI/SQLAlchemy, PostGIS, public Core SDK and command contracts, MapLibre adapter via Core. Use existing locked dependencies; add libraries only after ADR/review.
- **Storage**: Core spatial features for geometry, module-scoped relational data with tenant/project keys, module-owned migrations, RLS, explicit revisions; protected private records/files separated.
- **Testing**: pytest pure rules and real PostGIS integrations, Playwright real-browser workflows, architecture tests, recovery and benchmarks.
- **Platform**: web and responsive field workspace, Linux/VPS and portable deployment.
- **Performance**: SC-010 10k inspection p95 <3s, 100k/50 concurrent benchmark, SC-006/007.
- **Constraints**: no vertical import in Core; atomic writes, no PII in standard exports, no real PII until approval; rules not legal authority.
- **Scale**: 100k buildings/project reference profile, 13 target workstreams.

## Constitution Check
| Principle | Design evidence | Gate |
|---|---|---|
| I platform/module isolation | module-owned code, generic registration, no Core vertical import | architecture test |
| II data portability | explicit supported export, redacted and restricted packages | round-trip |
| III tenant isolation | server project auth and RLS, segregate PII | cross-tenant/permission negatives |
| IV spatial correctness | explicit source/storage/display CRS and verified transforms | control points |
| V attributable changes | preview revision, atomic apply, proposal history, immutable audit | simultaneous edit/replay |
| VI capability model | generic command/UI contribution and capability gate | inactive/absent capability disabled |
| VII web-native engine boundaries | Core map/offline/export contracts, plugin domain only | browser and mobile |
| VIII workflow value | independent vertical slices with real UI acceptance | executable tests |

Design check passes principles; **integration readiness blocked**. Verified main c5cc5a9 lacks R004–R014 contracts. VPS candidate c6d449f has R014 generic register/list/activate/create/list but not deactivation, revisioned updates, typed geo constraints or end-to-end command execution. Audit 018 G001–G010 confirms dependency and release gaps. Do not merge/deploy this branch or fake those gates. Pure module-domain work with no runtime dependencies is permitted; wait for verified prerequisite promotion before integration. No exception to constitution.

## Project Structure
```text
specs/019-house-numbering-plugin/
  spec.md plan.md research.md data-model.md quickstart.md tasks.md
  contracts/extension-contract.md
  checklists/requirements.md
modules/house_numbering/
  domain.py                # pure rule engine (no Core imports)
  manifest.json            # existing v1 compatibility; proposed upgrade after promotion
  application.py           # commands/ports, after gate
  persistence.py           # module-owned scoped repository, after gate
  migrations/              # module-owned lifecycle, after gate
  frontend/                # generic UI contribution, after gate
tests/modules/
  test_house_numbering_domain.py
  test_house_numbering_integration.py
tests/e2e/house-numbering.spec.ts
```
Generic Core capabilities required by modules go through a linked Core remediation spec, never a named import. The existing reference module in candidate integration stays unchanged until reviewed reconciliation.

## Complexity Tracking
No constitutional deviations; module-specific database constraints warranted by atomic uniqueness and audit. No extra service/deployment.
