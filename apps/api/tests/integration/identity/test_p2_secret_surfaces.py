from __future__ import annotations

import json
from uuid import uuid4

import pytest
from sqlalchemy import text
from tools.check_secret_surfaces import scan_text

from openvietgis_api.config import get_settings
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.identity.test_bootstrap import uninitialized as uninitialized
from tests.integration.rls.test_cross_tenant_api import client_for_user
from tests.integration.service_identity.test_service_authorization import _service_client
from tests.integration.tenancy.test_p1_audit_mutations import _private_runtime_logs


def test_p2_actual_bootstrap_and_service_secrets_stay_out_of_log_and_audit(
    uninitialized: None,
    capsys: pytest.CaptureFixture[str],
) -> None:
    world = seed_access_world()
    authority = get_settings().bootstrap_secret.get_secret_value()
    canaries = [authority]
    responses: list[str] = []
    with _private_runtime_logs() as runtime:
        try:
            with client_for_user(world.owner_user_id) as owner:
                for value in (
                    owner.cookies.get("ovg_session"),
                    owner.headers.get("X-OpenVietGIS-CSRF"),
                ):
                    assert value is not None
                    canaries.append(value)
                bootstrap = owner.post("/api/v1/bootstrap/initialize", json={
                    "organization_name": "Secret scan",
                    "organization_slug": f"scan-{uuid4().hex}",
                }, headers={"Authorization": "Bearer " + authority})
                responses.append(bootstrap.text)
                assert bootstrap.status_code == 201
                org = bootstrap.json()["organization_id"]
                root = f"/api/v1/organizations/{org}"
                project = owner.post(f"{root}/projects", json={
                    "name": "Security test project", "slug": f"scan-project-{uuid4().hex}",
                })
                responses.append(project.text)
                assert project.status_code == 201
                project_id = project.json()["id"]
                principal = owner.post(f"{root}/service-principals", json={
                    "name": f"scan-principal-{uuid4().hex}",
                })
                responses.append(principal.text)
                assert principal.status_code == 201
                principal_id = principal.json()["id"]
                grant = owner.post(f"{root}/grants", json={
                    "subject_kind": "service_principal", "subject_id": principal_id,
                    "scope_kind": "project", "project_id": project_id, "role": "viewer",
                })
                responses.append(grant.text)
                assert grant.status_code == 201
                issued = owner.post(f"{root}/service-principals/{principal_id}/credentials", json={
                    "name": "scan-credential", "permission_ceiling": ["project.read"],
                })
                assert issued.status_code == 201
                # The issuance response alone intentionally contains this one-time credential.
                secret = issued.json()["secret"]
                canaries.append(secret)
                canaries.append(secret.split("_", 1)[1])
                with _service_client(secret) as service:
                    allowed = service.get(f"{root}/projects/{project_id}")
                    responses.append(allowed.text)
                    assert allowed.status_code == 200
                    revoke = owner.post(
                        f"{root}/service-principals/{principal_id}/credentials/"
                        f"{issued.json()['id']}/revoke"
                    )
                    assert revoke.status_code == 204
                    refused = service.get(f"{root}/projects/{project_id}")
                    responses.append(refused.text)
                    assert refused.status_code == 401
                reuse = owner.post("/api/v1/bootstrap/initialize", json={
                    "organization_name": "Reuse", "organization_slug": "reuse",
                }, headers={"Authorization": "Bearer " + authority})
                assert reuse.status_code == 409
                responses.append(reuse.text)
                audit = owner.get(f"{root}/audit-events", params={"limit": 200})
                assert audit.status_code == 200
                responses.append(audit.text)
            with get_admin_engine().connect() as connection:
                persisted = connection.execute(
                    text("SELECT * FROM audit.audit_events WHERE organization_id = :org"),
                    {"org": org},
                ).mappings().all()
            assert len(persisted) >= 5
            responses.append(json.dumps([dict(row) for row in persisted], default=str))
            console = capsys.readouterr()
            findings = scan_text(
                "\n".join([*responses, runtime.getvalue(), console.out, console.err]),
                canaries=canaries,
            )
            assert not findings
        finally:
            capsys.readouterr()
