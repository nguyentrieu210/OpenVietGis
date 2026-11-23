from __future__ import annotations

from datetime import UTC, datetime
from uuid import UUID

from sqlalchemy import text
from sqlalchemy.engine import RowMapping
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.rls import (
    prepare_service_credential_lookup,
    require_member_context,
)
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.domain import (
    ResourceGrant,
    ServiceCredential,
    ServicePrincipal,
    ServicePrincipalStatus,
)
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.roles import (
    ORGANIZATION_ROLE_PERMISSIONS,
    RESOURCE_ROLE_PERMISSIONS,
)
from openvietgis_api.domains.authorization.service import TenantResourceNotFoundError
from openvietgis_api.domains.tenancy.domain import (
    ActorType,
    MembershipRole,
    ResourceRole,
    ResourceScopeKind,
)


class SqlEffectivePermissionProvider:
    def __init__(self, session: Session) -> None:
        self._session = session

    def resolve(self, context: AuthorizationContext) -> frozenset[Permission]:
        actor = context.actor
        if actor.actor_id is None:
            return frozenset()
        if actor.actor_type == "service_principal":
            return self._resolve_service(context)
        if actor.actor_type != "human":
            return frozenset()

        try:
            require_member_context(self._session, context, context.organization_id)
        except TenantResourceNotFoundError:
            return frozenset()
        rows = (
            self._session.execute(
                text(
                    """
                    WITH requested AS (
                        SELECT CAST(:organization_id AS uuid) AS organization_id,
                               CAST(:user_id AS uuid) AS user_id,
                               CAST(:workspace_id AS uuid) AS workspace_id,
                               CAST(:project_id AS uuid) AS project_id
                    )
                    SELECT m.role AS membership_role, g.role AS grant_role
                    FROM requested AS r
                    JOIN org.organization_memberships AS m
                      ON m.organization_id = r.organization_id
                     AND m.user_id = r.user_id
                     AND m.status = 'active'
                    JOIN org.organizations AS o
                      ON o.id = m.organization_id
                     AND o.status = 'active'
                    LEFT JOIN workspace.projects AS p
                      ON p.organization_id = r.organization_id
                     AND p.id = r.project_id
                    LEFT JOIN workspace.workspaces AS w
                      ON r.project_id IS NULL
                     AND w.organization_id = r.organization_id
                     AND w.id = r.workspace_id
                    LEFT JOIN org.resource_grants AS g
                      ON g.organization_id = r.organization_id
                     AND g.subject_kind = 'membership'
                     AND g.membership_id = m.id
                     AND g.revoked_at IS NULL
                     AND (
                          g.scope_kind = 'organization'
                          OR (
                              g.scope_kind = 'workspace'
                              AND g.workspace_id = CASE
                                  WHEN r.project_id IS NOT NULL THEN p.workspace_id
                                  ELSE r.workspace_id
                              END
                          )
                          OR (
                              g.scope_kind = 'project'
                              AND g.project_id = r.project_id
                          )
                     )
                    WHERE (
                        r.project_id IS NOT NULL
                        AND p.id IS NOT NULL
                        AND (r.workspace_id IS NULL OR r.workspace_id = p.workspace_id)
                    ) OR (
                        r.project_id IS NULL
                        AND (r.workspace_id IS NULL OR w.id IS NOT NULL)
                    )
                    """
                ),
                {
                    "organization_id": context.organization_id,
                    "user_id": actor.actor_id,
                    "workspace_id": context.workspace_id,
                    "project_id": context.project_id,
                },
            )
            .mappings()
            .all()
        )

        permissions: set[Permission] = set()
        for row in rows:
            permissions.update(
                ORGANIZATION_ROLE_PERMISSIONS[MembershipRole(row["membership_role"])]
            )
            if row["grant_role"] is not None:
                permissions.update(RESOURCE_ROLE_PERMISSIONS[ResourceRole(row["grant_role"])])

        return frozenset(permissions)

    def _resolve_service(self, context: AuthorizationContext) -> frozenset[Permission]:
        actor = context.actor
        if actor.actor_id is None or context.credential_id is None:
            return frozenset()
        try:
            require_member_context(self._session, context, context.organization_id)
        except (TenantResourceNotFoundError, PermissionError):
            return frozenset()

        rows = (
            self._session.execute(
                text(
                    """
                    WITH requested AS (
                        SELECT CAST(:organization_id AS uuid) AS organization_id,
                               CAST(:principal_id AS uuid) AS principal_id,
                               CAST(:credential_id AS uuid) AS credential_id,
                               CAST(:workspace_id AS uuid) AS workspace_id,
                               CAST(:project_id AS uuid) AS project_id
                    )
                    SELECT g.role AS grant_role, c.permission_ceiling
                    FROM requested AS r
                    JOIN org.service_principals AS sp
                      ON sp.organization_id = r.organization_id
                     AND sp.id = r.principal_id
                     AND sp.status = 'active'
                     AND sp.revoked_at IS NULL
                    JOIN org.service_credentials AS c
                      ON c.organization_id = sp.organization_id
                     AND c.service_principal_id = sp.id
                     AND c.id = r.credential_id
                     AND c.revoked_at IS NULL
                     AND (c.expires_at IS NULL OR c.expires_at > now())
                    JOIN org.organizations AS o
                      ON o.id = sp.organization_id
                     AND o.status = 'active'
                    LEFT JOIN workspace.projects AS p
                      ON p.organization_id = r.organization_id
                     AND p.id = r.project_id
                    LEFT JOIN workspace.workspaces AS w
                      ON r.project_id IS NULL
                     AND w.organization_id = r.organization_id
                     AND w.id = r.workspace_id
                    LEFT JOIN org.resource_grants AS g
                      ON g.organization_id = r.organization_id
                     AND g.subject_kind = 'service_principal'
                     AND g.service_principal_id = sp.id
                     AND g.revoked_at IS NULL
                     AND (
                          g.scope_kind = 'organization'
                          OR (
                              g.scope_kind = 'workspace'
                              AND g.workspace_id = CASE
                                  WHEN r.project_id IS NOT NULL THEN p.workspace_id
                                  ELSE r.workspace_id
                              END
                          )
                          OR (
                              g.scope_kind = 'project'
                              AND g.project_id = r.project_id
                          )
                     )
                    WHERE (
                        r.project_id IS NOT NULL
                        AND p.id IS NOT NULL
                        AND (r.workspace_id IS NULL OR r.workspace_id = p.workspace_id)
                    ) OR (
                        r.project_id IS NULL
                        AND (r.workspace_id IS NULL OR w.id IS NOT NULL)
                    )
                    """
                ),
                {
                    "organization_id": context.organization_id,
                    "principal_id": actor.actor_id,
                    "credential_id": context.credential_id,
                    "workspace_id": context.workspace_id,
                    "project_id": context.project_id,
                },
            )
            .mappings()
            .all()
        )
        if not rows:
            return frozenset()

        permissions: set[Permission] = set()
        ceiling_raw = rows[0]["permission_ceiling"]
        for row in rows:
            if row["grant_role"] is not None:
                permissions.update(RESOURCE_ROLE_PERMISSIONS[ResourceRole(row["grant_role"])])
        if ceiling_raw is not None:
            ceiling = {Permission(value) for value in ceiling_raw}
            permissions.intersection_update(ceiling)
        return frozenset(permissions)


