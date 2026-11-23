# OpenVietGIS — Architect Review 2026-11-23

## Scope and evidence

**Snapshot**: base `maintenance/control-kit-20261123@d8be26a`, derived from `main@5baade9`.
**Mode**: source-level, read-only architecture review plus a narrow CI/documentation remediation on an isolated branch; not a production acceptance certificate.
**Production state**: previously verified release `v2026.11.23-5baade9`; no redeploy or database operations performed during this review.

## Architecture verdict

**Keep the modular monolith (ADR-002)**. The stack already has explicit domain/module boundaries, shared SDK contracts, separately deployable supporting services, and a valid path to grow the product without immediate microservice extraction. Prioritize a canonical client, enforce release checks, reduce UI orchestration coupling, and test official module failure modes.

## Findings and actions

### P0 — Production frontend absent from root validation build (fixed in this branch)

- **Evidence**: `package.json` root `validate:ts` originally built only `@openvietgis/web`; `infra/prod/Dockerfile.web` and the live release manifest build `@openvietgis/web-next`.
- **Risk**: Root validation could report green without ever bundling the actual public frontend, even though TypeScript may be checked by the recursive workspace step. The Dockerfile has a separate production build.
- **Change**: add `pnpm --filter @openvietgis/web-next build` to the root validation script, preserving legacy validation. Added a source-level invariant test.
- **Acceptance**: Node 26 + pinned pnpm build `apps/web-next` successfully and PR CI passes. **Production build NOT VERIFIED in this review.**

### P0 — Canonical frontend ownership unclear in architecture docs (fixed in this branch)

- **Evidence**: `docs/architecture.md` named `apps/web` the desktop-class shell. Release manifest selected `apps/web-next`; public `/` and `/app` serve its index. `infra/prod/Dockerfile.web` copies `apps/web-next/dist`.
- **Risk**: New contributions land in the legacy UI, and reviewers mistake a dev preview for production.
- **Change**: document the current canonical public frontend as `apps/web-next` and legacy/preview role of `apps/web`. **Do not delete apps/web** without route and feature parity validation.

### P1 — Frontend orchestration hotspot

- **Evidence**: `apps/web-next/src/LiveApp.tsx` is about 73 KiB, `SpatialWorkspace.tsx` about 43 KiB. `LiveApp.tsx` imports editing, processing, raster, CAD/3D, chat, permissions and plugin panels and manages session/org/project flows. `apps/web/src/app/router.tsx` uses React Router; current public `apps/web-next/src/main.tsx` selects pages via URL query parameters and `LiveApp` manages navigation with `history.replaceState`.
- **Risk**: High blast radius from unrelated feature changes; inconsistent navigation/session behavior and difficult targeted tests.
- **Action**: define stable page/feature contracts; peel off bounded responsibilities progressively (auth shell, project workspace, plugin host, map orchestration). Preserve existing deep links; add route regression tests first. Do not replace the whole UI in one change.

### P1 — Trusted official-module startup boundary

- **Evidence**: `apps/api/openvietgis_api/main.py` registers 43 HTTP router groups/lines and discovers official modules on startup via `domains/extensions/runtime.py`; the discovery function reads matching `manifest.json` + `runtime.py`, imports by convention, and raises on invalid runtime.
- **Risk**: Bad first-party plugin packaging or import can stop backend startup. This is a failure-mode observation, not an exploitable security bug.
- **Action**: test absent modules, malformed manifest, missing router and incompatible version in an isolated test app; decide between fail-closed policy and per-module quarantine with explicit operator alert. Do not silently skip security-critical extension registration.

### P1 — Mixed module lifecycle requires explicit classification

- **Evidence**: `modules/house_numbering` has both manifest and runtime. `modules/land_asset` has manifest/workflow but no `runtime.py`; current official runtime discovery requires both files.
- **Risk**: Documentation/marketplace can imply a second live official runtime although the registration code will not load it. This is a packaging distinction, not proof the land-asset module cannot be used elsewhere.
- **Action**: publish capability states (`manifest-only`, `runtime-active`, `project-enabled`, `verified-live`). Include module compatibility assertions in CI.

### P1 — Architecture and security evidence are separate

- **Evidence**: `tools/check_architecture.py` enforces literal import relationships; `tests/architecture/test_architecture.py` has explicit invalid/valid fixtures. The source-level rule reported **OK** and the five architecture tests passed. Release hashing and API executable pinning also passed in Control Kit.
- **Risk**: Static checks do not prove module permissions, runtime tenant isolation, DB migration revision or GIS/CAD/3D E2E workflows.
- **Action**: require explicit runtime evidence for tenant/project isolation, plugin permissions, migrations and browser flows before marking release accepted.

## Updated target boundaries

```text
apps/web-next (canonical live web)     apps/web (legacy/preview, not yet removable)
             |                                  |
             +----------- packages/client-sdk --+
                                |
                         API v1 adapters
                                |
                  application / domain ports
                                |
                    persistence + infrastructure
                                ^
                                |
      official module registry -> trusted module runtime
                                |
                  modules/house_numbering

packages/spatial-workspace: shared CAD/3D/GIS implementation,
consumed by canonical frontend (UI-side, not a service by itself).

MCP: separate optional AI integration surface with independent
authentication/authorization and explicit release provenance.
```

## Mandatory gates before consolidation

1. **CI:** successful production frontend build (`web-next`) plus prior legacy regression checks.
2. **Auth:** role/tenant/project isolation and plugin authorization E2E using test accounts.
3. **Modules:** absent/bad official modules and version compatibility behavior tested.
4. **Data:** actual migration revision compared to immutable release, rollback compatibility analyzed.
5. **GIS/CAD/3D:** smoke tests exercising editing, export, 3D selection and plugin activation.
6. **Release:** manifest, web assets, backend source, server runtime and all component revisions pinned.

## Commands run

- `python3 tools/check_architecture.py` → **PASS**.
- `python3 tools/check_docs.py` → **PASS**.
- `python -m pytest -q tests/architecture` → **5 tests PASS**.

**Not run:** full frontend build and full CI/E2E, production migration introspection, live authorization testing. Those require a test-safe environment and controlled resources.

## Outcome

Architect review produced a narrow root-validation fix and documentation alignment. This report does not authorize a deploy, branch deletion, migration or module quarantine; the branch must be reviewed and CI-tested before merge.
