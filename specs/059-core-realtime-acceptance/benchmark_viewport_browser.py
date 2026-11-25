"""Serve the built workspace and real API on loopback for a synthetic benchmark."""

from __future__ import annotations

import json
import os
import socket
import subprocess
import threading
from pathlib import Path

from benchmark_spatial_http import SIZES, seed_layer
from openvietgis_api.config import get_settings
from openvietgis_api.main import create_app
from starlette.exceptions import HTTPException
from starlette.responses import Response
from starlette.staticfiles import StaticFiles
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client
from uvicorn import Config, Server

from tools.test_database import assert_safe_test_target

ROOT = Path(__file__).resolve().parents[2]


def main() -> None:
    settings = get_settings()
    if settings.environment != "test":
        raise RuntimeError("Run only on a fresh synthetic PostGIS test database.")
    assert_safe_test_target(settings.migration_db_url)
    assert_safe_test_target(settings.runtime_db_url)
    static = StaticFiles(directory=ROOT / "apps/web-next/dist", html=True)
    api = create_app()

    async def app(scope, receive, send):
        if scope["type"] != "http" or scope["path"].startswith("/api/v1/"):
            await api(scope, receive, send)
            return
        path = scope["path"].removeprefix("/rewrite/")
        if path in {"", "/", "/app", "/app/"}:
            path = "/index.html"
        try:
            await static(scope | {"path": path, "root_path": ""}, receive, send)
        except HTTPException as exc:
            await Response(status_code=exc.status_code)(scope, receive, send)

    with socket.socket() as http_socket:
        http_socket.bind(("127.0.0.1", 0))
        http_socket.listen()
        origin = f"http://127.0.0.1:{http_socket.getsockname()[1]}"
        server = Server(Config(app, log_level="warning", access_log=False))
        thread = threading.Thread(target=server.run, kwargs={"sockets": [http_socket]})
        thread.start()
        try:
            sizes = tuple(
                int(value) for value in os.getenv(
                    "OVG059_SCALE_SIZES", ",".join(str(size) for size in SIZES)
                ).split(",")
            )
            if not sizes or not set(sizes) <= set(SIZES):
                raise RuntimeError(f"Allowed dataset sizes: {SIZES}")
            for rows in sizes:
                world = seed_access_world()
                root = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
                with _client(world.owner_user_id) as client:
                    layer_id, _, _ = seed_layer(client, world, root, rows)
                    account = client.get("/api/v1/me").json()["id"]
                    settings_json = json.dumps(
                        {
                            "rows": rows,
                            "origin": origin,
                            "layerId": layer_id,
                            "token": client.cookies["ovg_session"],
                            "cameraKey": (
                                f"ovg-workspace:v1:{account}:"
                                f"{world.organization_id}:{world.project_id}:camera"
                            ),
                        }
                    )
                    subprocess.run(
                        ["node", str(Path(__file__).with_name("benchmark_viewport_browser.mjs"))],
                        input=settings_json,
                        text=True,
                        check=True,
                        cwd=ROOT,
                    )
        finally:
            server.should_exit = True
            thread.join()


if __name__ == "__main__":
    main()
