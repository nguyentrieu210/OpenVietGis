from __future__ import annotations

from collections.abc import Iterator
from uuid import UUID, uuid4

import pytest
from fastapi.testclient import TestClient
from sqlalchemy import text

from openvietgis_api.config import get_settings
from openvietgis_api.main import create_app
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user

ROOT = "/api/v1/bootstrap"


@pytest.fixture
def uninitialized(migrated_database: None) -> Iterator[None]:
    with get_admin_engine().begin() as connection:
        original = dict(
            connection.execute(
                text("""
            SELECT initialized_at, initial_organization_id, initialized_by_user_id, updated_at
            FROM platform.installation_state WHERE singleton_key = 'installation'
        """)
            )
            .mappings()
            .one()
        )
        connection.execute(
            text("""
            UPDATE platform.installation_state
            SET initialized_at = NULL, initial_organization_id = NULL,
                initialized_by_user_id = NULL, updated_at = now()
            WHERE singleton_key = 'installation'
        """)
        )
    try:
        yield
    finally:
        with get_admin_engine().begin() as connection:
            connection.execute(
                text("""
                UPDATE platform.installation_state
                SET initialized_at = :initialized_at,
                    initial_organization_id = :initial_organization_id,
                    initialized_by_user_id = :initialized_by_user_id,
                    updated_at = :updated_at
                WHERE singleton_key = 'installation'
            """),
                original,
            )


def _payload() -> dict[str, str]:
    return {"organization_name": "Bootstrap fixture", "organization_slug": f"boot-{uuid4().hex}"}


def _authority() -> dict[str, str]:
    return {"Authorization": "Bearer " + get_settings().bootstrap_secret.get_secret_value()}


def test_bootstrap_requires_session_and_authority_and_commits_once(
    uninitialized: None,
) -> None:
    world = seed_access_world()
    payload = _payload()
    with TestClient(create_app()) as anonymous, client_for_user(world.wrong_user_id) as user:
        assert anonymous.get(f"{ROOT}/status").json() == {"initialized": False}
        assert anonymous.post(f"{ROOT}/initialize", json=payload).status_code == 401
        assert (
            anonymous.post(f"{ROOT}/initialize", json=payload, headers=_authority()).status_code
            == 401
        )
        assert user.post(f"{ROOT}/initialize", json=payload).status_code == 403
        assert (
            user.post(
                f"{ROOT}/initialize", json=payload, headers={"Authorization": "Bearer wrong"}
            ).status_code
            == 403
        )
        assert (
            user.post(
                f"{ROOT}/initialize",
                json=payload,
                headers={**_authority(), "X-OpenVietGIS-CSRF": "invalid"},
            ).status_code
            == 403
        )
        assert anonymous.get(f"{ROOT}/status").json() == {"initialized": False}
        result = user.post(f"{ROOT}/initialize", json=payload, headers=_authority())
        assert result.status_code == 201, result.text
        org = UUID(result.json()["organization_id"])
        member = UUID(result.json()["owner_membership_id"])
        assert anonymous.get(f"{ROOT}/status").json() == {"initialized": True}
        assert (
            user.post(f"{ROOT}/initialize", json=_payload(), headers=_authority()).status_code
            == 409
        )
        assert user.get(f"/api/v1/organizations/{org}").status_code == 200
    with get_admin_engine().connect() as connection:
        stored = connection.execute(
            text("""
            SELECT initialized_at, initial_organization_id, initialized_by_user_id
            FROM platform.installation_state WHERE singleton_key = 'installation'
        """)
        ).one()
        assert stored.initialized_at is not None
        assert stored.initial_organization_id == org
        assert stored.initialized_by_user_id == world.wrong_user_id
        membership = connection.execute(
            text("""
            SELECT user_id, organization_id, role, status
            FROM org.organization_memberships WHERE id = :id
        """),
            {"id": member},
        ).one()
        assert tuple(membership) == (world.wrong_user_id, org, "owner", "active")
        audit = connection.execute(
            text("""
            SELECT actor_id, action, outcome FROM audit.audit_events
            WHERE organization_id = :org AND action = 'installation.bootstrap.initialize'
        """),
            {"org": org},
        ).one()
        assert tuple(audit) == (world.wrong_user_id, "installation.bootstrap.initialize", "success")
