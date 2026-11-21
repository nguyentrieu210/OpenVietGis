# Implementation Plan: App Shell & Identity UX

**Branch**: `spec/016-app-shell-identity-ux` | **Date**: 2026-11-21 | **Spec**: [spec.md](spec.md)

**Input**: Child frontend slice of [R002](../../ROADMAP.md#r002), using the accepted identity/session/tenancy contracts already present on verified main.

## Summary

Refactor the existing React SPA into a stable public/authenticated shell without changing backend identity or authorization semantics. Keep `/` public, add dedicated `/login`, make `/app` the authenticated dashboard entry, preserve protected deep-link return destinations, and migrate legacy authenticated URLs through safe redirects. Introduce a small OpenVietGIS UI layer (semantic CSS tokens + accessible primitives), a stable session boundary that does not unmount the authenticated shell during background `/me` checks, a persistent sidebar/topbar/account menu, Vietnamese-first labels, deterministic empty/error states and browser acceptance/visual regression.

## Technical Context

**Language/Version**: TypeScript 6; Node.js 26; React 19.

**Primary Dependencies**: Existing React 19, React Router 7.18.5, TanStack Query 5.107.1 and `@openvietgis/client-sdk`. No heavy component framework is planned. Native semantic controls and project-owned UI primitives are preferred; any new headless accessibility dependency requires normal dependency-age/security review.

**Storage**: Existing R002 server persistence remains unchanged. Browser persistence is limited to non-sensitive personal shell preferences (for example collapsed navigation); no OIDC/provider token or authorization snapshot may be stored client-side.

**Testing**: Existing Vitest/unit tests, TypeScript strict checking, ESLint, Vite production build, real Playwright + configured OIDC/Keycloak acceptance, deterministic screenshot regression for Landing/Login/Dashboard/Account Menu, and unchanged backend authorization/tenant suites.

**Target Platform**: Modern desktop browsers as primary; adaptive tablet shell at 768 px and wider. Mobile authenticated GIS parity is not part of this slice.

**Project Type**: Existing web SPA in the OpenVietGIS monorepo.

**Performance Goals**: Preserve the authenticated shell during background session checks; shell-only interactions should complete within a 100 ms p95 interaction budget on the reference browser after data is available. Record pre-change bundle/startup baselines and reject unexplained >15% regression in transferred application JavaScript or deterministic shell startup metrics.

**Constraints**: Vietnamese-first; light theme; no raw roadmap codes/UUIDs in primary navigation; WCAG 2.1 AA baseline for primary flows; external identity tokens remain outside browser storage; current CSRF/session semantics remain intact; public landing remains public even when a session exists; legacy deep links must preserve valid context.

**Scale/Scope**: Public Landing, Login, App Dashboard, persistent App Shell, Account Menu, Identity/Session surface, organization/project navigation and legacy-route migration. No new backend domain model, no new authorization model, no dark mode, and no GIS workspace redesign in this slice.

## Constitution Check

**Pre-design gate: PASS.**

- **Platform-first/module isolation**: shell is generic Core navigation; no vertical module semantics.
- **Open data/interoperability**: no data contract changes.
- **Tenant isolation**: UI visibility remains non-authoritative; protected reads/mutations continue to rely on server checks. The plan explicitly avoids weakening protected tenant query freshness to solve flicker.
- **Spatial correctness**: no CRS or geometry semantics change.
- **Version/audit**: no material data mutation model change.
- **Capability-driven architecture**: shell navigation can expose capability-aware destinations but does not invent capabilities.
- **Web-native/engine-agnostic**: long-lived React SPA is already accepted by ADR-003; public and authenticated shells remain independent of map renderer internals.
- **Workflow value**: completes the entry/dashboard/account workflow before visual polish expansion.

**Post-design gate: PASS.** No Constitution deviation is required.

## Project Structure

### Documentation (this feature)

```text
specs/016-app-shell-identity-ux/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── routes-and-redirects.md
│   ├── session-shell-state.md
│   └── navigation-and-account.md
├── checklists/requirements.md
└── tasks.md
```

### Source Code (repository root)

```text
apps/web/src/
├── app/
│   ├── App.tsx
│   ├── router.tsx
│   ├── session/
│   │   ├── SessionBoundary.tsx
│   │   └── session-state.ts
│   └── shell/
│       ├── AppShell.tsx
│       ├── AppNavigation.tsx
│       ├── Breadcrumbs.tsx
│       └── CommandPalette.tsx
├── ui/
│   ├── tokens.css
│   ├── global.css
│   └── primitives/
├── features/
│   ├── landing/LandingPage.tsx
│   ├── auth/
│   │   ├── LoginPage.tsx
│   │   ├── AccountMenu.tsx
│   │   └── IdentityPage.tsx
│   └── dashboard/AppDashboard.tsx
└── main.tsx

tests/e2e/
├── app-shell.spec.ts
├── auth-navigation.spec.ts
└── app-shell-visual.spec.ts
```

Existing organization/project/workspace/membership pages are migrated into the shell rather than rewritten as new backend features.

**Structure Decision**: Keep all implementation in the existing web package and existing SDK contracts. Add a small project-owned UI layer and explicit session/shell modules so authentication identity state, protected tenant data and visual shell concerns are not conflated.

## Delivery and Integration

1. Record exact verified-main baseline and current auth/browser regression status.
2. Implement design tokens/primitives and public Landing/Login without changing protected routes.
3. Introduce SessionBoundary state machine and tests before moving authenticated pages into AppShell.
4. Add canonical `/app` routes, safe `return_to` handling and legacy redirects.
5. Integrate dashboard/navigation/account menu incrementally.
6. Run deterministic visual regression, keyboard/accessibility checks and real browser auth/logout/deep-link acceptance.
7. Run full root validation and independent review before convergence.
8. Do not merge by overwriting any later GIS work; integration owner must reconcile this branch with the current verified frontend baseline.

## Complexity Tracking

No Constitution violation. The explicit session-shell boundary is necessary because current protected-data freshness rules and long-lived shell rendering have different security/UX responsibilities.