def _grant_from_row(row: RowMapping) -> ResourceGrant:
    return ResourceGrant(
        id=row["grant_id"],
        organization_id=row["organization_id"],
        subject_kind=row["subject_kind"],
        membership_id=row["membership_id"],
        service_principal_id=row["service_principal_id"],
        scope_kind=ResourceScopeKind(row["scope_kind"]),
        workspace_id=row["workspace_id"],
        project_id=row["project_id"],
        role=ResourceRole(row["role"]),
        created_at=row["created_at"],
        created_by_actor_type=ActorType(row["created_by_actor_type"]),
        created_by_actor_id=row["created_by_actor_id"],
        revoked_at=row["revoked_at"],
    )


class SqlResourceGrantRepository:
    def __init__(self, session: Session) -> None:
        self._session = session

    def list_for_organization(
        self, organization_id: UUID, *, context: AuthorizationContext
    ) -> list[ResourceGrant]:
        require_member_context(self._session, context, organization_id)
        rows = (
            self._session.execute(
                text(
                    """
                    SELECT
                        id AS grant_id,
                        organization_id,
                        subject_kind,
                        membership_id,
                        service_principal_id,
                        scope_kind,
                        workspace_id,
                        project_id,
                        role,
                        created_at,
                        created_by_actor_type,
                        created_by_actor_id,
                        revoked_at
                    FROM org.resource_grants
                    WHERE organization_id = :organization_id
                    ORDER BY created_at, id
                    """
                ),
                {"organization_id": organization_id},
            )
            .mappings()
            .all()
        )
        return [_grant_from_row(row) for row in rows]

    def create(self, grant: ResourceGrant, *, context: AuthorizationContext) -> None:
        require_member_context(self._session, context, grant.organization_id)
        self._session.execute(
            text(
                """
                INSERT INTO org.resource_grants (
                    id, organization_id, subject_kind, membership_id,
                    service_principal_id, scope_kind, workspace_id,
                    project_id, role, created_at, created_by_actor_type,
                    created_by_actor_id, revoked_at
                )
                VALUES (
                    :id, :organization_id, :subject_kind, :membership_id,
                    :service_principal_id, :scope_kind, :workspace_id,
                    :project_id, :role, :created_at, :created_by_actor_type,
                    :created_by_actor_id, :revoked_at
                )
                """
            ),
            {
                "id": grant.id,
                "organization_id": grant.organization_id,
                "subject_kind": grant.subject_kind,
                "membership_id": grant.membership_id,
                "service_principal_id": grant.service_principal_id,
                "scope_kind": grant.scope_kind.value,
                "workspace_id": grant.workspace_id,
                "project_id": grant.project_id,
                "role": grant.role.value,
                "created_at": grant.created_at,
                "created_by_actor_type": grant.created_by_actor_type.value,
                "created_by_actor_id": grant.created_by_actor_id,
                "revoked_at": grant.revoked_at,
            },
        )

    def get(
        self,
        *,
        context: AuthorizationContext,
        organization_id: UUID,
        grant_id: UUID,
        for_update: bool = False,
    ) -> ResourceGrant | None:
        require_member_context(self._session, context, organization_id)
        suffix = " FOR UPDATE" if for_update else ""
        row = (
            self._session.execute(
                text(
                    """
                    SELECT
                        id AS grant_id,
                        organization_id,
                        subject_kind,
                        membership_id,
                        service_principal_id,
                        scope_kind,
                        workspace_id,
                        project_id,
                        role,
                        created_at,
                        created_by_actor_type,
                        created_by_actor_id,
                        revoked_at
                    FROM org.resource_grants
                    WHERE organization_id = :organization_id
                      AND id = :grant_id
                    """
                    + suffix
                ),
                {"organization_id": organization_id, "grant_id": grant_id},
            )
            .mappings()
            .one_or_none()
        )
        return _grant_from_row(row) if row is not None else None

    def update(self, grant: ResourceGrant, *, context: AuthorizationContext) -> None:
        require_member_context(self._session, context, grant.organization_id)
        self._session.execute(
            text(
                """
                UPDATE org.resource_grants
                SET revoked_at = :revoked_at
                WHERE organization_id = :organization_id
                  AND id = :grant_id
                """
            ),
            {
                "organization_id": grant.organization_id,
                "grant_id": grant.id,
                "revoked_at": grant.revoked_at,
            },
        )


