# OpenVietGIS v1.0 independent integration preflight — 2026-11-23

Scope: isolated staging source only. No promotion, production migration, live tenant mutation, OAuth client registration, or certification.

## Pinned inputs and evidence

| Input | Commit |
| --- | --- |
| Beta Studio / partner ecosystem snapshot | d5f58da |
| AI Copilot remote MCP candidate | 65b1198 |
| Spatial Workspace CAD/3D candidate | efeab70 |
| Combined source snapshot | 3814361dbde536f91534491d20fa0ea6b3556a96 |

Both pinned merges completed without content conflicts. Shared main and active feature worktrees were not mutated.

| Check at combined snapshot | Result |
| --- | --- |
| Frozen offline dependency install (Node 26, pnpm 12.21.0) | PASS |
| web-next TypeScript and production Vite build | PASS (1.80 MB main bundle warning; ~511 KB gzip) |
| Spatial workspace logic and georeference + footprint tests | PASS 28/28 |
| MCP isolated fixture tests | PASS 27/27 |
| Architecture, docs navigation, API contract checks | PASS |
| Scope check | FAIL — rewrite renderer imports bypass the previously accepted MapEngine adapter, plus an initial hard-coded module example in Developer Portal |
| Exact release gate | BLOCKED — accepted profile, independent pinned verification and mandatory certified releases absent |

MCP tests use fixture credentials; not actual Claude OAuth authorization. Spatial workspace package tests are not proof that the mode selector, lifecycle or engine adapters are integrated in the production host. Neither feature is production-ready merely because merging their source succeeded.

## Remediation

- DONE in this branch: removed the hard-coded vertical endpoint from the core Developer Portal; uses a project-scoped layers endpoint as the generic example. The approved-address integration remains documented under its own section in docs/developer-integrations.md. Frontend typecheck passed after the change; the marker no longer appears in scope diagnostics.
- Do not silence the renderer guard with a blanket path exception. Reconcile the new workspace renderer architecture against the accepted MapEngine contract and record approved boundaries plus negative tests before passing the scope check.
- Wire Spec 022 mode host to the reviewed Studio baseline, secure project asset linkage and perform browser lifecycle/dirty-edit/revocation tests.
- Complete Spec 021 scoped production OAuth configuration and authorized real-project integration, including denial and revocation, before enabling the endpoint.
- Complete exact candidate integration/production-profile checks, cross-tenant access, backup + artifact recovery, large data benchmarks and required independent human observation gates.
- Keep Model Builder, raster/remote sensing, and Layout Designer as existing candidate capabilities; expand only after exact source acceptance.

The initial combined browser run exceeded its deadline. Subsequent independent AI Copilot browser testing on the current workspace route reproduced an out-of-viewport Connect button caused by the scrollable toolbar clipping the popup. Both first-party stylesheet copies now use a fixed, bounded, scrollable viewport panel. Rebuilt and retested: PASS on synthetic API fixtures for service disabled, restricted project, authenticated session (no copied token), denial, actual map navigation/view, and disconnect. This is not real Keycloak/Claude OAuth acceptance. Independent Layout Designer browser test on the workspace route completed successfully: two independent map cameras, save v1/v2, reopen, exported A4 landscape PDF MediaBox 841.8898 × 595.2756 pt and 3508 × 2480 PNG with verified 300 DPI pHYs metadata. The fixture uses 200 mock GIS features; artifacts and RESULT:0 at /tmp/ovg-v1-layout-gyyyPy.

## Combined candidate verification

On combined commit 60267e9 (including spatial integration d255abf), scope checker and 7/7 guard tests, 28/28 spatial unit tests, package/frontend TypeScript, Vite production build and integrated GIS → 3D → CAD → GIS browser fixture all PASS. The browser verifies actual WebGL rendering, CAD upload/revision UI and authorization revocation using intercepted API fixtures. The combined clean offline frozen install remains blocked by missing pnpm registry metadata; build used existing matching dependency versions. Alembic has one head, 0029_cad_documents; no production migration was run. Production OAuth, real authorized workflows, clean installation and mandatory exact release certifications remain open.

The preflight is deliberately fail-closed. No readiness claim or public release is authorized by this record.
