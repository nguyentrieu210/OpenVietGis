from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from threading import Event
from uuid import UUID, uuid4

import pytest
from fastapi.testclient import TestClient
from sqlalchemy import text
from sqlalchemy.exc import OperationalError

from openvietgis_api.adapters.persistence.authorization_repository import (
    SqlServicePrincipalRepository,
)
from openvietgis_api.main import create_app
from openvietgis_api.security.secrets import digest_secret
from openvietgis_api.security.sessions import issue_session_token
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world


def _client_for_user(user_id: UUID) -> TestClient:
    session_id = uuid4()
    raw_token, digest = issue_session_token(session_id)
    with get_admin_engine().begin() as connection:
        link_id = connection.execute(
            text("SELECT id FROM identity.identity_links WHERE user_id=:user"),
            {"user": user_id},
        ).scalar_one()
        connection.execute(
            text(
                """
                INSERT INTO identity.web_sessions (
                    id, user_id, identity_link_id, secret_hash, issued_at, expires_at
                )
                VALUES (:id,:user,:link,:digest,now(),now()+interval '1 hour')
                """
            ),
            {
                "id": session_id,
                "user": user_id,
                "link": link_id,
                "digest": digest,
            },
        )
    client = TestClient(create_app())
    client.cookies.set("ovg_session", raw_token)
    csrf = client.get("/api/v1/auth/csrf")
    assert csrf.status_code == 200
    client.headers["X-OpenVietGIS-CSRF"] = csrf.json()["token"]
    return client


def _service_client(secret: str) -> TestClient:
    client = TestClient(create_app())
    client.headers["Authorization"] = "Bearer " + secret
    return client


def test_service_credential_allowed_scope_ceiling_cross_tenant_and_revocation(
    migrated_database: None,
) -> None:
    a, b = seed_access_world(), seed_access_world()
    root_a = f"/api/v1/organizations/{a.organization_id}"
    root_b = f"/api/v1/organizations/{b.organization_id}"

    with _client_for_user(a.owner_user_id) as owner:
        created = owner.post(
            f"{root_a}/service-principals",
            json={"name": "integration-check", "description": "P2 fixture"},
        )
        assert created.status_code == 201, created.text
        principal_id = created.json()["id"]

        grant = owner.post(
            f"{root_a}/grants",
            json={
                "subject_kind": "service_principal",
                "subject_id": principal_id,
                "scope_kind": "project",
                "project_id": str(a.project_id),
                "role": "viewer",
            },
        )
        assert grant.status_code == 201, grant.text
        unscoped = owner.post(
            f"{root_a}/projects",
            json={
                "name": "Unscoped project",
                "slug": f"unscoped-{uuid4().hex}",
                "workspace_id": str(a.workspace_a_id),
            },
        )
        assert unscoped.status_code == 201, unscoped.text
        unscoped_project_id = unscoped.json()["id"]

        too_wide = owner.post(
            f"{root_a}/service-principals/{principal_id}/credentials",
            json={
                "name": "too-wide",
                "permission_ceiling": ["project.read", "project.edit"],
            },
        )
        assert too_wide.status_code == 400, too_wide.text

        issued = owner.post(
            f"{root_a}/service-principals/{principal_id}/credentials",
            json={
                "name": "reader",
                "permission_ceiling": ["project.read"],
            },
        )
        assert issued.status_code == 201, issued.text
        credential_id = issued.json()["id"]
        metadata_url = f"{root_a}/service-principals/{principal_id}/credentials"
        metadata = owner.get(metadata_url)
        assert metadata.status_code == 200, metadata.text
        assert metadata.headers["cache-control"] == "no-store"
        assert metadata.json()[0]["id"] == credential_id
        assert metadata.json()[0]["permission_ceiling"] == ["project.read"]
        assert "secret" not in metadata.json()[0]
        assert "secret_hash" not in metadata.text
        with _client_for_user(a.member_user_id) as member:
            assert member.get(metadata_url).status_code == 403
        with _client_for_user(b.owner_user_id) as other:
            assert (
                other.get(f"{root_b}/service-principals/{principal_id}/credentials").status_code
                == 404
            )
        secret = issued.json()["secret"]
        assert secret.startswith("ovgsvc")
        assert "secret_hash" not in issued.text
        with get_admin_engine().connect() as connection:
            stored = connection.execute(
                text(
                    "SELECT prefix, secret_hash, last_used_at "
                    "FROM org.service_credentials WHERE id=:id"
                ),
                {"id": credential_id},
            ).one()
        assert stored.prefix == secret[:20]
        assert bytes(stored.secret_hash) == digest_secret(secret)
        assert stored.last_used_at is None

        with _service_client(secret) as service:
            allowed = service.get(f"{root_a}/projects/{a.project_id}")
            assert allowed.status_code == 200, allowed.text
            assert allowed.json()["id"] == str(a.project_id)
            activity = service.get(f"{root_a}/projects/{a.project_id}/collaboration/activities")
            assert activity.status_code == 200, activity.text
            assert isinstance(activity.json()["items"], list)
            journal = service.get(f"{root_a}/projects/{a.project_id}/integrations/events")
            assert journal.status_code == 200, journal.text
            denied_journal = service.get(
                f"{root_a}/projects/{unscoped_project_id}/integrations/events"
            )
            assert denied_journal.status_code == 403, denied_journal.text
            denied_activity = service.get(
                f"{root_a}/projects/{unscoped_project_id}/collaboration/activities"
            )
            assert denied_activity.status_code == 403, denied_activity.text

            denied_edit = service.patch(
                f"{root_a}/projects/{a.project_id}",
                json={"name": "service-must-not-edit"},
            )
            assert denied_edit.status_code == 403, denied_edit.text

            out_of_scope = service.get(f"{root_a}/projects/{unscoped_project_id}")
            assert out_of_scope.status_code == 403, out_of_scope.text

            cross_tenant = service.get(f"{root_b}/projects/{b.project_id}")
            assert cross_tenant.status_code == 401
            assert cross_tenant.json()["detail"] == "invalid service credential"

        with get_admin_engine().connect() as connection:
            last_used_at = connection.execute(
                text("SELECT last_used_at FROM org.service_credentials WHERE id=:id"),
                {"id": credential_id},
            ).scalar_one()
        assert last_used_at is not None

        revoked = owner.post(
            f"{root_a}/service-principals/{principal_id}/credentials/{credential_id}/revoke"
        )
        assert revoked.status_code == 204
        assert owner.get(metadata_url).json()[0]["revoked_at"] is not None

        with _service_client(secret) as service:
            after_revoke = service.get(f"{root_a}/projects/{a.project_id}")
            assert after_revoke.status_code == 401

        second = owner.post(
            f"{root_a}/service-principals/{principal_id}/credentials",
            json={"name": "second", "permission_ceiling": ["project.read"]},
        )
        assert second.status_code == 201
        second_secret = second.json()["secret"]

        principal_revoke = owner.post(f"{root_a}/service-principals/{principal_id}/revoke")
        assert principal_revoke.status_code == 204
        with _service_client(second_secret) as service:
            assert service.get(f"{root_a}/projects/{a.project_id}").status_code == 401


