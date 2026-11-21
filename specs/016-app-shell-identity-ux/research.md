# Research: App Shell & Identity UX

## Decision 1 — Separate public, login and authenticated shells

**Decision**: Keep `/` public, use `/login` for authentication entry, and use `/app` as the canonical authenticated dashboard. Protected deep links preserve a validated relative return destination.

**Rationale**: The current root route changes meaning based on session state, which makes the landing page appear to disappear and makes auth transitions visually unstable. Separate shells make product intent and browser history predictable.

**Alternatives considered**:
- Keep one conditional root route: rejected because it conflates marketing/public and protected application states.
- Redirect every authenticated visitor away from `/`: rejected because the approved product decision says the landing remains independently viewable.

## Decision 2 — Stable session shell without weakening protected data freshness

**Decision**: Create a dedicated session state machine for first-party account identity. After an authenticated account is known, background `/me` revalidation may mark the session as checking without unmounting the shell. A confirmed 401 transitions to signed-out. Protected organization/project/layer queries retain the existing strict freshness behavior and may not reuse stale protected payload as current authority.

**Rationale**: The current `FreshAccount` combines session identity with `protectedQueryOptions`, so any refetch temporarily removes authenticated UI. Changing those options globally would solve flicker by weakening the intended stale-authorization defense. Splitting concerns preserves both security and UX.

**Alternatives considered**:
- Increase `staleTime` globally for all protected queries: rejected as a tenant/authorization regression risk.
- Never revalidate `/me`: rejected because session expiry/logout still needs observable recovery.
- Keep a cached protected tenant page visible during any refetch: rejected because the existing contract intentionally avoids treating stale protected data as current authorization.

## Decision 3 — Return destinations are relative and scope-safe

**Decision**: Preserve only application-relative path/search/hash return destinations. Reject or normalize absolute/external targets. The server remains responsible for provider/OIDC state validation.

**Rationale**: Deep links improve workflow, but an unvalidated return target can become an open redirect.

## Decision 4 — Project-owned lightweight UI system

**Decision**: Add semantic CSS tokens and a small set of project-owned accessible primitives. Do not introduce a large component framework. Prefer native buttons/links/forms/details where they satisfy behavior; only consider a headless primitive dependency for difficult focus-management cases after normal review.

**Rationale**: Grill approved an OpenVietGIS-specific professional GIS identity rather than a generic admin template. A small UI layer controls visual consistency without coupling the product to a heavy library.

## Decision 5 — Unified OpenVietGIS visual direction

**Decision**: Use a light, professional geospatial system with teal as the primary product accent, slate neutrals, white cards/surfaces, visible focus and restrained motion. Public pages may be more spacious; authenticated pages become denser. Use a robust system sans-serif stack (with Inter-compatible metrics) and a monospace stack only for technical values.

**Rationale**: UI UX Pro Max classified the product as B2B/enterprise and emphasized accessible, high-contrast, restrained design. Its raw B2B palette suggested navy/blue and Plus Jakarta Sans, while the GIS-specific recommendation selected teal `#0F766E`. Product-owner Grill decisions already chose teal GIS identity. A unified teal/slate system avoids two competing brands and avoids an external font dependency.

**UI UX Pro Max evidence used**:
- Accessible & Ethical style: visible focus, keyboard navigation, reduced motion, 44×44 touch targets, contrast.
- B2B shell recommendation: restrained enterprise surfaces and low-friction login.
- Anti-patterns: hidden credentials, playful/AI purple gradients.
- Navigation research: deep-linkable URLs, skip links, predictable keyboard order.
- Authentication research: accessible authentication, announced errors, accessible names.

## Decision 6 — Vietnamese-first with technical standards preserved

**Decision**: User-facing shell text is Vietnamese by default. Established standards and identifiers such as CRS/EPSG/GeoJSON remain untranslated when appropriate. Backend enum strings and roadmap IDs are mapped to user-facing labels.

**Rationale**: Mixed raw English/internal terminology was a direct usability problem observed by the product owner.

## Decision 7 — Route migration is compatibility, not a flag day

**Decision**: Canonical authenticated routes live under `/app`, while supported old URLs redirect to equivalent canonical destinations during migration.

**Rationale**: Existing bookmarks, OIDC return paths and cross-feature links must not break immediately.

## Decision 8 — No new backend contract is required initially

**Decision**: Reuse existing SDK functions for identity providers, current user, organizations, projects, invitations, identity links, CSRF and logout. Escalate a backend/API change only if implementation proves an actual UX blocker.

**Rationale**: The approved scope is a frontend child slice and accepted R002 contracts already cover required behavior.
