from __future__ import annotations

import os
import socket
import subprocess
import sys
import time
from pathlib import Path
from uuid import uuid4

import httpx

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user

ROOT = Path(__file__).resolve().parents[4]


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def start_api(port: int) -> subprocess.Popen[bytes]:
    process = subprocess.Popen(
        [
            sys.executable,
            "-m",
            "uvicorn",
            "openvietgis_api.main:app",
            "--host",
            "127.0.0.1",
            "--port",
            str(port),
            "--log-level",
            "warning",
        ],
        cwd=ROOT,
        env=os.environ.copy(),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )

    deadline = time.monotonic() + 10
    url = f"http://127.0.0.1:{port}/api/v1/health/ready"
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("API process exited before becoming ready")
        try:
            response = httpx.get(url, timeout=0.5)
            if response.status_code == 200:
                return process
        except httpx.HTTPError:
            pass
        time.sleep(0.1)

    stop_api(process)
    raise RuntimeError("API process did not become ready within 10 seconds")


def stop_api(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def test_tenant_record_survives_ten_real_api_process_restarts(migrated_database: None) -> None:
    world = seed_access_world()
    with client_for_user(world.owner_user_id) as authenticated:
        cookie = authenticated.cookies.get("ovg_session")
        csrf = authenticated.headers["X-OpenVietGIS-CSRF"]
    assert cookie is not None
    port = free_port()
    base_url = f"http://127.0.0.1:{port}/api/v1"
    path = f"{base_url}/organizations/{world.organization_id}/projects"
    process = start_api(port)

    try:
        with httpx.Client(
            cookies={"ovg_session": cookie}, headers={"X-OpenVietGIS-CSRF": csrf}, timeout=2
        ) as client:
            for index in range(10):
                create = client.post(
                    path,
                    json={
                        "name": f"process-restart-{index}",
                        "slug": f"restart-{uuid4().hex}",
                    },
                )
                assert create.status_code == 201
                created = create.json()

                stop_api(process)
                process = start_api(port)

                loaded = client.get(f"{path}/{created['id']}")
                assert loaded.status_code == 200
                assert loaded.json() == created
    finally:
        stop_api(process)