def _issue_manager_credential(
    owner: TestClient,
    root: str,
    project_id: UUID,
) -> tuple[str, str, str]:
    created = owner.post(f"{root}/service-principals", json={"name": "race-fixture"})
    assert created.status_code == 201, created.text
    principal_id = created.json()["id"]
    grant = owner.post(
        f"{root}/grants",
        json={
            "subject_kind": "service_principal",
            "subject_id": principal_id,
            "scope_kind": "project",
            "project_id": str(project_id),
            "role": "manager",
        },
    )
    assert grant.status_code == 201, grant.text
    issued = owner.post(
        f"{root}/service-principals/{principal_id}/credentials",
        json={"name": "race-credential"},
    )
    assert issued.status_code == 201, issued.text
    return principal_id, issued.json()["id"], issued.json()["secret"]


@pytest.mark.parametrize("revoke_principal", [False, True])
def test_revoke_between_credential_lookup_and_use_denies_stale_authentication(
    migrated_database: None,
    monkeypatch: pytest.MonkeyPatch,
    revoke_principal: bool,
) -> None:
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"
    looked_up = Event()
    resume = Event()
    original = SqlServicePrincipalRepository.get_credential_for_authentication

    def pause_after_lookup(self, *, organization_id, credential_id):
        resolved = original(self, organization_id=organization_id, credential_id=credential_id)
        if resolved is not None:
            looked_up.set()
            assert resume.wait(timeout=15), "credential lookup was not resumed"
        return resolved

    monkeypatch.setattr(
        SqlServicePrincipalRepository,
        "get_credential_for_authentication",
        pause_after_lookup,
    )
    with _client_for_user(world.owner_user_id) as owner:
        principal_id, credential_id, secret = _issue_manager_credential(
            owner,
            root,
            world.project_id,
        )
        revoke_path = f"{root}/service-principals/{principal_id}"
        if not revoke_principal:
            revoke_path += f"/credentials/{credential_id}"

        def service_edit():
            with _service_client(secret) as service:
                return service.patch(
                    f"{root}/projects/{world.project_id}",
                    json={"name": "must-not-be-written-after-revocation"},
                )

        with ThreadPoolExecutor(max_workers=1) as executor:
            pending = executor.submit(service_edit)
            try:
                assert looked_up.wait(timeout=10), "service did not reach credential lookup"
                # The service has genuinely read an active credential in another
                # transaction. Commit the admin revocation before resuming it.
                revoked = owner.post(f"{revoke_path}/revoke")
                assert revoked.status_code == 204, revoked.text
            finally:
                resume.set()
            result = pending.result(timeout=15)

        assert result.status_code == 401, result.text
        with _service_client(secret) as service:
            assert service.get(f"{root}/projects/{world.project_id}").status_code == 401

        with get_admin_engine().connect() as connection:
            credential = connection.execute(
                text("SELECT revoked_at, last_used_at FROM org.service_credentials WHERE id=:id"),
                {"id": credential_id},
            ).one()
            assert credential.last_used_at is None
            if not revoke_principal:
                assert credential.revoked_at is not None
            else:
                principal_status = connection.execute(
                    text("SELECT status FROM org.service_principals WHERE id=:id"),
                    {"id": principal_id},
                ).scalar_one()
                assert principal_status == "revoked"
            project_name = connection.execute(
                text("SELECT name FROM workspace.projects WHERE id=:id"),
                {"id": world.project_id},
            ).scalar_one()
            assert project_name != "must-not-be-written-after-revocation"


