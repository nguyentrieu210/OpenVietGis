---
description: "Dependency-ordered implementation tasks for OpenVietGIS App Shell & Identity UX"
---

# Tasks: App Shell & Identity UX

**Input**: Design documents from `/specs/016-app-shell-identity-ux/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Required by the specification for session stability, deep links, logout, accessibility, responsive behavior and visual regression. Write targeted tests before the implementation they prove.

## Phase 1: Setup

**Purpose**: Establish the shared frontend visual layer and reproducible baseline.

- [ ] T001 Record exact implementation baseline, supported Node/pnpm versions, current web bundle metrics and existing auth/browser test status in `specs/016-app-shell-identity-ux/quickstart.md`
- [ ] T002 [P] Create semantic light-theme design tokens and spacing/typography/focus variables in `apps/web/src/ui/tokens.css`
- [ ] T003 [P] Create global reset, skip-link, reduced-motion and responsive shell foundations in `apps/web/src/ui/global.css`
- [ ] T004 Wire `apps/web/src/ui/tokens.css` and `apps/web/src/ui/global.css` exactly once from `apps/web/src/main.tsx`
- [ ] T005 [P] Add accessible shared Button/IconButton/StatusMessage primitives in `apps/web/src/ui/primitives/Button.tsx`, `apps/web/src/ui/primitives/IconButton.tsx`, and `apps/web/src/ui/primitives/StatusMessage.tsx`

**Checkpoint**: Shared styling exists without changing routing/auth behavior.

## Phase 2: Foundational — Session and Shell Contracts

**Purpose**: Separate stable application-session chrome from strict protected tenant-data freshness.

- [ ] T006 [P] Add failing state-transition tests for `SessionShellState.phase`, `SessionShellState.account`, `SessionShellState.lastConfirmedAt`, and `SessionShellState.errorKind` as defined in `specs/016-app-shell-identity-ux/data-model.md` to `apps/web/src/app/session/session-state.test.ts`
- [ ] T007 Implement the tested `SessionShellState` state machine and transition helpers in `apps/web/src/app/session/session-state.ts` using the canonical fields from `specs/016-app-shell-identity-ux/data-model.md`
- [ ] T008 [P] Add failing tests for safe `ReturnDestination.pathname`, `ReturnDestination.search`, and `ReturnDestination.hash` validation from `specs/016-app-shell-identity-ux/data-model.md` in `apps/web/src/app/session/return-destination.test.ts`
- [ ] T009 Implement same-origin relative return-destination parsing/serialization in `apps/web/src/app/session/return-destination.ts` using `ReturnDestination.pathname`, `ReturnDestination.search`, and `ReturnDestination.hash` from `specs/016-app-shell-identity-ux/data-model.md`
- [ ] T010 Add failing component tests proving background `/me` checks do not unmount a previously authenticated shell and confirmed 401 does clear it in `apps/web/src/app/session/SessionBoundary.test.tsx`
- [ ] T011 Implement `SessionBoundary` and session context in `apps/web/src/app/session/SessionBoundary.tsx` while leaving strict tenant/resource reads on existing `protectedQueryOptions`
- [ ] T012 Implement authorization-loss cache/session coordination for confirmed protected-request auth loss in `apps/web/src/app/session/session-auth-loss.ts` and integrate only the necessary hook points in `apps/web/src/app/protected-query.ts`
- [ ] T013 [P] Implement `ShellPreference.navigationCollapsed` from `specs/016-app-shell-identity-ux/data-model.md` with non-sensitive local persistence in `apps/web/src/app/shell/shell-preference.ts`
- [ ] T014 Implement `AppNavigationItem.id`, `AppNavigationItem.labelKey`, `AppNavigationItem.destination`, `AppNavigationItem.scope`, and `AppNavigationItem.availability` from `specs/016-app-shell-identity-ux/data-model.md` in `apps/web/src/app/shell/navigation-model.ts`
- [ ] T015 Build the reusable authenticated chrome in `apps/web/src/app/shell/AppShell.tsx`, `apps/web/src/app/shell/AppNavigation.tsx`, and `apps/web/src/app/shell/Breadcrumbs.tsx` without exposing roadmap IDs or UUIDs as primary labels
- [ ] T016 Add the shared command-palette foundation and navigation command registry in `apps/web/src/app/shell/CommandPalette.tsx` and `apps/web/src/app/shell/command-registry.ts`

**Checkpoint**: Foundational session/shell architecture is independently testable before page migration.

## Phase 3: User Story 1 — Enter OpenVietGIS without authentication flicker (Priority: P1) MVP

**Goal**: Public Landing + dedicated Login + stable authenticated transition + deep-link return.

**Independent Test**: Signed-out public landing → login → provider sign-in → intended protected destination, then focus/reconnect/background revalidation with no public/loading flash.

- [ ] T017 [P] [US1] Add failing Playwright coverage for public `/`, dedicated `/login`, protected deep-link return and invalid external return fallback in `tests/e2e/auth-navigation.spec.ts`
- [ ] T018 [P] [US1] Add failing Playwright DOM/screenshot sequence asserting no Landing/full-page-loading flash during authenticated focus/revalidation in `tests/e2e/app-shell.spec.ts`
- [ ] T019 [P] [US1] Implement the public product landing content and accessible CTA structure in `apps/web/src/features/landing/LandingPage.tsx` and `apps/web/src/features/landing/landing.css`
- [ ] T020 [US1] Replace the embedded login panel with a dedicated provider-neutral Login page using validated `return_to` in `apps/web/src/features/auth/LoginPage.tsx`
- [ ] T021 [US1] Rebuild route guards/canonical public-authenticated route separation for `/`, `/login`, and `/app` in `apps/web/src/app/router.tsx`
- [ ] T022 [US1] Remove or reduce legacy `FreshAccount` responsibilities so page components consume `SessionBoundary` without full-screen refetch replacement in `apps/web/src/features/auth/FreshAccount.tsx`
- [ ] T023 [US1] Make login/provider errors announced and keyboard accessible in `apps/web/src/features/auth/LoginPage.tsx` and `apps/web/src/ui/primitives/StatusMessage.tsx`

**Checkpoint**: US1 is demoable independently as the stable public/auth gateway.

## Phase 4: User Story 2 — Work from a clear authenticated dashboard (Priority: P1)

**Goal**: Human-readable `/app` dashboard with organizations, projects, empty states and primary actions.

**Independent Test**: User with organizations/projects can identify and open a project without UUIDs; user with no projects gets a useful empty state.

- [ ] T024 [P] [US2] Add failing dashboard tests for organization/project cards, no-project empty state and absence of primary UUID labels in `apps/web/src/features/dashboard/AppDashboard.test.tsx`
- [ ] T025 [P] [US2] Add failing browser flow for `/app` organization/project navigation in `tests/e2e/app-shell.spec.ts`
- [ ] T026 [US2] Implement `AppDashboard` using existing organization/project SDK reads in `apps/web/src/features/dashboard/AppDashboard.tsx` and `apps/web/src/features/dashboard/dashboard.css`
- [ ] T027 [US2] Migrate organization switch/list presentation into the authenticated shell in `apps/web/src/features/organizations/OrganizationHome.tsx`
- [ ] T028 [US2] Migrate project list/create presentation into the authenticated shell with readable workspace/status labels in `apps/web/src/features/projects/ProjectPage.tsx`

**Checkpoint**: Dashboard orientation is independently usable without the GIS workspace redesign.

## Phase 5: User Story 3 — Manage account and sign out from anywhere (Priority: P1)

**Goal**: Consistent account/session/invitation/logout access from every authenticated shell.

**Independent Test**: Open account menu from multiple authenticated routes, sign out, then confirm protected access is gone.

- [ ] T029 [P] [US3] Add failing logout/account-menu tests covering confirmed logout and ambiguous failure revalidation in `apps/web/src/features/auth/AccountMenu.test.tsx`
- [ ] T030 [P] [US3] Add Playwright logout-from-deep-route coverage in `tests/e2e/auth-navigation.spec.ts`
- [ ] T031 [US3] Implement `AccountMenuState` safe display/account/session/sign-out behavior from `specs/016-app-shell-identity-ux/data-model.md` in `apps/web/src/features/auth/AccountMenu.tsx`
- [ ] T032 [US3] Restyle and route the identity/session surface under the canonical app shell in `apps/web/src/features/auth/IdentityPage.tsx`
- [ ] T033 [US3] Move invitation acceptance/entry links into canonical app navigation without changing invitation contracts in `apps/web/src/features/invitations/InvitationAcceptPage.tsx` and `apps/web/src/features/invitations/InvitationIssuePanel.tsx`

**Checkpoint**: Session control is consistently reachable and security behavior is preserved.

## Phase 6: User Story 4 — Navigate consistently across the application (Priority: P2)

**Goal**: Persistent sidebar/topbar/breadcrumbs, command search, and safe legacy-route migration.

**Independent Test**: Visit Overview, Projects, Workspaces, Membership and Account; active location remains clear and legacy bookmarks redirect safely.

- [ ] T034 [P] [US4] Add route-contract tests for every legacy-to-canonical mapping in `apps/web/src/app/router.test.tsx`
- [ ] T035 [P] [US4] Add Playwright coverage for persistent shell navigation/breadcrumb active state in `tests/e2e/app-shell.spec.ts`
- [ ] T036 [US4] Implement legacy redirects from `contracts/routes-and-redirects.md` in `apps/web/src/app/router.tsx`
- [ ] T037 [US4] Finish scope-aware primary navigation and breadcrumb mapping in `apps/web/src/app/shell/AppNavigation.tsx` and `apps/web/src/app/shell/Breadcrumbs.tsx`
- [ ] T038 [US4] Register initial shell commands for Dashboard, Projects, Workspaces, Members/Access and Account in `apps/web/src/app/shell/command-registry.ts`

**Checkpoint**: US4 works independently as an information-architecture migration layer.

## Phase 7: User Story 5 — Coherent Vietnamese-first responsive interface (Priority: P2)

**Goal**: Vietnamese labels, keyboard/accessibility baseline, tablet adaptation and deterministic visual quality.

**Independent Test**: Keyboard + screenshot + width checks on Landing, Login, Dashboard and Account Menu.

- [ ] T039 [P] [US5] Add deterministic screenshot baselines for Landing, Login, Dashboard and Account Menu in `tests/e2e/app-shell-visual.spec.ts`
- [ ] T040 [P] [US5] Add keyboard/focus/skip-link and 768/1024/1440 width acceptance checks in `tests/e2e/app-shell-accessibility.spec.ts`
- [ ] T041 [US5] Replace remaining raw English/internal enum/roadmap labels in affected shell/auth/organization/project/workspace/membership components under `apps/web/src/features/`
- [ ] T042 [US5] Complete sidebar collapse/tablet responsive behavior and 44 px touch-target rules in `apps/web/src/app/shell/AppShell.tsx`, `apps/web/src/app/shell/AppNavigation.tsx`, and `apps/web/src/ui/global.css`
- [ ] T043 [US5] Add reduced-motion and focus-visible handling required by the approved design system in `apps/web/src/ui/global.css`

**Checkpoint**: All five user stories are independently demonstrable.

## Phase 8: Polish & Cross-Cutting Validation

- [ ] T044 [P] Compare final web bundle/startup metrics with T001 baseline and record justified deltas in `specs/016-app-shell-identity-ux/quickstart.md`
- [ ] T045 Run all feature quickstart scenarios and record exact browser/runtime/commit evidence in `specs/016-app-shell-identity-ux/quickstart.md`
- [ ] T046 Run `pnpm --filter @openvietgis/web test`, lint, typecheck, build, targeted Playwright and unchanged R002 security/tenant regressions; record results in `specs/016-app-shell-identity-ux/quickstart.md`
- [ ] T047 Run full repository validation and resolve any cross-feature route/link regressions before marking tasks complete in `specs/016-app-shell-identity-ux/tasks.md`
- [ ] T048 Perform independent code review and Spec Kit convergence against `specs/016-app-shell-identity-ux/spec.md`, then document any remaining human/external evidence honestly in `specs/016-app-shell-identity-ux/quickstart.md`

## Dependencies & Execution Order

- Phase 1 → Phase 2 is mandatory.
- US1, US2 and US3 are all P1, but US1 should land first because canonical routing/session shell is the foundation consumed by US2/US3.
- After US1, US2 and US3 can proceed in parallel on distinct feature files.
- US4 depends on the canonical routes introduced by US1 and integrates the completed shell destinations from US2/US3.
- US5 can begin visual test scaffolding after Phase 2, but final localization/responsive screenshots wait for US1–US4.
- Phase 8 requires all selected user stories.

## Parallel Opportunities

- T002/T003/T005 can run in parallel.
- T006/T008/T013/T014 can run in parallel before their integration tasks.
- T017/T018/T019 can be assigned separately with coordination around router integration.
- US2 and US3 implementation can run in parallel after US1.
- T039/T040 can be prepared in parallel with late US4 work.

## Implementation Strategy

**MVP**: Phases 1–2 + US1 + US2 + US3. This yields a complete public→login→dashboard→account/logout product entrance.

Then add US4 route/navigation migration and US5 polish/accessibility. No production deployment or merge is implied until full validation and convergence.
