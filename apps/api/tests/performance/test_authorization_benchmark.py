from __future__ import annotations

from math import ceil
from time import perf_counter_ns
from uuid import UUID, uuid4

from sqlalchemy import text

from openvietgis_api.adapters.persistence.authorization_repository import (
    SqlEffectivePermissionProvider,
)
from openvietgis_api.adapters.persistence.db import get_engine, get_session_factory
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.roles import RESOURCE_ROLE_PERMISSIONS
from openvietgis_api.domains.tenancy.domain import ResourceRole
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import AccessWorld, seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user

EXTRA_WORKSPACES = 30
EXTRA_PROJECTS = 30
SAMPLES_PER_PHASE = 25
BUDGET_MS = 50.0


def _seed_grants(world: AccessWorld) -> tuple[UUID, UUID, UUID]:
    # Three grants apply to the selected project (one of each scope); 60 do not.
    # Irrelevant grants belong to the same member and tenant, so a query cannot
    # eliminate the load by simply filtering on organization/membership.
    workspace_ids = [uuid4() for _ in range(EXTRA_WORKSPACES)]
    project_ids = [uuid4() for _ in range(EXTRA_PROJECTS)]
    organization_grant, workspace_grant, project_grant = uuid4(), uuid4(), uuid4()
    with get_admin_engine().begin() as connection:
        connection.execute(
            text("""
                INSERT INTO workspace.workspaces (
                    id, organization_id, name, slug, status,
                    created_by_actor_type, created_by_actor_id, created_at, updated_at
                ) VALUES (
                    :id, :org, 'Benchmark workspace', :slug, 'active',
                    'human', :owner, now(), now()
                )
            """),
            [
                {
                    "id": workspace_id,
                    "org": world.organization_id,
                    "slug": f"benchmark-{workspace_id.hex}",
                    "owner": world.owner_user_id,
                }
                for workspace_id in workspace_ids
            ],
        )
        connection.execute(
            text("""
                INSERT INTO workspace.projects (
                    id, organization_id, workspace_id, name, slug, status,
                    created_by_actor_type, created_by_actor_id, created_at, updated_at
                ) VALUES (
                    :id, :org, :workspace, 'Benchmark project', :slug, 'active',
                    'human', :owner, now(), now()
                )
            """),
            [
                {
                    "id": project_id,
                    "org": world.organization_id,
                    "workspace": workspace_ids[index % len(workspace_ids)],
                    "slug": f"benchmark-{project_id.hex}",
                    "owner": world.owner_user_id,
                }
                for index, project_id in enumerate(project_ids)
            ],
        )

        grants = [
            (organization_grant, "organization", None, None, "viewer"),
            (workspace_grant, "workspace", world.workspace_a_id, None, "manager"),
            (project_grant, "project", None, world.project_id, "editor"),
        ]
        grants.extend(
            (uuid4(), "workspace", workspace_id, None, "manager") for workspace_id in workspace_ids
        )
        grants.extend(
            (uuid4(), "project", None, project_id, "manager") for project_id in project_ids
        )
        connection.execute(
            text("""
                INSERT INTO org.resource_grants (
                    id, organization_id, subject_kind, membership_id, scope_kind,
                    workspace_id, project_id, role, created_at,
                    created_by_actor_type, created_by_actor_id
                ) VALUES (
                    :id, :org, 'membership', :member, :scope,
                    :workspace, :project, :role, now(), 'human', :owner
                )
            """),
            [
                {
                    "id": grant_id,
                    "org": world.organization_id,
                    "member": world.member_membership_id,
                    "scope": scope,
                    "workspace": workspace_id,
                    "project": project_id,
                    "role": role,
                    "owner": world.owner_user_id,
                }
                for grant_id, scope, workspace_id, project_id, role in grants
            ],
        )
        assert (
            connection.execute(
                text("""
                SELECT count(*) FROM org.resource_grants
                WHERE organization_id = :org AND membership_id = :member
                  AND revoked_at IS NULL
            """),
                {"org": world.organization_id, "member": world.member_membership_id},
            ).scalar_one()
            == 63
        )
    return organization_grant, workspace_grant, project_grant


def _p95_ms(samples: list[float]) -> float:
    return sorted(samples)[ceil(0.95 * len(samples)) - 1]