def test_service_credential_use_holds_org_lock_before_project_mutation(
    migrated_database: None,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"
    touched = Event()
    resume = Event()
    original = SqlServicePrincipalRepository.record_credential_use

    def pause_after_touch(self, credential, *, context):
        recorded = original(self, credential, context=context)
        if recorded:
            touched.set()
            assert resume.wait(timeout=15), "service credential use was not resumed"
        return recorded

    monkeypatch.setattr(
        SqlServicePrincipalRepository,
        "record_credential_use",
        pause_after_touch,
    )
    with _client_for_user(world.owner_user_id) as owner:
        principal_id, credential_id, secret = _issue_manager_credential(
            owner,
            root,
            world.project_id,
        )

        def service_edit():
            with _service_client(secret) as service:
                return service.patch(
                    f"{root}/projects/{world.project_id}",
                    json={"name": "service-edited-before-revocation"},
                )

        with ThreadPoolExecutor(max_workers=1) as executor:
            pending = executor.submit(service_edit)
            try:
                assert touched.wait(timeout=10), "service did not reach credential touch"
                # The authorization transaction must lock the organization before
                # touching the credential. Otherwise an admin could take this
                # org lock while waiting for the service's credential lock.
                with get_admin_engine().connect() as connection:
                    with pytest.raises(OperationalError) as caught:
                        connection.execute(
                            text("SELECT id FROM org.organizations WHERE id=:id FOR UPDATE NOWAIT"),
                            {"id": world.organization_id},
                        )
                    assert getattr(caught.value.orig, "sqlstate", None) == "55P03"
            finally:
                resume.set()
            result = pending.result(timeout=15)
        assert result.status_code == 200, result.text

        revoked = owner.post(
            f"{root}/service-principals/{principal_id}/credentials/{credential_id}/revoke"
        )
        assert revoked.status_code == 204, revoked.text
        with _service_client(secret) as service:
            assert service.get(f"{root}/projects/{world.project_id}").status_code == 401


def test_service_principal_management_is_human_admin_only(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"
    with _client_for_user(world.owner_user_id) as owner:
        created = owner.post(f"{root}/service-principals", json={"name": "admin-only"})
        assert created.status_code == 201
        principal_id = created.json()["id"]
        assert any(
            item["id"] == principal_id for item in owner.get(f"{root}/service-principals").json()
        )

        grant = owner.post(
            f"{root}/grants",
            json={
                "subject_kind": "service_principal",
                "subject_id": principal_id,
                "scope_kind": "project",
                "project_id": str(world.project_id),
                "role": "viewer",
            },
        )
        assert grant.status_code == 201
        issued = owner.post(
            f"{root}/service-principals/{principal_id}/credentials",
            json={"name": "reader", "permission_ceiling": ["project.read"]},
        )
        assert issued.status_code == 201

    with _service_client(issued.json()["secret"]) as service:
        denied = service.get(f"{root}/service-principals")
        assert denied.status_code == 401
        assert denied.json()["detail"] == "human session required"
