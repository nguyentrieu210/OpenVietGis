from __future__ import annotations

import re
from dataclasses import dataclass, replace
from datetime import UTC, datetime
from uuid import UUID, uuid4

from openvietgis_api.domains.audit.domain import AuditEvent, AuditOutcome
from openvietgis_api.domains.audit.ports import AuditRepository
from openvietgis_api.domains.authorization.context import (
    Actor,
    AuthorizationContext,
    InvitationAcceptanceContext,
    TenantContext,
)
from openvietgis_api.domains.authorization.domain import ResourceGrant
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.ports import ResourceGrantRepository
from openvietgis_api.domains.authorization.roles import ORGANIZATION_ROLE_PERMISSIONS
from openvietgis_api.domains.identity.domain import UserStatus
from openvietgis_api.domains.identity.ports import IdentityRepository
from openvietgis_api.domains.tenancy.domain import (
    ActorType,
    Invitation,
    InvitationStatus,
    InvitationTargetKind,
    MembershipRole,
    MembershipStatus,
    Organization,
    OrganizationMembership,
    OrganizationStatus,
    Project,
    ProjectStatus,
    ResourceRole,
    ResourceScopeKind,
    Workspace,
    WorkspaceStatus,
)
from openvietgis_api.domains.tenancy.ports import InstallationStateReader, TenancyRepository
from openvietgis_api.security.secrets import (
    digest_secret,
    generate_secret,
    safe_prefix,
    verify_secret,
)

SLUG_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")


class InstallationNotInitializedError(RuntimeError):
    pass


class InvalidOrganizationInputError(ValueError):
    pass


class WorkspaceNotFoundError(LookupError):
    pass


class ProjectNotFoundError(LookupError):
    pass


class WorkspaceHasActiveProjectsError(RuntimeError):
    pass


class InvalidLifecycleTransitionError(ValueError):
    pass


class CrossOrganizationWorkspaceError(ValueError):
    pass


class OrganizationNotFoundError(LookupError):
    pass


class MembershipNotFoundError(LookupError):
    pass


class LastOwnerError(RuntimeError):
    pass


class OwnerRequiredError(PermissionError):
    pass


class InvitationNotFoundError(LookupError):
    pass


class InvalidInvitationError(ValueError):
    pass


class ExpiredInvitationError(InvalidInvitationError):
    """A verified invitation expired; persist only its terminal state."""


class InvitationTargetMismatchError(PermissionError):
    pass


@dataclass(frozen=True, slots=True)
class IssuedInvitation:
    invitation: Invitation
    secret: str


def _actor_type(actor: Actor) -> ActorType:
    return ActorType(actor.actor_type)


def _clean_name(value: str) -> str:
    normalized = value.strip()
    if not normalized:
        raise ValueError("name is required")
    return normalized


def _clean_slug(value: str) -> str:
    normalized = value.strip().lower()
    if not SLUG_RE.fullmatch(normalized):
        raise ValueError("invalid slug")
    return normalized


def _lock_active_organization(
    repository: TenancyRepository,
    organization_id: UUID,
    *,
    context: TenantContext,
) -> Organization:
    """Serialize tenant lifecycle and membership/invitation mutations on one row."""
    organization = repository.get_organization(organization_id, for_update=True, context=context)
    if organization is None:
        raise OrganizationNotFoundError("organization not found")
    if organization.status != OrganizationStatus.ACTIVE:
        raise InvalidLifecycleTransitionError("organization is not active")
    return organization


def _require_member_permission(
    repository: TenancyRepository,
    *,
    actor: Actor,
    organization_id: UUID,
    permission: Permission,
) -> OrganizationMembership:
    # Call only while holding the organization row lock. Revocation cannot race
    # the authorization decision for these administrative mutations.
    if actor.actor_type != "human" or actor.actor_id is None:
        raise OwnerRequiredError("active human membership required")
    membership = repository.get_membership_for_user(
        context=AuthorizationContext(actor, organization_id),
        organization_id=organization_id,
        user_id=actor.actor_id,
        for_update=True,
    )
    if (
        membership is None
        or membership.status != MembershipStatus.ACTIVE
        or permission not in ORGANIZATION_ROLE_PERMISSIONS[membership.role]
    ):
        raise OwnerRequiredError("insufficient organization authority")
    return membership


