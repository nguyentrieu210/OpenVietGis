from __future__ import annotations

import os
import socket
import subprocess
import sys
import tempfile
import time
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path

import httpx
from alembic import command
from alembic.config import Config

from tools.migrate_official_modules import migrate_official_modules
from tools.test_database import build_test_environment

ROOT = Path(__file__).resolve().parents[1]


@contextmanager
def native_e08(environment: dict[str, str]) -> Iterator[dict[str, str]]:
    """Run the sole E08 implementation against the same isolated test database."""
    binary = Path(
        environment.get(
            "OPENVIETGIS_RUST_TEST_BINARY", str(ROOT / "target/debug/openvietgis-api-rust"),
        )
    )
    if not binary.is_file():
        # Tests not exercising native E08 can still use the migration/legacy fixture.
        # E08 requests fail closed if the native binary was not built.
        yield {k: v for k, v in environment.items() if k != "OPENVIETGIS_E08_RUST_URL"}
        return
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    origin = f"http://127.0.0.1:{port}"
    native_env = {
        **environment,
        "OPENVIETGIS_BIND_ADDR": f"127.0.0.1:{port}",
        "OPENVIETGIS_DATABASE_URL": environment["OPENVIETGIS_RUNTIME_DATABASE_URL"].replace(
            "postgresql+psycopg://", "postgresql://", 1
        ),
        "OPENVIETGIS_DB_MAX_CONNECTIONS": "4",
    }
    with tempfile.TemporaryDirectory(prefix="ovg-native-e08-") as directory:
        native_env["OPENVIETGIS_ARTIFACT_STORE_DIR"] = directory
        with subprocess.Popen([str(binary.resolve())], cwd=ROOT, env=native_env) as process:
            try:
                deadline = time.monotonic() + 30
                with httpx.Client(trust_env=False, timeout=2.0) as client:
                    while time.monotonic() < deadline and process.poll() is None:
                        try:
                            if client.get(origin + "/api/v1/health/ready").status_code == 200:
                                break
                        except httpx.RequestError:
                            pass
                        time.sleep(0.2)
                    else:
                        raise RuntimeError("Rust E08 test backend failed readiness")
                yield {**environment, "OPENVIETGIS_E08_RUST_URL": origin}
            finally:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


def main() -> int:
    if len(sys.argv) < 2:
        raise RuntimeError("A test command is required")
    environment = build_test_environment(os.environ)
    os.environ.update(environment)
    command.upgrade(Config(str(ROOT / "apps" / "api" / "alembic.ini")), "head")
    migrate_official_modules()
    with native_e08(environment) as test_environment:
        return subprocess.run(
            sys.argv[1:], cwd=ROOT, env=test_environment, check=False
        ).returncode


if __name__ == "__main__":
    raise SystemExit(main())
