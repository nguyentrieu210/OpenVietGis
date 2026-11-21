# UI Contract: Canonical Routes & Legacy Redirects

## Public routes

| Canonical route | Purpose |
| --- | --- |
| `/` | Public OpenVietGIS landing page |
| `/login` | Dedicated identity-provider login entry |

Opening `/` never requires authentication.

## Authenticated canonical routes

| Canonical route | Purpose |
| --- | --- |
| `/app` | Authenticated dashboard |
| `/app/organizations/:organizationId` | Organization overview/context |
| `/app/organizations/:organizationId/projects` | Project list/home entry |
| `/app/organizations/:organizationId/workspaces` | Workspace grouping |
| `/app/organizations/:organizationId/membership` | Members and access |
| `/app/account` | Account and application-session surface |
| `/app/invitations/accept` | Invitation acceptance |

Project capability routes append below the canonical organization/project path as those slices integrate.

## Login return contract

1. A protected route detects no valid application session.
2. The frontend constructs a relative `return_to` from pathname + search + hash.
3. `/login` passes the validated relative destination to the existing provider login endpoint.
4. After OIDC/application-session completion, normal route guards resolve the destination.
5. External/invalid/unavailable destinations fall back to `/app`.

## Legacy compatibility

Initially supported legacy routes under `/organizations/.../`, `/identity` and `/invitations/accept` redirect to canonical `/app/...` destinations while preserving valid scoped parameters.

Redirects do not bypass authorization and do not translate an inaccessible resource into a disclosed resource.
