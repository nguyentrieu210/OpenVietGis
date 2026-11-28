"""HTTP denial cache policy for retained and native API parity; no database."""

import pytest
from fastapi import HTTPException
from fastapi.responses import Response
from fastapi.testclient import TestClient

from openvietgis_api.main import create_app


@pytest.fixture
def client():
    app = create_app()

    @app.get("/cache-policy-test/{status_code}")
    def response(status_code: int, policy: str | None = None) -> Response:
        headers = {"Cache-Control": policy} if policy else {}
        return Response("synthetic", status_code=status_code, headers=headers)

    @app.get("/cache-policy-exception/{status_code}")
    def denied(status_code: int) -> None:
        raise HTTPException(status_code, "synthetic denial")

    with TestClient(app) as test_client:
        yield test_client


@pytest.mark.parametrize("status", [401, 403, 404])
@pytest.mark.parametrize("route", ["cache-policy-test", "cache-policy-exception"])
def test_denials_are_private_and_never_stored(client, status, route):
    response = client.get(f"/{route}/{status}")
    assert response.status_code == status
    assert response.headers["cache-control"] == "private, no-store"


def test_unknown_route_is_not_negative_cached(client):
    response = client.get("/synthetic-unknown-route")
    assert response.status_code == 404
    assert response.headers["cache-control"] == "private, no-store"


@pytest.mark.parametrize("status", [200, 401, 403, 404])
def test_explicit_route_policy_is_preserved(client, status):
    response = client.get(f"/cache-policy-test/{status}", params={"policy": "no-store"})
    assert response.headers["cache-control"] == "no-store"


@pytest.mark.parametrize("status", [200, 422, 503])
def test_other_responses_keep_their_existing_policy(client, status):
    response = client.get(f"/cache-policy-test/{status}")
    assert response.status_code == status
    assert "cache-control" not in response.headers
