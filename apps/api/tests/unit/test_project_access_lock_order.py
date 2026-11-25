from unittest.mock import Mock
from uuid import uuid4

import pytest
from fastapi import Depends, FastAPI
from fastapi.testclient import TestClient

from openvietgis_api.adapters.http import gis_access_guard
from openvietgis_api.adapters.http.security import AuthenticatedRequest
from openvietgis_api.domains.authorization.context import Actor


@pytest.mark.parametrize(
    ("suffix", "lock"),
    [
        ("/field/packages", "FOR UPDATE"),
        ("/field/packages/{package_id}/download", "FOR UPDATE"),
        ("/field/packages/{package_id}/snapshot", "FOR UPDATE"),
        ("/layers", "FOR SHARE"),
    ],
)
def test_project_guard_acquires_required_lock_on_first_read(
    monkeypatch: pytest.MonkeyPatch, suffix: str, lock: str
) -> None:
    session = Mock()
    auth = AuthenticatedRequest(Actor("human", uuid4(), "Test editor"), None, None)
    monkeypatch.setattr(gis_access_guard, "require_member_context", Mock())
    monkeypatch.setattr(gis_access_guard, "scoped", Mock(return_value=False))

    app = FastAPI()
    app.dependency_overrides[gis_access_guard.get_db_session] = lambda: session
    app.dependency_overrides[gis_access_guard.authenticate_request] = lambda: auth
    app.add_api_route(
        "/organizations/{organization_id}/projects/{project_id}" + suffix,
        lambda: {},
        dependencies=[Depends(gis_access_guard.project_access_guard)],
    )

    path = f"/organizations/{uuid4()}/projects/{uuid4()}"
    with TestClient(app) as client:
        assert client.get(path + suffix.format(package_id=uuid4())).status_code == 200

    queries = [str(call.args[0]) for call in session.execute.call_args_list]
    assert len(queries) == 2
    assert "FROM org.organizations" in queries[0]
    assert "FROM workspace.projects" in queries[1]
    assert all(query.endswith(lock) for query in queries)