def _measure_fresh_resolution(
    context: AuthorizationContext, expected: frozenset[Permission]
) -> float:
    # New transaction and new resolver for every sample; no cached authorization
    # result. Include DB checkout, live actor/member context binding, placement
    # lookup, grant selection and permission union. Exclude fixture setup, HTTP
    # serialization and transaction cleanup (this is repository-level latency).
    with get_session_factory()() as session:
        provider = SqlEffectivePermissionProvider(session)
        start = perf_counter_ns()
        resolved = provider.resolve(context)
        elapsed_ms = (perf_counter_ns() - start) / 1_000_000
        assert resolved == expected
        session.rollback()
    return elapsed_ms


def test_authorization_p95_dynamic_scope_and_next_request_revocation(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    org_grant, workspace_grant, project_grant = _seed_grants(world)
    context = AuthorizationContext(
        actor=Actor(
            actor_type="human",
            actor_id=world.member_user_id,
            display_name="Benchmark Member",
        ),
        organization_id=world.organization_id,
        project_id=world.project_id,
    )
    member_permissions = frozenset({Permission.ORGANIZATION_READ})
    viewer = member_permissions | RESOURCE_ROLE_PERMISSIONS[ResourceRole.VIEWER]
    editor = viewer | RESOURCE_ROLE_PERMISSIONS[ResourceRole.EDITOR]
    manager = editor | RESOURCE_ROLE_PERMISSIONS[ResourceRole.MANAGER]

    # Warm only the runtime connection/pool, not any permission result. The
    # production runtime engine itself enforces the non-owner/no-BYPASSRLS role.
    with get_engine().connect() as connection:
        assert connection.execute(text("SELECT current_user")).scalar_one() == (
            "openvietgis_runtime"
        )

    results: dict[str, list[float]] = {}

    def measure_phase(name: str, expected: frozenset[Permission]) -> None:
        # The very first sample after each committed mutation is asserted and
        # timed: a stale result must fail on the next transaction, not after TTL.
        samples = [_measure_fresh_resolution(context, expected) for _ in range(SAMPLES_PER_PHASE)]
        results[name] = samples

    root = f"/api/v1/organizations/{world.organization_id}"
    project_url = f"{root}/projects/{world.project_id}"
    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(world.member_user_id) as member,
    ):
        measure_phase("workspace_a_inherits_manager", manager)
        assert member.get(project_url).status_code == 200

        moved = owner.patch(project_url, json={"workspace_id": str(world.workspace_b_id)})
        assert moved.status_code == 200, moved.text
        # Workspace grant no longer applies; organization and direct project grants remain.
        measure_phase("workspace_b_loses_manager", editor)

        assert owner.post(f"{root}/grants/{project_grant}/revoke").status_code == 204
        measure_phase("direct_project_grant_revoked", viewer)

        moved_back = owner.patch(project_url, json={"workspace_id": str(world.workspace_a_id)})
        assert moved_back.status_code == 200, moved_back.text
        measure_phase("workspace_a_inheritance_returns", manager)

        assert owner.post(f"{root}/grants/{workspace_grant}/revoke").status_code == 204
        measure_phase("workspace_grant_revoked", viewer)

        assert owner.post(f"{root}/grants/{org_grant}/revoke").status_code == 204
        measure_phase("organization_grant_revoked", member_permissions)
        denied = member.get(project_url)
        assert denied.status_code == 403, denied.text

    total_samples = [sample for samples in results.values() for sample in samples]
    phase_p95 = {phase: _p95_ms(samples) for phase, samples in results.items()}
    total_p95 = _p95_ms(total_samples)
    print(
        f"authorization benchmark runtime=openvietgis_runtime "
        f"grants=63(3 relevant,60 irrelevant initially) samples={len(total_samples)} "
        f"p95_ms={total_p95:.3f} "
        f"phases={', '.join(f'{phase}:{ms:.3f}ms' for phase, ms in phase_p95.items())}"
    )
    # The combined percentile is not allowed to hide a slow lifecycle phase.
    assert total_p95 <= BUDGET_MS
    assert all(ms <= BUDGET_MS for ms in phase_p95.values()), phase_p95
