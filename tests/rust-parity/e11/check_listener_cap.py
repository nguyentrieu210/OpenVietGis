"""E11 SSE listener cap: 40 dedicated LISTEN connections per process and stream family;
the next stream gets `event: unavailable` and closes (FastAPI oracle and Rust)."""

import argparse
import os
import sys
import time
from pathlib import Path
from tempfile import TemporaryDirectory

import psycopg

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))
import run as parity
from check_realtime import READY, Frames, fastapi_server
from check_team import editor_member
from run import (
    baseline,
    configure_python,
    database,
    project_path,
    require,
    rust_client,
    session_headers,
)

CAP = 40
UNAVAILABLE = "event: unavailable\ndata: {}\n\n"
RETRY = "retry: 3000\n\n"


def settle(owner, limit=25, timeout=90):
    """Wait until earlier streams have closed their database connections."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        with psycopg.connect(owner) as conn:
            count = conn.execute(
                "SELECT count(*) FROM pg_stat_activity WHERE datname=current_database()"
            ).fetchone()[0]
        if count <= limit:
            return
        time.sleep(1)
    print(f"WARN {count} database connections still open", flush=True)


@parity.case
def managed_cap(label, client, path, headers):
    """Rust-only: managed streams hold a LISTEN connection each, so they are capped too."""
    streams = [Frames(client, path, headers) for _ in range(CAP)]
    try:
        for index, stream in enumerate(streams):
            frame = stream.next(20)
            require(frame == RETRY, f"managed stream {index} got {frame!r}")
        time.sleep(3)  # `retry` precedes LISTEN: let every stream take its slot.
        extra = Frames(client, path, headers)
        try:
            frames = [extra.next(20) for _ in range(3)]
            require(frames == [RETRY, UNAVAILABLE, "<closed>"],
                    f"managed stream {CAP + 1} got {frames!r}")
        finally:
            extra.close()
    finally:
        for stream in streams:
            stream.close()
    print(f"PASS {label}", flush=True)


@parity.case
def listener_cap(label, client, path, headers):
    streams = [Frames(client, path, headers) for _ in range(CAP)]
    try:
        for index, stream in enumerate(streams):
            frame = stream.next(20)
            require(frame == READY, f"stream {index} got {frame!r}")
        extra = Frames(client, path, headers)
        try:
            frame = extra.next(20)
            require(frame == UNAVAILABLE, f"stream {CAP + 1} got {frame!r}")
            require(extra.next(10) == "<closed>", f"stream {CAP + 1} stayed open")
        finally:
            extra.close()
    finally:
        for stream in streams:
            stream.close()
    print(f"PASS {label}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    binary = parser.parse_args().candidate.resolve()
    if not binary.is_file():
        parser.error("build the Rust API first")
    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with (
        TemporaryDirectory(prefix="ovg-e11-cap-") as directory,
        database(admin) as (owner, runtime),
    ):
        work = Path(directory)
        configure_python(owner, runtime, work)
        baseline.run_alembic_upgrade(owner, runtime)
        fixture = baseline.seed_benchmark_database(
            owner, seed=20261126, scales=["1k"], csrf_secret=baseline.DEFAULT_CSRF_SECRET
        )
        csrf = session_headers(
            fixture["session_a_token"], **{"x-openvietgis-csrf": fixture["csrf_a_token"]}
        )
        project = project_path(fixture["org_a_id"], fixture["project_a_id"])
        paths = {
            "project": f"{project}/collaboration/events",
            "organization chat": f"/api/v1/organizations/{fixture['org_a_id']}/chat/events",
        }
        with fastapi_server(work) as oracle, rust_client(binary, runtime, work) as rust:
            _, editor, editor_csrf = editor_member(owner, oracle, fixture, csrf)
            # FastAPI frees a slot only at its next 15 s wake-up: wait for each family
            # to release its connections so the test never hits max_connections.
            for name, client in (("Rust", rust), ("FastAPI", oracle)):
                for family, path in paths.items():
                    settle(owner)
                    listener_cap(f"E11 {name} {family} stream 41 is unavailable", client, path,
                                 editor)
            settle(owner)
            created = rust.post(f"{project}/integration-hub/managed/sessions", headers=editor_csrf,
                                json={})
            require(created.status_code == 201, f"managed session: {created.text}")
            managed = f"{project}/integration-hub/managed/events?session_id={created.json()['id']}"
            managed_cap("E11 Rust managed stream 41 is unavailable", rust, managed, editor)
    print("E11 listener cap parity finished", flush=True)


if __name__ == "__main__":
    main()
