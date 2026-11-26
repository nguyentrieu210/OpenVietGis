"""E11 SSE listener cap: 40 dedicated LISTEN connections per process and stream family;
the next stream gets `event: unavailable` and closes (FastAPI oracle and Rust)."""

import argparse
import os
import sys
import time
from pathlib import Path
from tempfile import TemporaryDirectory

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
            _, editor, _ = editor_member(owner, oracle, fixture, csrf)
            # Rust releases a slot on disconnect; FastAPI only at its next 15 s wake-up,
            # so Rust goes first and FastAPI's families are spaced to free connections.
            for family, path in paths.items():
                listener_cap(f"E11 Rust {family} stream 41 is unavailable", rust, path, editor)
            for index, (family, path) in enumerate(paths.items()):
                if index:
                    time.sleep(20)
                listener_cap(f"E11 FastAPI {family} stream 41 is unavailable", oracle, path,
                             editor)
    print("E11 listener cap parity finished", flush=True)


if __name__ == "__main__":
    main()
