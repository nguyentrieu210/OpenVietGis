# Data Model: App Shell & Identity UX

This slice adds no persistent server entity. The model below defines client-visible UI/session state and its constraints.

## SessionShellState

Represents the browser's first-party application-session view.

- `phase`: `unknown | authenticated | checking | signed_out | unavailable`
- `account`: current safe account summary when `authenticated` or background `checking`
- `lastConfirmedAt`: optional client timestamp used only for UX diagnostics, never authorization
- `errorKind`: optional non-sensitive classification for unavailable/signed-out transitions

### Rules

- `unknown` may show a bounded initial bootstrap/skeleton.
- `checking` with a previously confirmed account keeps shell chrome mounted but does not authorize protected tenant data.
- Confirmed 401 sets `signed_out` and clears protected query caches before protected UI can continue.
- Network failure with no previously confirmed account sets `unavailable`; it must not guess authenticated state.
- Provider/OIDC access and ID tokens are never fields of this state.

## ReturnDestination

A user-intended destination preserved through login.

- `pathname`: application-relative path beginning with `/`
- `search`: optional query string
- `hash`: optional fragment

### Rules

- No scheme, host, protocol-relative URL or external origin is accepted.
- After authentication, the destination is re-evaluated by normal route guards/authorization.
- Invalid or unavailable destinations fall back to `/app`.

## AppNavigationItem

Describes one primary shell destination.

- `id`: stable UI command/navigation identity
- `labelKey`: Vietnamese-facing label identifier
- `destination`: canonical app route or command
- `scope`: global, organization or project
- `availability`: visible, disabled-with-reason, or omitted

### Rules

- Availability is a presentation result, never an authorization grant.
- Internal roadmap IDs are not labels.
- Project-scoped items require current valid project context.

## AccountMenuState

- safe account display name
- optional safe email snapshot
- links to account/session and invitations
- sign-out mutation state

### Rules

- Logout is idempotent from the user's perspective; ambiguous network outcomes trigger authoritative session revalidation.
- Provider tokens/secrets are never displayed or persisted.

## ShellPreference

Non-sensitive personal browser preference.

- `navigationCollapsed`: boolean
- optional density/accessibility preference introduced by an accepted later slice

### Rules

- Must not include membership, permission, token, project data, layer data or authorization snapshots.
- Preference failure cannot block access to the application.