class TenancyService:
    def __init__(
        self,
        repository: TenancyRepository,
        installation_state: InstallationStateReader,
        audit_repository: AuditRepository,
    ) -> None:
        self._repository = repository
        self._installation_state = installation_state
        self._audit_repository = audit_repository

    def _audit(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        action: str,
        resource_type: str,
        resource_id: UUID,
        metadata: dict[str, object] | None = None,
        now: datetime,
    ) -> None:
        self._audit_repository.append(
            AuditEvent(
                id=uuid4(),
                organization_id=organization_id,
                actor_type=_actor_type(actor),
                actor_id=actor.actor_id,
                actor_display_snapshot=actor.display_name,
                action=action,
                resource_type=resource_type,
                resource_id=resource_id,
                outcome=AuditOutcome.SUCCESS,
                metadata=metadata or {},
                created_at=now,
            )
        )

    def create_organization(
        self,
        *,
        actor: Actor,
        name: str,
        slug: str,
        now: datetime | None = None,
    ) -> tuple[Organization, OrganizationMembership]:
        if actor.actor_type != "human" or actor.actor_id is None:
            raise InvalidOrganizationInputError("human actor required")
        if not self._installation_state.is_initialized():
            raise InstallationNotInitializedError("installation is not initialized")
        try:
            normalized_name = _clean_name(name)
            normalized_slug = _clean_slug(slug)
        except ValueError as exc:
            raise InvalidOrganizationInputError(str(exc)) from exc

        now = now or datetime.now(UTC)
        organization = Organization(
            id=uuid4(),
            name=normalized_name,
            slug=normalized_slug,
            status=OrganizationStatus.ACTIVE,
            created_by_actor_type=ActorType.HUMAN,
            created_by_actor_id=actor.actor_id,
            created_at=now,
            updated_at=now,
            deleted_at=None,
        )
        membership = OrganizationMembership(
            id=uuid4(),
            organization_id=organization.id,
            user_id=actor.actor_id,
            role=MembershipRole.OWNER,
            status=MembershipStatus.ACTIVE,
            created_at=now,
            activated_at=now,
            suspended_at=None,
            revoked_at=None,
        )
        self._repository.create_organization_with_owner(
            organization, membership, context=AuthorizationContext(actor, organization.id)
        )
        self._audit(
            actor=actor,
            organization_id=organization.id,
            action="organization.create",
            resource_type="organization",
            resource_id=organization.id,
            metadata={"slug": organization.slug},
            now=now,
        )
        return organization, membership

    def create_workspace(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        name: str,
        slug: str,
        credential_id: UUID | None = None,
        now: datetime | None = None,
    ) -> Workspace:
        context = AuthorizationContext(actor, organization_id, credential_id=credential_id)
        _lock_active_organization(self._repository, organization_id, context=context)
        now = now or datetime.now(UTC)
        workspace = Workspace(
            id=uuid4(),
            organization_id=organization_id,
            name=_clean_name(name),
            slug=_clean_slug(slug),
            status=WorkspaceStatus.ACTIVE,
            created_by_actor_type=_actor_type(actor),
            created_by_actor_id=actor.actor_id,
            created_at=now,
            updated_at=now,
            archived_at=None,
            deleted_at=None,
        )
        self._repository.create_workspace(workspace, context=context)
        self._audit(
            actor=actor,
            organization_id=organization_id,
            action="workspace.create",
            resource_type="workspace",
            resource_id=workspace.id,
            metadata={"slug": workspace.slug},
            now=now,
        )
        return workspace

    def update_workspace(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        workspace_id: UUID,
        name: str | None = None,
        status: WorkspaceStatus | None = None,
        credential_id: UUID | None = None,
        now: datetime | None = None,
    ) -> Workspace:
        if name is None and status is None:
            raise InvalidLifecycleTransitionError("at least one change is required")
        context = AuthorizationContext(actor, organization_id, credential_id=credential_id)
        _lock_active_organization(self._repository, organization_id, context=context)
        workspace = self._repository.get_workspace(
            context=context,
            organization_id=organization_id,
            workspace_id=workspace_id,
        )
        if workspace is None:
            raise WorkspaceNotFoundError("workspace not found")
        if workspace.status == WorkspaceStatus.DELETED and (
            status != WorkspaceStatus.ACTIVE or name is not None
        ):
            raise InvalidLifecycleTransitionError(
                "deleted workspace requires a status-only restore"
            )

        now = now or datetime.now(UTC)
        new_status = status or workspace.status
        if new_status == WorkspaceStatus.DELETED:
            if (
                self._repository.count_active_projects_in_workspace(
                    context=context,
                    organization_id=organization_id,
                    workspace_id=workspace_id,
                )
                > 0
            ):
                raise WorkspaceHasActiveProjectsError(
                    "workspace with active projects cannot be deleted"
                )

        updated = replace(
            workspace,
            name=_clean_name(name) if name is not None else workspace.name,
            status=new_status,
            updated_at=now,
            archived_at=(
                now
                if new_status == WorkspaceStatus.ARCHIVED
                else None
                if new_status == WorkspaceStatus.ACTIVE
                else workspace.archived_at
            ),
            deleted_at=now if new_status == WorkspaceStatus.DELETED else None,
        )
        self._repository.update_workspace(updated, context=context)
        self._audit(
            actor=actor,
            organization_id=organization_id,
            action="workspace.update",
            resource_type="workspace",
            resource_id=workspace_id,
            metadata={"status": updated.status.value, "previous_status": workspace.status.value},
            now=now,
        )
        return updated

    def create_project(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        name: str,
        slug: str,
        workspace_id: UUID | None = None,
        credential_id: UUID | None = None,
        now: datetime | None = None,
    ) -> Project:
        context = AuthorizationContext(actor, organization_id, credential_id=credential_id)
        _lock_active_organization(self._repository, organization_id, context=context)
        if workspace_id is not None:
            workspace = self._repository.get_workspace(
                context=context,
                organization_id=organization_id,
                workspace_id=workspace_id,
            )
            if workspace is None:
                raise CrossOrganizationWorkspaceError("workspace does not belong to organization")
            if workspace.status == WorkspaceStatus.DELETED:
                raise InvalidLifecycleTransitionError("deleted workspace cannot receive projects")

        now = now or datetime.now(UTC)
        project = Project(
            id=uuid4(),
            organization_id=organization_id,
            workspace_id=workspace_id,
            name=_clean_name(name),
            slug=_clean_slug(slug),
            status=ProjectStatus.ACTIVE,
            created_by_actor_type=_actor_type(actor),
            created_by_actor_id=actor.actor_id,
            created_at=now,
            updated_at=now,
            archived_at=None,
            deleted_at=None,
        )
        self._repository.create_project(project, context=context)
        self._audit(
            actor=actor,
            organization_id=organization_id,
            action="project.create",
            resource_type="project",
            resource_id=project.id,
            metadata={"slug": project.slug},
            now=now,
        )
        return project

    def update_project(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        project_id: UUID,
        name: str | None = None,
        workspace_id: UUID | None = None,
        workspace_supplied: bool = False,
        status: ProjectStatus | None = None,
        credential_id: UUID | None = None,
        now: datetime | None = None,
    ) -> Project:
        if name is None and status is None and not workspace_supplied:
            raise InvalidLifecycleTransitionError("at least one change is required")
        context = AuthorizationContext(actor, organization_id, credential_id=credential_id)
        _lock_active_organization(self._repository, organization_id, context=context)
        project = self._repository.get_project(
            context=context,
            organization_id=organization_id,
            project_id=project_id,
        )
        if project is None:
            raise ProjectNotFoundError("project not found")
        if project.status == ProjectStatus.DELETED and (
            status != ProjectStatus.ACTIVE or name is not None or workspace_supplied
        ):
            raise InvalidLifecycleTransitionError("deleted project requires a status-only restore")
        if project.status == ProjectStatus.ARCHIVED and (
            name is not None or (workspace_supplied and status != ProjectStatus.ACTIVE)
        ):
            raise InvalidLifecycleTransitionError(
                "archived project must be restored before normal mutation"
            )

        target_workspace_id = project.workspace_id
        if workspace_supplied:
            target_workspace_id = workspace_id
            if workspace_id is not None:
                workspace = self._repository.get_workspace(
                    context=context,
                    organization_id=organization_id,
                    workspace_id=workspace_id,
                )
                if workspace is None:
                    raise CrossOrganizationWorkspaceError(
                        "workspace does not belong to organization"
                    )
                if workspace.status == WorkspaceStatus.DELETED:
                    raise InvalidLifecycleTransitionError(
                        "deleted workspace cannot receive projects"
                    )

        new_status = status or project.status
        if new_status == ProjectStatus.ACTIVE and target_workspace_id is not None:
            target_workspace = self._repository.get_workspace(
                context=context,
                organization_id=organization_id,
                workspace_id=target_workspace_id,
            )
            if project.status == ProjectStatus.DELETED and (
                target_workspace is None or target_workspace.status != WorkspaceStatus.ACTIVE
            ):
                raise InvalidLifecycleTransitionError(
                    "restore the workspace before the deleted project"
                )
            if target_workspace is None or target_workspace.status == WorkspaceStatus.DELETED:
                raise InvalidLifecycleTransitionError(
                    "restore project with an explicit valid workspace or ungroup it"
                )
        now = now or datetime.now(UTC)
        updated = replace(
            project,
            name=_clean_name(name) if name is not None else project.name,
            workspace_id=target_workspace_id,
            status=new_status,
            updated_at=now,
            archived_at=(
                now
                if new_status == ProjectStatus.ARCHIVED
                else None
                if new_status == ProjectStatus.ACTIVE
                else project.archived_at
            ),
            deleted_at=now if new_status == ProjectStatus.DELETED else None,
        )
        self._repository.update_project(updated, context=context)
        self._audit(
            actor=actor,
            organization_id=organization_id,
            action="project.update",
            resource_type="project",
            resource_id=project_id,
            metadata={
                "status": updated.status.value,
                "previous_status": project.status.value,
                "workspace_id": (
                    str(updated.workspace_id) if updated.workspace_id is not None else None
                ),
            },
            now=now,
        )
        return updated

    def update_organization(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        name: str | None = None,
        status: OrganizationStatus | None = None,
        now: datetime | None = None,
    ) -> Organization:
        context = AuthorizationContext(actor, organization_id)
        organization = self._repository.get_organization(
            organization_id,
            context=context,
            for_update=True,
        )
        if organization is None:
            raise OrganizationNotFoundError("organization not found")

        _require_member_permission(
            self._repository,
            actor=actor,
            organization_id=organization_id,
            permission=(
                Permission.ORGANIZATION_LIFECYCLE
                if status is not None
                else Permission.ORGANIZATION_MANAGE_SETTINGS
            ),
        )
        if organization.status != OrganizationStatus.ACTIVE and status is None:
            raise InvalidLifecycleTransitionError("organization is not active")
        if organization.status != OrganizationStatus.ACTIVE and name is not None:
            raise InvalidLifecycleTransitionError(
                "restore organization before changing its settings"
            )

        now = now or datetime.now(UTC)
        new_status = status or organization.status
        if organization.status == OrganizationStatus.DELETED and new_status not in {
            OrganizationStatus.DELETED,
            OrganizationStatus.ACTIVE,
        }:
            raise InvalidLifecycleTransitionError("deleted organization can only be restored")
        updated = replace(
            organization,
            name=_clean_name(name) if name is not None else organization.name,
            status=new_status,
            updated_at=now,
            deleted_at=(
                now
                if new_status == OrganizationStatus.DELETED
                else None
                if new_status == OrganizationStatus.ACTIVE
                else organization.deleted_at
            ),
        )
        self._repository.update_organization(updated, context=context)
        self._audit(
            actor=actor,
            organization_id=organization_id,
            action="organization.update",
            resource_type="organization",
            resource_id=organization_id,
            metadata={"status": updated.status.value},
            now=now,
        )
        return updated

    def update_membership(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        membership_id: UUID,
        role: MembershipRole | None = None,
        status: MembershipStatus | None = None,
        now: datetime | None = None,
    ) -> OrganizationMembership:
        context = AuthorizationContext(actor, organization_id)
        _lock_active_organization(self._repository, organization_id, context=context)
        caller_membership = _require_member_permission(
            self._repository,
            actor=actor,
            organization_id=organization_id,
            permission=Permission.ORGANIZATION_MANAGE_MEMBERS,
        )
        membership = self._repository.get_membership(
            context=context,
            organization_id=organization_id,
            membership_id=membership_id,
            for_update=True,
        )
        if membership is None:
            raise MembershipNotFoundError("membership not found")

        new_role = role or membership.role
        new_status = status or membership.status
        if (
            membership.role == MembershipRole.OWNER or new_role == MembershipRole.OWNER
        ) and caller_membership.role != MembershipRole.OWNER:
            raise OwnerRequiredError("Owner required for ownership changes")

        stops_being_active_owner = (
            membership.role == MembershipRole.OWNER
            and membership.status == MembershipStatus.ACTIVE
            and (new_role != MembershipRole.OWNER or new_status != MembershipStatus.ACTIVE)
        )
        if (
            stops_being_active_owner
            and self._repository.count_other_active_owners(
                context=context,
                organization_id=organization_id,
                excluding_membership_id=membership.id,
            )
            == 0
        ):
            raise LastOwnerError("organization must retain an active Owner")

        now = now or datetime.now(UTC)
        updated = replace(
            membership,
            role=new_role,
            status=new_status,
            activated_at=(
                now
                if new_status == MembershipStatus.ACTIVE
                and membership.status != MembershipStatus.ACTIVE
                else membership.activated_at
            ),
            suspended_at=(
                now
                if new_status == MembershipStatus.SUSPENDED
                else None
                if new_status == MembershipStatus.ACTIVE
                else membership.suspended_at
            ),
            revoked_at=(
                now
                if new_status == MembershipStatus.REVOKED
                else None
                if new_status == MembershipStatus.ACTIVE
                else membership.revoked_at
            ),
        )
        self._repository.update_membership(updated, context=context)
        self._audit(
            actor=actor,
            organization_id=organization_id,
            action="organization.membership.update",
            resource_type="organization_membership",
            resource_id=membership_id,
            metadata={"role": new_role.value, "status": new_status.value},
            now=now,
        )
        return updated

    def transfer_ownership(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        target_membership_id: UUID,
        now: datetime | None = None,
    ) -> tuple[OrganizationMembership, OrganizationMembership]:
        context = AuthorizationContext(actor, organization_id)
        if actor.actor_type != "human" or actor.actor_id is None:
            raise OwnerRequiredError("human Owner required")
        _lock_active_organization(self._repository, organization_id, context=context)
        current = self._repository.get_membership_for_user(
            context=context,
            organization_id=organization_id,
            user_id=actor.actor_id,
            for_update=True,
        )
        if (
            current is None
            or current.status != MembershipStatus.ACTIVE
            or current.role != MembershipRole.OWNER
        ):
            raise OwnerRequiredError("active Owner required")
        target = self._repository.get_membership(
            context=context,
            organization_id=organization_id,
            membership_id=target_membership_id,
            for_update=True,
        )
        if target is None or target.status != MembershipStatus.ACTIVE:
            raise MembershipNotFoundError("active target membership required")

        now = now or datetime.now(UTC)
        promoted = replace(target, role=MembershipRole.OWNER)
        self._repository.update_membership(promoted, context=context)
        if current.id == promoted.id:
            demoted = current
        else:
            demoted = replace(current, role=MembershipRole.ADMIN)
            self._repository.update_membership(demoted, context=context)

        self._audit(
            actor=actor,
            organization_id=organization_id,
            action="organization.owner.transfer",
            resource_type="organization_membership",
            resource_id=promoted.id,
            metadata={"previous_owner_membership_id": str(current.id)},
            now=now,
        )
        return demoted, promoted