def _principal_from_row(row: RowMapping) -> ServicePrincipal:
    return ServicePrincipal(
        id=row["principal_id"],
        organization_id=row["organization_id"],
        name=row["principal_name"],
        description=row["description"],
        status=ServicePrincipalStatus(row["principal_status"]),
        created_at=row["principal_created_at"],
        created_by_actor_type=ActorType(row["created_by_actor_type"]),
        created_by_actor_id=row["created_by_actor_id"],
        revoked_at=row["principal_revoked_at"],
    )


def _credential_from_row(row: RowMapping) -> ServiceCredential:
    ceiling = row["permission_ceiling"]
    return ServiceCredential(
        id=row["credential_id"],
        organization_id=row["organization_id"],
        service_principal_id=row["service_principal_id"],
        name=row["credential_name"],
        prefix=row["prefix"],
        secret_hash=bytes(row["secret_hash"]),
        permission_ceiling=tuple(ceiling) if ceiling is not None else None,
        created_at=row["credential_created_at"],
        expires_at=row["expires_at"],
        last_used_at=row["last_used_at"],
        revoked_at=row["credential_revoked_at"],
    )


_PRINCIPAL_COLUMNS = """
    id AS principal_id,
    organization_id,
    name AS principal_name,
    description,
    status AS principal_status,
    created_at AS principal_created_at,
    created_by_actor_type,
    created_by_actor_id,
    revoked_at AS principal_revoked_at
"""

