from __future__ import annotations

from uuid import UUID, uuid4

import pytest
from fastapi.testclient import TestClient
from sqlalchemy import text

from openvietgis_api.adapters.persistence.db import get_engine
from tests.fixtures.identity_tenancy import AccessWorld, seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user


def _verify_runtime_role() -> None:
    with get_engine().connect() as connection:
        assert connection.execute(text("SELECT current_user")).scalar_one() == (
            "openvietgis_runtime"
        )


def _viewer_grant(
    owner: TestClient, world: AccessWorld, *, workspace_id: UUID | None = None
) -> str:
    root = f"/api/v1/organizations/{world.organization_id}"
    scope = (
        {"scope_kind": "workspace", "workspace_id": str(workspace_id)}
        if workspace_id is not None
        else {"scope_kind": "project", "project_id": str(world.project_id)}
    )
    issued = owner.post(
        f"{root}/grants",
        json={
            "subject_kind": "membership",
            "subject_id": str(world.member_membership_id),
            "role": "viewer",
            **scope,
        },
    )
    assert issued.status_code == 201, issued.text
    return issued.json()["id"]


def _visible_projects(client: TestClient, root: str) -> set[str]:
    response = client.get(f"{root}/projects")
    assert response.status_code == 200, response.text
    return {item["id"] for item in response.json()}


def test_http_grant_revocation_takes_effect_on_same_authenticated_session(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    _verify_runtime_role()
    root = f"/api/v1/organizations/{world.organization_id}"
    url = f"{root}/projects/{world.project_id}"
    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(world.member_user_id) as member,
    ):
        original_session = member.cookies.get("ovg_session")
        assert original_session
        assert member.get(url).status_code == 403
        assert str(world.project_id) not in _visible_projects(member, root)
        grant_id = _viewer_grant(owner, world)
        granted = member.get(url)
        assert granted.status_code == 200
        assert granted.json()["id"] == str(world.project_id)

        assert owner.post(f"{root}/grants/{grant_id}/revoke").status_code == 204
        # No intervening login, session refresh, wait, or cache invalidation.
        denied = member.get(url)
        assert denied.status_code == 403
        assert str(world.project_id) not in _visible_projects(member, root)
        assert member.get("/api/v1/me").status_code == 200
        assert member.cookies.get("ovg_session") == original_session


@pytest.mark.parametrize("status", ["suspended", "revoked"])
def test_http_membership_suspension_or_revocation_denies_next_request(
    migrated_database: None, status: str
) -> None:
    world = seed_access_world()
    _verify_runtime_role()
    root = f"/api/v1/organizations/{world.organization_id}"
    url = f"{root}/projects/{world.project_id}"
    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(world.member_user_id) as member,
    ):
        original_session = member.cookies.get("ovg_session")
        assert original_session
        _viewer_grant(owner, world)
        assert member.get(url).status_code == 200

        changed = owner.patch(
            f"{root}/members/{world.member_membership_id}", json={"status": status}
        )
        assert changed.status_code == 200, changed.text
        assert changed.json()["status"] == status
        denied = member.get(url)
        assert denied.status_code == 404
        assert member.get(f"{root}/projects").status_code == 404
        # Global identity session remains authenticated, but cannot expose this tenant.
        assert member.get("/api/v1/me").status_code == 200
        assert member.cookies.get("ovg_session") == original_session


def test_http_workspace_inheritance_tracks_project_moves_without_grant_copying(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    _verify_runtime_role()
    root = f"/api/v1/organizations/{world.organization_id}"
    url = f"{root}/projects/{world.project_id}"
    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(world.member_user_id) as member,
    ):
        workspace_grant = _viewer_grant(owner, world, workspace_id=world.workspace_a_id)
        assert member.get(url).status_code == 200
        assert str(world.project_id) in _visible_projects(member, root)

        moved = owner.patch(url, json={"workspace_id": str(world.workspace_b_id)})
        assert moved.status_code == 200, moved.text
        assert moved.json()["workspace_id"] == str(world.workspace_b_id)
        assert moved.json()["organization_id"] == str(world.organization_id)
        assert member.get(url).status_code == 403
        assert str(world.project_id) not in _visible_projects(member, root)
        grants_after_move = owner.get(f"{root}/grants")
        assert grants_after_move.status_code == 200
        assert {item["id"] for item in grants_after_move.json()} == {workspace_grant}

        project_grant = _viewer_grant(owner, world)
        assert member.get(url).status_code == 200
        assert str(world.project_id) in _visible_projects(member, root)

        workspace = owner.post(
            f"{root}/workspaces",
            json={"name": "Workspace C", "slug": f"workspace-c-{uuid4().hex}"},
        )
        assert workspace.status_code == 201, workspace.text
        workspace_c = workspace.json()["id"]
        moved_again = owner.patch(url, json={"workspace_id": workspace_c})
        assert moved_again.status_code == 200, moved_again.text
        assert moved_again.json()["workspace_id"] == workspace_c
        assert moved_again.json()["organization_id"] == str(world.organization_id)
        still_visible = member.get(url)
        assert still_visible.status_code == 200
        assert still_visible.json()["workspace_id"] == workspace_c
        assert str(world.project_id) in _visible_projects(member, root)
        grants_after_second_move = owner.get(f"{root}/grants")
        assert grants_after_second_move.status_code == 200
        assert {item["id"] for item in grants_after_second_move.json()} == {
            workspace_grant,
            project_grant,
        }


def test_foreign_workspace_create_and_move_payloads_are_non_disclosing(
    migrated_database: None,
) -> None:
    a, b = seed_access_world(), seed_access_world()
    _verify_runtime_role()
    root = f"/api/v1/organizations/{a.organization_id}"
    url = f"{root}/projects/{a.project_id}"
    with (
        client_for_user(a.owner_user_id) as owner,
        client_for_user(b.owner_user_id) as other_owner,
        client_for_user(a.member_user_id) as member,
    ):
        before = owner.get(url)
        assert before.status_code == 200
        initial_projects = _visible_projects(owner, root)
        assert initial_projects == {str(a.project_id)}
        absent = str(uuid4())
        foreign = str(b.workspace_a_id)

        for actor in (owner, member):
            create_results = [
                actor.post(
                    f"{root}/projects",
                    json={
                        "name": "Blocked creation",
                        "slug": f"blocked-{uuid4().hex}",
                        "workspace_id": workspace_id,
                    },
                )
                for workspace_id in (foreign, absent)
            ]
            move_results = [
                actor.patch(url, json={"workspace_id": workspace_id})
                for workspace_id in (foreign, absent)
            ]
            for foreign_response, absent_response in (create_results, move_results):
                assert foreign_response.status_code == absent_response.status_code == 404
                assert foreign_response.json() == absent_response.json()
                assert foreign not in foreign_response.text

        # Same-tenant but insufficient permission is 403, unlike foreign or absent.
        same_tenant = member.post(
            f"{root}/projects",
            json={
                "name": "No create permission",
                "slug": f"blocked-{uuid4().hex}",
                "workspace_id": str(a.workspace_a_id),
            },
        )
        assert same_tenant.status_code == 403
        assert member.patch(url, json={"workspace_id": str(a.workspace_b_id)}).status_code == 403
        assert owner.get(url).json() == before.json()
        assert _visible_projects(owner, root) == initial_projects
        foreign_project = other_owner.get(
            f"/api/v1/organizations/{b.organization_id}/projects/{b.project_id}"
        )
        assert foreign_project.status_code == 200
        assert foreign_project.json()["workspace_id"] == str(b.workspace_a_id)
