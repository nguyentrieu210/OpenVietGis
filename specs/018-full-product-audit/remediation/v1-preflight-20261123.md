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

## Recovery and access remediation

- Recovery previously inventoried only CAD conversion artifacts, so immutable CAD document revisions added in 0029 could be silently omitted from the required file set. Inventory now includes every CAD document version as a mandatory hash/size-checked reference and preserves pre-0029 compatibility. A missing version table on the current schema fails closed.
- Recovery authority now fingerprints the complete scoped GIS membership/group/group-member/rule envelope. If historical and current GIS policy differ or the newer fingerprint is missing on either side, all human authority is quarantined pending independent re-establishment. Older schemas retain their existing snapshot representation; a pre-fix manifest on a newer schema with an incomplete authority fingerprint cannot pass exact inventory verification.
- Ruff PASS; 37 recovery/authority unit tests PASS, including old/new schema inventory and missing/changed GIS fingerprint. On a newly created labeled loopback-only PostGIS 18.7 container (`ovg-v1-recovery-20261123b`, port 32812), database migrated from zero to 0029; targeted CAD vault, layout and GIS access/batch tests 4/4 PASS. No shared or live database was migrated. Test result /tmp/ovg-v1-recovery-tFN5ZB/run.log.
- Actual synthetic signed PostgreSQL+artifact backup: missing CAD bytes rejected before backup directory creation, two immutable CAD versions inventoried. Isolated authenticated restore PASS (7.79s local synthetic drill), both drawing files byte-exact, schema 0029, no historical users/memberships/grants/sessions/credentials active, no runtime CONNECT or activation. This is not an approved production RPO/RTO or authorization reconciliation; no current production authority source used. Private fixture/backup/restore evidence retained under /tmp/ovg-v1-recovery-tFN5ZB. A second isolated restore using a separately attested synthetic authority snapshot after revoking an active GIS rule also PASS: base membership/grant envelope unchanged, GIS fingerprint changed, no historical users/memberships/grants preserved. No production recovery certification.

## Follow-on exact candidate checks

- On recovery fix candidate d7bcfd4: accepted runtime scope/MapEngine boundary, architecture, API contracts, documentation all PASS. 60 release-gate, secret-surface, scope and tenant-timing checker unit tests PASS. Authorization benchmark against the restricted runtime role on the disposable PostGIS database: 150 samples, p95 21.387ms (budget 50ms); all lifecycle phases within budget, next-request revocation and real project move tested.
- Separate sibling worktrees (not included in d7bcfd4): quality convergence aa1e472, CAD editor/georeference 219b3a3, MCP advanced workspace 77c06ab. Their evidence is branch-specific. Coordinate a pinned combined candidate with reviewed conflicts and repeat tests; never infer production acceptance from separate preview deployments. Do not alter active main or sibling worktrees. Read-only three-way previews: quality aa1e472 has no shared-path conflict with the recovery fix; CAD 219b3a3 overlaps LiveApp, SpatialWorkspace, package export and evidence (evidence conflict); MCP 77c06ab conflicts in first-party CSS (preserve viewport clipping fix). Team collaboration 1869613 is separately active and overlaps API main and LiveApp (conflict). Treat these as inputs for a coordinated merge, not verified combined source.
- Exact release gate still BLOCKED on d7bcfd4: accepted deployment profile, exact independent verification and mandatory R001/R002/R003/R004/R005/R006/R007/R011/R015 certifications absent.

The preflight is deliberately fail-closed. No readiness claim or public release is authorized by this record.
