from uuid import uuid4

from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.db import get_session_factory
from openvietgis_api.adapters.persistence.platform_repository import SqlPlatformRepository
from openvietgis_api.adapters.persistence.rls import bind_authenticated_actor
from openvietgis_api.adapters.persistence.tenancy_repository import SqlTenancyRepository
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from openvietgis_api.domains.tenancy.service import TenancyService
from tests.fixtures.identity_tenancy import seed_access_world


def test_real_tenant_repository_persists_stable_identity_and_timestamps(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    actor = Actor("human", world.owner_user_id, "Owner")
    context = AuthorizationContext(actor, world.organization_id)
    with get_session_factory()() as session:
        bind_authenticated_actor(session, actor)
        repository = SqlTenancyRepository(session)
        service = TenancyService(
            repository, SqlPlatformRepository(session), SqlAuditRepository(session)
        )
        created = service.create_project(
            actor=actor,
            organization_id=world.organization_id,
            name="  repository-check  ",
            slug=f"repo-{uuid4().hex}",
        )
        session.commit()
    assert created.name == "repository-check"
    assert created.created_at == created.updated_at
    assert created.created_at.tzinfo is not None

    with get_session_factory()() as session:
        bind_authenticated_actor(session, actor)
        loaded = SqlTenancyRepository(session).get_project(
            context=context, organization_id=world.organization_id, project_id=created.id
        )
        assert loaded == created