class InvitationService:
    def __init__(
        self,
        tenancy_repository: TenancyRepository,
        identity_repository: IdentityRepository,
        grant_repository: ResourceGrantRepository,
        audit_repository: AuditRepository,
    ) -> None:
        self._tenancy_repository = tenancy_repository
        self._identity_repository = identity_repository
        self._grant_repository = grant_repository
        self._audit_repository = audit_repository

    def issue(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        target_kind: InvitationTargetKind,
        organization_role: MembershipRole,
        expires_at: datetime,
        target_email: str | None = None,
        target_user_id: UUID | None = None,
        target_issuer: str | None = None,
        target_subject: str | None = None,
        resource_scope_kind: ResourceScopeKind | None = None,
        resource_workspace_id: UUID | None = None,
        resource_project_id: UUID | None = None,
        resource_role: ResourceRole | None = None,
        now: datetime | None = None,
    ) -> IssuedInvitation:
        context = AuthorizationContext(actor, organization_id)
        _lock_active_organization(self._tenancy_repository, organization_id, context=context)
        inviter_membership = _require_member_permission(
            self._tenancy_repository,
            actor=actor,
            organization_id=organization_id,
            permission=Permission.INVITATION_MANAGE,
        )
        if (
            organization_role == MembershipRole.OWNER
            and inviter_membership.role != MembershipRole.OWNER
        ):
            raise OwnerRequiredError("Owner required for Owner invitation")
        now = now or datetime.now(UTC)
        if expires_at <= now:
            raise InvalidInvitationError("invitation expiry must be in the future")

        normalized_email = target_email.strip().lower() if target_email else None
        if target_kind == InvitationTargetKind.EMAIL:
            if not normalized_email or target_user_id or target_issuer or target_subject:
                raise InvalidInvitationError("email target requires only target_email")
        elif target_kind == InvitationTargetKind.USER:
            if target_user_id is None or normalized_email or target_issuer or target_subject:
                raise InvalidInvitationError("user target requires only target_user_id")
        elif target_kind == InvitationTargetKind.EXTERNAL_IDENTITY:
            if not target_issuer or not target_subject or normalized_email or target_user_id:
                raise InvalidInvitationError("external identity target requires issuer and subject")

        if resource_scope_kind is None:
            if any(
                value is not None
                for value in (
                    resource_workspace_id,
                    resource_project_id,
                    resource_role,
                )
            ):
                raise InvalidInvitationError("resource grant scope is incomplete")
        elif resource_role is None:
            raise InvalidInvitationError("resource grant requires a resource role")
        elif resource_scope_kind == ResourceScopeKind.WORKSPACE:
            if resource_workspace_id is None or resource_project_id is not None:
                raise InvalidInvitationError("workspace grant requires workspace_id")
            if (
                self._tenancy_repository.get_workspace(
                    context=context,
                    organization_id=organization_id,
                    workspace_id=resource_workspace_id,
                )
                is None
            ):
                raise InvalidInvitationError("workspace does not belong to organization")
        elif resource_scope_kind == ResourceScopeKind.PROJECT:
            if resource_project_id is None or resource_workspace_id is not None:
                raise InvalidInvitationError("project grant requires project_id")
            if (
                self._tenancy_repository.get_project(
                    context=context,
                    organization_id=organization_id,
                    project_id=resource_project_id,
                )
                is None
            ):
                raise InvalidInvitationError("project does not belong to organization")
        elif resource_scope_kind == ResourceScopeKind.ORGANIZATION:
            if resource_workspace_id is not None or resource_project_id is not None:
                raise InvalidInvitationError("organization grant cannot include resource id")

        secret = generate_secret("inv", bytes_of_entropy=32)
        invitation = Invitation(
            id=uuid4(),
            organization_id=organization_id,
            status=InvitationStatus.PENDING,
            target_kind=target_kind,
            target_email_normalized=normalized_email,
            target_user_id=target_user_id,
            target_issuer=target_issuer,
            target_subject=target_subject,
            organization_role=organization_role,
            resource_scope_kind=resource_scope_kind,
            resource_workspace_id=resource_workspace_id,
            resource_project_id=resource_project_id,
            resource_role=resource_role,
            token_hash=digest_secret(secret),
            token_prefix=safe_prefix(secret),
            expires_at=expires_at,
            created_at=now,
            created_by_actor_type=_actor_type(actor),
            created_by_actor_id=actor.actor_id,
            accepted_at=None,
            accepted_by_user_id=None,
            revoked_at=None,
        )
        self._tenancy_repository.create_invitation(invitation, context=context)
        self._audit_repository.append(
            AuditEvent(
                id=uuid4(),
                organization_id=organization_id,
                actor_type=_actor_type(actor),
                actor_id=actor.actor_id,
                actor_display_snapshot=actor.display_name,
                action="invitation.create",
                resource_type="invitation",
                resource_id=invitation.id,
                outcome=AuditOutcome.SUCCESS,
                metadata={
                    "target_kind": target_kind.value,
                    "organization_role": organization_role.value,
                },
                created_at=now,
            )
        )
        return IssuedInvitation(invitation=invitation, secret=secret)

    def _target_matches(
        self,
        invitation: Invitation,
        *,
        user_id: UUID,
        identity_link_id: UUID | None,
    ) -> bool:
        if invitation.target_kind == InvitationTargetKind.USER:
            return invitation.target_user_id == user_id

        # Email and external-identity invitations require proof from the identity
        # used to establish this authenticated session, not another linked identity.
        if identity_link_id is None:
            return False
        link = next(
            (
                item
                for item in self._identity_repository.list_identity_links(user_id)
                if item.id == identity_link_id and item.user_id == user_id
            ),
            None,
        )
        if link is None:
            return False
        if invitation.target_kind == InvitationTargetKind.EMAIL:
            return (
                link.email_verified
                and link.email_snapshot is not None
                and link.email_snapshot.strip().lower() == invitation.target_email_normalized
            )

        return link.issuer == invitation.target_issuer and link.subject == invitation.target_subject

    def check_recipient(
        self,
        *,
        actor: Actor,
        user_id: UUID,
        organization_id: UUID,
        invitation_id: UUID,
        secret: str,
        identity_link_id: UUID | None,
    ) -> str:
        """A verified link holder sees only whether the current session can accept."""
        if actor.actor_type != "human" or actor.actor_id != user_id:
            return "invalid"
        context = InvitationAcceptanceContext(actor, organization_id, invitation_id)
        invitation = self._tenancy_repository.get_invitation(
            context=context, organization_id=organization_id, invitation_id=invitation_id
        )
        if (
            invitation is None
            or invitation.status != InvitationStatus.PENDING
            or invitation.expires_at <= datetime.now(UTC)
            or not verify_secret(secret, invitation.token_hash)
        ):
            return "invalid"
        return (
            "eligible"
            if self._target_matches(invitation, user_id=user_id, identity_link_id=identity_link_id)
            else "mismatch"
        )

    def accept(
        self,
        *,
        actor: Actor,
        user_id: UUID,
        organization_id: UUID,
        invitation_id: UUID,
        secret: str,
        identity_link_id: UUID | None = None,
        now: datetime | None = None,
    ) -> tuple[OrganizationMembership, ResourceGrant | None]:
        if actor.actor_type != "human" or actor.actor_id != user_id:
            raise InvitationTargetMismatchError("authenticated human identity required")
        user = self._identity_repository.get_user(user_id)
        if user is None or user.status != UserStatus.ACTIVE:
            raise InvitationTargetMismatchError("authenticated human identity required")

        context = AuthorizationContext(actor, organization_id)
        invitation_context = InvitationAcceptanceContext(actor, organization_id, invitation_id)

        # The same organization row is acquired first for invitation issue,
        # acceptance/revocation, ownership transfer, and membership changes.
        try:
            _lock_active_organization(
                self._tenancy_repository, organization_id, context=invitation_context
            )
        except (OrganizationNotFoundError, InvalidLifecycleTransitionError) as exc:
            raise InvalidInvitationError("invalid invitation") from exc
        invitation = self._tenancy_repository.get_invitation(
            context=invitation_context,
            organization_id=organization_id,
            invitation_id=invitation_id,
            for_update=True,
        )
        if invitation is None:
            raise InvitationNotFoundError("invitation not found")
        now = now or datetime.now(UTC)
        if invitation.status != InvitationStatus.PENDING:
            raise InvalidInvitationError("invitation is no longer pending")
        if not verify_secret(secret, invitation.token_hash):
            raise InvalidInvitationError("invalid invitation secret")
        if not self._target_matches(invitation, user_id=user_id, identity_link_id=identity_link_id):
            raise InvitationTargetMismatchError("invitation target does not match identity")
        invitation_context = replace(invitation_context, verified=True)
        if invitation.expires_at <= now:
            expired = replace(invitation, status=InvitationStatus.EXPIRED)
            self._tenancy_repository.update_invitation(expired, context=invitation_context)
            raise ExpiredInvitationError("invitation has expired")

        # Assigning Owner requires a currently active Owner inviter. This also
        # invalidates outstanding Owner invitations after the inviter loses authority.
        if invitation.organization_role == MembershipRole.OWNER:
            inviter = (
                self._tenancy_repository.get_membership_for_user(
                    context=invitation_context,
                    organization_id=organization_id,
                    user_id=invitation.created_by_actor_id,
                    for_update=True,
                )
                if invitation.created_by_actor_type == ActorType.HUMAN
                and invitation.created_by_actor_id is not None
                else None
            )
            if (
                inviter is None
                or inviter.role != MembershipRole.OWNER
                or inviter.status != MembershipStatus.ACTIVE
            ):
                raise InvalidInvitationError("Owner invitation authority is no longer active")

        membership = self._tenancy_repository.get_membership_for_user(
            context=invitation_context,
            organization_id=organization_id,
            user_id=user_id,
            for_update=True,
        )
        if membership is None:
            membership = OrganizationMembership(
                id=uuid4(),
                organization_id=organization_id,
                user_id=user_id,
                role=invitation.organization_role,
                status=MembershipStatus.ACTIVE,
                created_at=now,
                activated_at=now,
                suspended_at=None,
                revoked_at=None,
            )
            self._tenancy_repository.create_membership(membership, context=invitation_context)
        else:
            # Invitation acceptance is not the ownership-transfer mechanism.
            # Never permit an invitation to downgrade an existing Owner.
            if (
                membership.role == MembershipRole.OWNER
                and invitation.organization_role != MembershipRole.OWNER
            ):
                raise InvalidInvitationError("invitation cannot downgrade an Owner")
            membership = replace(
                membership,
                role=invitation.organization_role,
                status=MembershipStatus.ACTIVE,
                activated_at=now,
                suspended_at=None,
                revoked_at=None,
            )
            self._tenancy_repository.update_membership(membership, context=invitation_context)

        grant: ResourceGrant | None = None
        if invitation.resource_scope_kind is not None:
            assert invitation.resource_role is not None
            grant = ResourceGrant(
                id=uuid4(),
                organization_id=organization_id,
                subject_kind="membership",
                membership_id=membership.id,
                service_principal_id=None,
                scope_kind=invitation.resource_scope_kind,
                workspace_id=invitation.resource_workspace_id,
                project_id=invitation.resource_project_id,
                role=invitation.resource_role,
                created_at=now,
                created_by_actor_type=_actor_type(actor),
                created_by_actor_id=actor.actor_id,
                revoked_at=None,
            )
            # Under the organization lock, reuse an identical active grant instead
            # of violating the active-grant uniqueness constraint on invitation retry.
            existing_grant = next(
                (
                    item
                    for item in self._grant_repository.list_for_organization(
                        organization_id, context=context
                    )
                    if item.membership_id == membership.id
                    and item.scope_kind == grant.scope_kind
                    and item.workspace_id == grant.workspace_id
                    and item.project_id == grant.project_id
                    and item.role == grant.role
                    and item.revoked_at is None
                ),
                None,
            )
            if existing_grant is not None:
                grant = existing_grant
            else:
                self._grant_repository.create(grant, context=context)
                self._audit_repository.append(
                    AuditEvent(
                        id=uuid4(),
                        organization_id=organization_id,
                        actor_type=_actor_type(actor),
                        actor_id=actor.actor_id,
                        actor_display_snapshot=actor.display_name,
                        action="authorization.grant.create",
                        resource_type="resource_grant",
                        resource_id=grant.id,
                        outcome=AuditOutcome.SUCCESS,
                        metadata={
                            "scope_kind": grant.scope_kind.value,
                            "role": grant.role.value,
                            "membership_id": str(membership.id),
                            "invitation_id": str(invitation.id),
                        },
                        created_at=now,
                    )
                )

        accepted = replace(
            invitation,
            status=InvitationStatus.ACCEPTED,
            accepted_at=now,
            accepted_by_user_id=user_id,
        )
        self._tenancy_repository.update_invitation(accepted, context=invitation_context)
        self._audit_repository.append(
            AuditEvent(
                id=uuid4(),
                organization_id=organization_id,
                actor_type=_actor_type(actor),
                actor_id=actor.actor_id,
                actor_display_snapshot=actor.display_name,
                action="invitation.accept",
                resource_type="invitation",
                resource_id=invitation.id,
                outcome=AuditOutcome.SUCCESS,
                metadata={
                    "membership_id": str(membership.id),
                    **({"grant_id": str(grant.id)} if grant is not None else {}),
                },
                created_at=now,
            )
        )
        return membership, grant

    def revoke(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        invitation_id: UUID,
        now: datetime | None = None,
    ) -> Invitation:
        context = AuthorizationContext(actor, organization_id)
        _lock_active_organization(self._tenancy_repository, organization_id, context=context)
        revoker = _require_member_permission(
            self._tenancy_repository,
            actor=actor,
            organization_id=organization_id,
            permission=Permission.INVITATION_MANAGE,
        )
        invitation = self._tenancy_repository.get_invitation(
            context=context,
            organization_id=organization_id,
            invitation_id=invitation_id,
            for_update=True,
        )
        if invitation is None:
            raise InvitationNotFoundError("invitation not found")
        if (
            invitation.organization_role == MembershipRole.OWNER
            and revoker.role != MembershipRole.OWNER
        ):
            raise OwnerRequiredError("Owner required to revoke an Owner invitation")
        if invitation.status != InvitationStatus.PENDING:
            raise InvalidInvitationError("only pending invitation can be revoked")
        now = now or datetime.now(UTC)
        revoked = replace(
            invitation,
            status=InvitationStatus.REVOKED,
            revoked_at=now,
        )
        self._tenancy_repository.update_invitation(revoked, context=context)
        self._audit_repository.append(
            AuditEvent(
                id=uuid4(),
                organization_id=organization_id,
                actor_type=_actor_type(actor),
                actor_id=actor.actor_id,
                actor_display_snapshot=actor.display_name,
                action="invitation.revoke",
                resource_type="invitation",
                resource_id=invitation.id,
                outcome=AuditOutcome.SUCCESS,
                metadata={},
                created_at=now,
            )
        )
        return revoked
