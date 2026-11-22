"""Optional real-browser acceptance over isolated API and PostGIS."""

from __future__ import annotations

import os
import socket
import subprocess
import threading
import time
from pathlib import Path

import pytest
import uvicorn

from openvietgis_api.config import get_settings
from openvietgis_api.main import create_app
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


@pytest.mark.skipif(
    not os.environ.get("OVG_DEVELOPER_BROWSER_URL"), reason="requires isolated preview"
)
def test_developer_portal_real_browser(migrated_database, monkeypatch):
    dev = seed_access_world()
    reviewer = seed_access_world()
    settings = get_settings()
    monkeypatch.setattr(settings, "marketplace_reviewer_user_ids", [reviewer.owner_user_id])
    monkeypatch.setattr(settings, "marketplace_catalog_path", None)
    monkeypatch.setattr(settings, "marketplace_operator_public_key", None)
    with _client(dev.owner_user_id) as owner, _client(reviewer.owner_user_id) as operator:
        sock = socket.socket()
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
        server = uvicorn.Server(uvicorn.Config(create_app(), log_level="error", access_log=False))
        worker = threading.Thread(target=server.run, kwargs={"sockets": [sock]}, daemon=True)
        worker.start()
        try:
            for _ in range(100):
                if server.started:
                    break
                if not worker.is_alive():
                    raise RuntimeError("isolated API failed")
                time.sleep(0.1)
            assert server.started
            env = {
                **os.environ,
                "OVG_DEVELOPER_TEST_API": f"http://127.0.0.1:{port}",
                "OVG_DEVELOPER_TEST_SESSION": owner.cookies.get("ovg_session"),
                "OVG_DEVELOPER_REVIEW_SESSION": operator.cookies.get("ovg_session"),
            }
            root = Path(__file__).resolve().parents[5]
            subprocess.run(
                ["node", str(root / "apps/web-next/developer-portal-smoke.mjs")],
                env=env,
                check=True,
                timeout=100,
            )
            state = owner.get("/api/v1/developer/marketplace/me").json()
            assert state["publisher"]["name"] == "Browser reference publisher"
            assert state["submissions"][0]["status"] == "approved"
            assert not state["submissions"][0]["published"]
        finally:
            server.should_exit = True
            worker.join(10)
