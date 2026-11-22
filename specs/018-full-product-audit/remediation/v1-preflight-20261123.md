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

A candidate browser run of AI Copilot and Layout Designer using a loopback Vite preview and synthetic API fixtures exceeded the 120-second connector deadline. Complete output was not captured; neither browser check is counted as passing. Repeat each independently with captured results.

The preflight is deliberately fail-closed. No readiness claim or public release is authorized by this record.
