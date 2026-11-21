# Quickstart Validation: App Shell & Identity UX

## Prerequisites

- Exact feature branch rebased onto the current verified main before implementation.
- Supported Node 26 runtime and frozen dependency install.
- Existing API, PostgreSQL test database and configured synthetic/reference OIDC provider.
- Existing R002 integration/security tests green before frontend acceptance.

## Validation scenarios

### 1. Public landing remains public

1. Start with a browser context without an application session.
2. Open `/`.
3. Confirm Landing renders directly and exposes a clear Đăng nhập action.
4. Open `/login`; confirm configured providers are listed.
5. Navigate back to `/`.

**Expected**: No protected organization data is requested/rendered as public content; landing and login are distinct surfaces.

### 2. Deep-link login and return

1. Open a protected canonical project URL while signed out.
2. Complete provider login.
3. Verify the browser returns to the same valid canonical destination.
4. Repeat with an invalid/external return target.

**Expected**: Valid relative deep link is restored; invalid/external target falls back to `/app`.

### 3. No auth flicker

1. Sign in and open `/app`.
2. Trigger window blur/focus, reconnect and background account revalidation.
3. Record DOM/screenshot sequence.

**Expected**: Authenticated shell remains mounted. It does not flash Landing or a full-page “checking” screen. Protected tenant content still follows its own strict current-read state.

### 4. Dashboard and navigation

1. Use a user with at least one organization and project.
2. Open `/app`.
3. Navigate Overview → Projects → Members/Access → Account.
4. Repeat with a user having no projects.

**Expected**: Human-readable names and empty states are clear; no primary UUID/roadmap-code labels.

### 5. Logout everywhere

1. Open Dashboard, a Project route and another authenticated surface.
2. Open the account menu and sign out.
3. Attempt to revisit a protected route.

**Expected**: Server session is invalidated; protected route returns to login and no stale protected data remains visible.

### 6. Legacy route migration

Exercise the agreed legacy URL matrix from [routes-and-redirects.md](contracts/routes-and-redirects.md).

**Expected**: Each supported old path reaches its canonical equivalent without context loss or authorization bypass.

### 7. Accessibility/responsive

- Keyboard-only pass through Landing, Login, shell navigation and account menu.
- Check visible focus, skip-to-content, accessible names and announced errors.
- Check widths 768, 1024 and 1440 px.
- Respect reduced motion.

### 8. Deterministic visual regression

Capture Landing, Login, Dashboard and Account Menu using stable fixture data.

**Expected**: No unexplained screenshot diff; remote/non-deterministic external content is excluded.

## Required commands

Use repository-standard commands from the exact supported runtime:

- `pnpm --filter @openvietgis/web test`
- `pnpm --filter @openvietgis/web lint`
- `pnpm --filter @openvietgis/web typecheck`
- `pnpm --filter @openvietgis/web build`
- targeted Playwright scenarios
- full root validation before convergence

Record exact commit, runtime versions and browser fixture for acceptance evidence.