_CREDENTIAL_COLUMNS = """
    id AS credential_id,
    organization_id,
    service_principal_id,
    name AS credential_name,
    prefix,
    secret_hash,
    permission_ceiling,
    created_at AS credential_created_at,
    expires_at,
    last_used_at,
    revoked_at AS credential_revoked_at
"""


class SqlServicePrincipalRepository:
    def __init__(self, session: Session) -> None:
        self._session = session

    def list_principals(
        self,
        organization_id: UUID,
        *,
        context: AuthorizationContext,
    ) -> list[ServicePrincipal]:
        require_member_context(self._session, context, organization_id)
        rows = (
            self._session.execute(
                text(
                    f"""
                    SELECT {_PRINCIPAL_COLUMNS}
                    FROM org.service_principals
                    WHERE organization_id = :organization_id
                    ORDER BY created_at, id
                    """
                ),
                {"organization_id": organization_id},
            )
            .mappings()
            .all()
        )
        return [_principal_from_row(row) for row in rows]

    def get_principal(
        self,
        *,
        context: AuthorizationContext,
        organization_id: UUID,
        service_principal_id: UUID,
        for_update: bool = False,
    ) -> ServicePrincipal | None:
        require_member_context(self._session, context, organization_id)
        suffix = " FOR UPDATE" if for_update else ""
        row = (
            self._session.execute(
                text(
                    f"""
                    SELECT {_PRINCIPAL_COLUMNS}
                    FROM org.service_principals
                    WHERE organization_id = :organization_id
                      AND id = :principal_id
                    {suffix}
                    """
                ),
                {
                    "organization_id": organization_id,
                    "principal_id": service_principal_id,
                },
            )
            .mappings()
            .one_or_none()
        )
        return _principal_from_row(row) if row is not None else None

    def create_principal(
        self,
        principal: ServicePrincipal,
        *,
        context: AuthorizationContext,
    ) -> None:
        require_member_context(self._session, context, principal.organization_id)
        self._session.execute(
            text(
                """
                INSERT INTO org.service_principals (
                    id, organization_id, name, description, status, created_at,
                    created_by_actor_type, created_by_actor_id, revoked_at
                )
                VALUES (
                    :id, :organization_id, :name, :description, :status, :created_at,
                    :created_by_actor_type, :created_by_actor_id, :revoked_at
                )
                """
            ),
            {
                "id": principal.id,
                "organization_id": principal.organization_id,
                "name": principal.name,
                "description": principal.description,
                "status": principal.status.value,
                "created_at": principal.created_at,
                "created_by_actor_type": principal.created_by_actor_type.value,
                "created_by_actor_id": principal.created_by_actor_id,
                "revoked_at": principal.revoked_at,
            },
        )

    def update_principal(
        self,
        principal: ServicePrincipal,
        *,
        context: AuthorizationContext,
    ) -> None:
        require_member_context(self._session, context, principal.organization_id)
        self._session.execute(
            text(
                """
                UPDATE org.service_principals
                SET status = :status, revoked_at = :revoked_at
                WHERE organization_id = :organization_id AND id = :principal_id
                """
            ),
            {
                "status": principal.status.value,
                "revoked_at": principal.revoked_at,
                "organization_id": principal.organization_id,
                "principal_id": principal.id,
            },
        )

    def create_credential(
        self,
        credential: ServiceCredential,
        *,
        context: AuthorizationContext,
    ) -> None:
        require_member_context(self._session, context, credential.organization_id)
        self._session.execute(
            text(
                """
                INSERT INTO org.service_credentials (
                    id, organization_id, service_principal_id, name, prefix,
                    secret_hash, permission_ceiling, created_at, expires_at,
                    last_used_at, revoked_at
                )
                VALUES (
                    :id, :organization_id, :service_principal_id, :name, :prefix,
                    :secret_hash, :permission_ceiling, :created_at, :expires_at,
                    :last_used_at, :revoked_at
                )
                """
            ),
            {
                "id": credential.id,
                "organization_id": credential.organization_id,
                "service_principal_id": credential.service_principal_id,
                "name": credential.name,
                "prefix": credential.prefix,
                "secret_hash": credential.secret_hash,
                "permission_ceiling": list(credential.permission_ceiling)
                if credential.permission_ceiling is not None
                else None,
                "created_at": credential.created_at,
                "expires_at": credential.expires_at,
                "last_used_at": credential.last_used_at,
                "revoked_at": credential.revoked_at,
            },
        )

    def list_credential_metadata(
        self,
        *,
        context: AuthorizationContext,
        organization_id: UUID,
        service_principal_id: UUID,
    ) -> list[dict[str, object]]:
        require_member_context(self._session, context, organization_id)
        # Never select the credential hash or a recoverable secret for the portal.
        rows = (
            self._session.execute(
                text("""
                SELECT id, name, prefix, permission_ceiling, created_at,
                       expires_at, last_used_at, revoked_at
                FROM org.service_credentials
                WHERE organization_id=:organization_id
                  AND service_principal_id=:service_principal_id
                ORDER BY created_at DESC, id
            """),
                {"organization_id": organization_id, "service_principal_id": service_principal_id},
            )
            .mappings()
            .all()
        )
        return [dict(row) for row in rows]

    def get_credential(
        self,
        *,
        context: AuthorizationContext,
        organization_id: UUID,
        service_principal_id: UUID,
        credential_id: UUID,
        for_update: bool = False,
    ) -> ServiceCredential | None:
        require_member_context(self._session, context, organization_id)
        suffix = " FOR UPDATE" if for_update else ""
        row = (
            self._session.execute(
                text(
                    f"""
                    SELECT {_CREDENTIAL_COLUMNS}
                    FROM org.service_credentials
                    WHERE organization_id = :organization_id
                      AND service_principal_id = :service_principal_id
                      AND id = :credential_id
                    {suffix}
                    """
                ),
                {
                    "organization_id": organization_id,
                    "service_principal_id": service_principal_id,
                    "credential_id": credential_id,
                },
            )
            .mappings()
            .one_or_none()
        )
        return _credential_from_row(row) if row is not None else None

    def mark_credential_revoked(
        self,
        credential: ServiceCredential,
        *,
        context: AuthorizationContext,
    ) -> None:
        require_member_context(self._session, context, credential.organization_id)
        if context.actor.actor_type != "human" or credential.revoked_at is None:
            raise PermissionError("human credential revocation required")
        # Revocation is monotonic and never writes usage metadata from a snapshot.
        self._session.execute(
            text(
                """
                UPDATE org.service_credentials
                SET revoked_at = :revoked_at
                WHERE organization_id = :organization_id
                  AND service_principal_id = :service_principal_id
                  AND id = :credential_id
                  AND revoked_at IS NULL
                """
            ),
            {
                "revoked_at": credential.revoked_at,
                "organization_id": credential.organization_id,
                "service_principal_id": credential.service_principal_id,
                "credential_id": credential.id,
            },
        )

    def record_credential_use(
        self,
        credential: ServiceCredential,
        *,
        context: AuthorizationContext,
    ) -> bool:
        require_member_context(self._session, context, credential.organization_id)
        if (
            context.actor.actor_type != "service_principal"
            or context.actor.actor_id != credential.service_principal_id
            or context.credential_id != credential.id
        ):
            raise PermissionError("matching service credential context required")

        # Acquire the same organization lock as all HTTP admin and project
        # mutations before locking the credential, or a service mutation can
        # deadlock with an administrator revoking the very same credential.
        # The secret has already been verified before we acquire this lock.
        organization = self._session.execute(
            text(
                """
                SELECT id FROM org.organizations
                WHERE id = :organization_id AND status = 'active'
                FOR UPDATE
                """
            ),
            {"organization_id": credential.organization_id},
        ).scalar_one_or_none()
        if organization is None:
            return False

        # Revalidate the live principal and credential after acquiring the
        # organization lock, which serializes administrative revoke/suspend.
        # Never write revoked_at from the (potentially stale) lookup snapshot.
        return (
            self._session.execute(
                text(
                    """
                UPDATE org.service_credentials AS c
                SET last_used_at = GREATEST(c.last_used_at, :now)
                FROM org.service_principals AS sp
                WHERE c.organization_id = :organization_id
                  AND c.service_principal_id = :service_principal_id
                  AND c.id = :credential_id
                  AND c.revoked_at IS NULL
                  AND (c.expires_at IS NULL OR c.expires_at > :now)
                  AND sp.organization_id = c.organization_id
                  AND sp.id = c.service_principal_id
                  AND sp.status = 'active'
                  AND sp.revoked_at IS NULL
                RETURNING c.id
                """
                ),
                {
                    "now": datetime.now(UTC),
                    "organization_id": credential.organization_id,
                    "service_principal_id": credential.service_principal_id,
                    "credential_id": credential.id,
                },
            ).scalar_one_or_none()
            is not None
        )

    def get_credential_for_authentication(
        self,
        *,
        organization_id: UUID,
        credential_id: UUID,
    ) -> tuple[ServicePrincipal, ServiceCredential] | None:
        prepare_service_credential_lookup(self._session, organization_id)
        row = (
            self._session.execute(
                text(
                    """
                    SELECT
                        sp.id AS principal_id,
                        sp.organization_id,
                        sp.name AS principal_name,
                        sp.description,
                        sp.status AS principal_status,
                        sp.created_at AS principal_created_at,
                        sp.created_by_actor_type,
                        sp.created_by_actor_id,
                        sp.revoked_at AS principal_revoked_at,
                        c.id AS credential_id,
                        c.service_principal_id,
                        c.name AS credential_name,
                        c.prefix,
                        c.secret_hash,
                        c.permission_ceiling,
                        c.created_at AS credential_created_at,
                        c.expires_at,
                        c.last_used_at,
                        c.revoked_at AS credential_revoked_at
                    FROM org.service_credentials AS c
                    JOIN org.service_principals AS sp
                      ON sp.organization_id = c.organization_id
                     AND sp.id = c.service_principal_id
                    JOIN org.organizations AS o
                      ON o.id = c.organization_id
                    WHERE c.organization_id = :organization_id
                      AND c.id = :credential_id
                      AND c.revoked_at IS NULL
                      AND (c.expires_at IS NULL OR c.expires_at > :now)
                      AND sp.status = 'active'
                      AND sp.revoked_at IS NULL
                      AND o.status = 'active'
                    """
                ),
                {
                    "organization_id": organization_id,
                    "credential_id": credential_id,
                    "now": datetime.now(UTC),
                },
            )
            .mappings()
            .one_or_none()
        )
        if row is None:
            return None
        return _principal_from_row(row), _credential_from_row(row)
