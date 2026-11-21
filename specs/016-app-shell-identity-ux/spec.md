# Feature Specification: App Shell & Identity UX

**Feature Branch**: `spec/016-app-shell-identity-ux`

**Created**: 2026-11-21

**Status**: Draft

**Input**: Child slice of [ROADMAP.md — R002 Identity, Organization & Tenancy](../../ROADMAP.md#r002). The product owner approved the frontend decision tree covering public landing, dedicated login, stable authenticated shell, organization/project dashboard, account/session/logout controls, consistent navigation, Vietnamese-first UI, route migration, and non-flickering authentication behavior. This slice changes user-facing experience only; existing server-side identity, membership, tenant and authorization semantics remain authoritative.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Enter OpenVietGIS without authentication flicker (Priority: P1)

A visitor can open OpenVietGIS, understand what the product is, choose to sign in, complete the configured identity-provider login, and arrive at the intended authenticated destination without the page flashing between public, loading and authenticated states.

**Why this priority**: This is the first product impression and the gateway to every protected workflow. A visually polished application is unusable if session state is confusing or unstable.

**Independent Test**: Starting from a signed-out browser, open the public landing page, navigate to login, complete a configured sign-in, verify the authenticated shell appears once, then focus/blur/reload the browser and confirm the shell does not flash back to landing or a full-page loading screen.

**Acceptance Scenarios**:

1. **Given** a signed-out visitor, **When** they open `/`, **Then** they see a public OpenVietGIS landing page whether or not a prior session existed.
2. **Given** a signed-out visitor on a protected destination, **When** authentication is required, **Then** the system preserves the intended destination and returns the user there after successful sign-in.
3. **Given** an authenticated user, **When** background session validation occurs, **Then** the current application shell remains visible unless the session is actually invalid.
4. **Given** an invalid or expired session, **When** a protected action confirms authorization loss, **Then** the user is sent to the login experience with a clear explanation and without exposing protected data.

---

### User Story 2 - Work from a clear authenticated dashboard (Priority: P1)

An authenticated user lands in a stable application shell where they can see their current organization context, recent or available projects, and direct actions to create/open a project, open a map, import data, or manage organization access.

**Why this priority**: The authenticated home must orient users before they enter specialist GIS tools; raw organization UUIDs and technical pages must not be the primary navigation model.

**Independent Test**: Sign in as a user with at least one organization and project, open `/app`, identify the current organization and available project actions without viewing internal IDs, then navigate to Projects and Membership from the persistent shell.

**Acceptance Scenarios**:

1. **Given** an authenticated user with organizations, **When** they open `/app`, **Then** they see a human-readable dashboard rather than a technical organization listing.
2. **Given** a user with multiple organizations, **When** they change organization context, **Then** the shell and dashboard update to the selected organization without exposing internal IDs as primary labels.
3. **Given** a user with no projects, **When** they open the dashboard, **Then** they see an empty state with clear next actions rather than a blank or error-like screen.
4. **Given** a user without permission for an administrative action, **When** they use the dashboard, **Then** that action is not presented as available; server authorization remains the final authority.

---

### User Story 3 - Manage account and sign out from anywhere (Priority: P1)

An authenticated user can open a consistent account menu from the application header to view account/session information, access invitations, and sign out from any major authenticated screen.

**Why this priority**: Session controls are security-critical and must not be hidden several screens deep.

**Independent Test**: From Dashboard, Project and a specialist workspace, open the same account menu, navigate to account/session information, sign out, and verify the session is invalidated and the browser returns to the public experience.

**Acceptance Scenarios**:

1. **Given** an authenticated user, **When** they open the account menu, **Then** they can access account/session information, invitations and sign-out.
2. **Given** an authenticated user, **When** they sign out, **Then** the active application session is invalidated and protected content is no longer available.
3. **Given** logout completes while the user is on a deep protected route, **When** navigation completes, **Then** the user returns to the public landing page rather than a broken protected screen.

---

### User Story 4 - Navigate consistently across the application (Priority: P2)

A user can move between Overview, Projects, GIS Data, Map, Tools and Members/Access through a persistent navigation model that remains recognizable across authenticated screens.

**Why this priority**: Current feature pages expose fragmented links and roadmap/internal terminology. A stable information architecture reduces cognitive load as capabilities grow.

**Independent Test**: Starting on the dashboard, visit at least five major authenticated areas and verify the same shell, active navigation state, breadcrumb context and account controls remain available.

**Acceptance Scenarios**:

1. **Given** an authenticated user, **When** they navigate between major areas, **Then** the persistent shell remains stable and the active location is visually clear.
2. **Given** an advanced or project-specific area, **When** the user needs technical metadata, **Then** that metadata is available through a secondary technical/details surface rather than crowding primary navigation.
3. **Given** a legacy bookmark, **When** the user opens a supported old route, **Then** the application safely redirects to the corresponding canonical route without losing organization/project context.

---

### User Story 5 - Use a coherent Vietnamese-first interface across devices (Priority: P2)

A user sees Vietnamese as the default interface language, with technical standards retained where translation would reduce clarity, and can use the main shell on desktop and tablet without broken layout or inaccessible interactions.

**Why this priority**: Mixed raw English/backend labels reduce trust and usability for the target audience.

**Independent Test**: Review the public landing, login, dashboard, account menu and navigation at desktop and tablet widths; verify user-facing labels are Vietnamese, technical standards remain recognizable, focus states are visible, and no horizontal layout break occurs.

**Acceptance Scenarios**:

1. **Given** a normal user-facing screen, **When** content renders, **Then** UI labels are Vietnamese by default except accepted technical terms such as CRS, EPSG, GeoJSON, GeoPackage, PostGIS, WMS or DXF.
2. **Given** keyboard navigation, **When** the user tabs through interactive controls, **Then** focus remains visible and interaction order is predictable.
3. **Given** a tablet-sized viewport, **When** the user opens the app shell, **Then** navigation adapts without forcing the full desktop layout or hiding required account/session controls.

### Edge Cases

- A login provider is unavailable or returns an error.
- A protected deep link is opened while signed out.
- The session expires while the browser remains open.
- A membership or grant is revoked while the shell is already mounted.
- The user belongs to no organization.
- The user belongs to many organizations with long names.
- The current organization or project becomes archived/deleted between page loads.
- A legacy route contains an organization or project the user can no longer access.
- Background refetches fail transiently while cached authenticated UI is visible.
- A user activates sign-out twice or network completion is ambiguous.
- Long Vietnamese labels, narrow tablet widths and browser zoom cause navigation pressure.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The product MUST provide a public landing experience at `/` that remains public and viewable independently of authentication state.
- **FR-002**: The product MUST provide a dedicated login experience at `/login` and MUST NOT require users to scroll through the landing page to find sign-in.
- **FR-003**: Protected destinations MUST preserve an authorized return destination through the login flow and return the user there after successful authentication.
- **FR-004**: The authenticated product MUST have a canonical application entry at `/app` that presents a human-readable dashboard.
- **FR-005**: The authenticated shell MUST provide persistent primary navigation for Overview, Projects, GIS Data, Map, Tools and Members/Access, with entries shown only when meaningful in the current scope.
- **FR-006**: The authenticated shell MUST provide a persistent account control with account/session, invitation and sign-out actions on all major authenticated surfaces.
- **FR-007**: Sign-out MUST invalidate the active application session and prevent further access to protected content through the signed-out browser session.
- **FR-008**: Session/background validation MUST NOT replace the entire authenticated application with a full-page loading or public screen while valid authenticated state is already known.
- **FR-009**: Confirmed authentication loss MUST transition the user to login without continuing to display protected data.
- **FR-010**: The dashboard MUST present organizations, projects and primary actions by human-readable names; internal UUIDs MUST NOT be primary navigation labels.
- **FR-011**: Internal identifiers, revisions and diagnostic metadata MUST be hidden from the ordinary primary workflow and made available only through an explicit technical/details surface when needed.
- **FR-012**: User-facing authorization affordances MUST be capability/permission aware, but hidden or disabled controls MUST NOT be treated as authorization enforcement.
- **FR-013**: The application MUST distinguish absent permission from unsupported capability when communicating why an action is unavailable.
- **FR-014**: Vietnamese MUST be the default user-facing language for this slice; accepted standards and technical terms MAY remain in their established notation.
- **FR-015**: Raw backend enum values, internal roadmap IDs and development labels MUST NOT appear in ordinary user-facing navigation or status text.
- **FR-016**: The public and authenticated shells MUST provide clear loading, empty, error and success states without browser-native alert dialogs.
- **FR-017**: Keyboard focus MUST be visible for primary navigation, login, account and dashboard controls; interactive controls MUST have accessible names.
- **FR-018**: The authenticated shell MUST remain usable at desktop and tablet widths; narrow layouts MUST adapt navigation rather than create horizontal page overflow.
- **FR-019**: Existing supported legacy URLs MUST redirect safely to canonical application routes during migration, preserving valid organization/project context.
- **FR-020**: Personal shell preferences such as collapsed navigation MAY persist locally but MUST remain separate from shared project/map configuration.
- **FR-021**: The frontend MUST reuse the existing identity/session/membership/authorization product semantics and MUST NOT create a second client-owned authorization model.
- **FR-022**: The frontend MUST NOT store external identity-provider access or ID tokens in browser storage.
- **FR-023**: Unsafe authenticated operations MUST continue to respect the existing application's CSRF and session protections.
- **FR-024**: Visual design MUST follow one coherent light-theme system for this slice; dark mode is outside initial scope.
- **FR-025**: The public landing and authenticated shell MUST use a consistent OpenVietGIS brand while allowing the authenticated workspace to be denser than the public marketing surface.

### Key Entities

- **Application Session**: The current first-party authenticated browser session and its visible state (checking, authenticated, signed out, invalid).
- **Account Context**: Human-readable user identity information exposed to account/session controls without provider secrets.
- **Organization Context**: The currently selected organization, its human-readable name and the user's effective relationship to it.
- **Project Summary**: Human-readable project information used for dashboard navigation and recent/open-project actions.
- **Navigation State**: The current canonical application location, active primary section, breadcrumb context and optional personal shell preferences.
- **Legacy Route Mapping**: A compatibility mapping from an old supported user-facing URL to a canonical application destination.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In browser acceptance tests, 100% of tested valid-session focus/refetch transitions keep the authenticated shell mounted without a visible full-page landing/loading flash.
- **SC-002**: A signed-out user can reach the login action from the public landing page in one intentional navigation action and can return to the landing page in one action.
- **SC-003**: A signed-in user with one organization can identify and open an available project from `/app` without reading or copying an internal UUID.
- **SC-004**: Sign-out is reachable within two interactions from every tested major authenticated screen and leaves subsequent protected requests unauthenticated.
- **SC-005**: All primary shell interactions required by this slice are operable with keyboard-only navigation and expose visible focus.
- **SC-006**: At 768 px and wider reference widths, the app shell has no page-level horizontal overflow caused by navigation/account controls.
- **SC-007**: Screenshot regression covers at least Landing, Login, Dashboard and Account Menu with deterministic non-map content and produces no unexplained visual differences at acceptance.
- **SC-008**: A tested set of supported legacy URLs redirects to canonical routes with equivalent valid organization/project context and no navigation dead ends.
- **SC-009**: A terminology audit finds no internal roadmap codes and no raw backend enum labels in the ordinary Landing, Login, Dashboard and primary navigation surfaces.
- **SC-010**: Existing server-side tenant/authorization regression tests continue to pass unchanged; the frontend slice introduces no evidence of client-side authorization becoming authoritative.

## Assumptions

- Existing R002 identity, organization, workspace/project, invitation, session, CSRF and authorization contracts remain the source of truth.
- Keycloak remains a reference identity provider but the UX must remain provider-neutral.
- Public account self-registration, password reset and provider-specific account management are outside this slice unless exposed by an existing configured identity provider.
- Dark mode is deferred; the initial accepted visual system is light-first.
- Desktop is the primary authenticated work context; tablet is supported by adaptive shell behavior. Full mobile GIS parity is outside this child slice.
- Exact startup/network performance budgets will be measured against the implementation baseline before final convergence rather than invented in the specification.
- This child slice is traced to roadmap R002 and does not change R002's security semantics or acceptance history.
