from __future__ import annotations

from fastapi import FastAPI, HTTPException
from fastapi.responses import Response
from fastapi.testclient import TestClient

from openvietgis_api.adapters.http.errors import install_error_handlers


def _error_app() -> FastAPI:
    app = FastAPI()
    install_error_handlers(app)

    for status_code in (401, 403, 404):
        async def protected_error(code: int = status_code) -> None:
            raise HTTPException(status_code=code, detail="protected error")

        app.add_api_route(f"/error/{status_code}", protected_error)

    def route_specific_error() -> Response:
        return Response(status_code=404, headers={"Cache-Control": "no-store"})

    app.add_api_route("/route-specific-error", route_specific_error)

    def success() -> dict[str, str]:
        return {"ok": "yes"}

    app.add_api_route("/success", success)
    return app


def test_protected_error_cache_policy_and_route_override() -> None:
    with TestClient(_error_app()) as client:
        for status_code in (401, 403, 404):
            response = client.get(f"/error/{status_code}")
            assert response.status_code == status_code
            assert response.headers["cache-control"] == "private, no-store"

        route_specific = client.get("/route-specific-error")
        assert route_specific.status_code == 404
        assert route_specific.headers["cache-control"] == "no-store"

        success = client.get("/success")
        assert success.status_code == 200
        assert "cache-control" not in success.headers
