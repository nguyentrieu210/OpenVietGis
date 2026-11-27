"""E11 candidate SQL/Rust integration against a disposable migrated database."""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from tempfile import TemporaryDirectory
from uuid import uuid4

import psycopg

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tests/rust-parity"))
from run import (
    baseline,
    configure_python,
    database,
    project_path,
    rust_client,
    scoped_member,
    session_headers,
    session_id,
    setup_gis_scope,
)


def check_storage(owner, runtime, f):
    a = f["org_a_id"], f["project_a_id"]
    b = f["org_b_id"], f["project_b_id"]
    with psycopg.connect(owner, autocommit=True) as conn:
        conn.execute((ROOT / "infra/rust-platform/e11/outbox.sql").read_text())
        assert conn.execute("SELECT ovg_e11.append_spatial(%s, %s)", b).fetchone() == (
            1,
        )
    with psycopg.connect(owner) as conn:
        conn.execute("SELECT ovg_e11.append_spatial(%s, %s)", a)
        with psycopg.connect(owner) as observer:
            assert observer.execute(
                "SELECT count(*) FROM ovg_e11.events WHERE organization_id=%s", (a[0],)
            ).fetchone() == (0,)
        conn.rollback()
    with psycopg.connect(owner) as conn:
        assert conn.execute("SELECT ovg_e11.append_spatial(%s, %s)", a).fetchone() == (
            1,
        )
        conn.execute(
            "SELECT ovg_e11.append_spatial(%s, %s) FROM generate_series(1,4096)", a
        )
        assert conn.execute(
            "SELECT count(*),min(sequence),max(sequence) FROM ovg_e11.events "
            "WHERE organization_id=%s",
            (a[0],),
        ).fetchone() == (4096, 2, 4097)
    with psycopg.connect(runtime) as conn:
        assert conn.execute("SELECT count(*) FROM ovg_e11.events").fetchone() == (0,)
        for name, value in zip(
            ("organization_id", "project_id", "actor_user_id"), (*a, f["user_a_id"])
        ):
            conn.execute(
                "SELECT set_config(%s,%s,true)", ("openvietgis." + name, value)
            )
        assert conn.execute("SELECT count(*) FROM ovg_e11.events").fetchone() == (4096,)
        assert conn.execute(
            "SELECT count(*) FROM ovg_e11.events WHERE organization_id=%s", (b[0],)
        ).fetchone() == (0,)
        assert conn.execute(
            "SELECT has_function_privilege(current_user,"
            "'ovg_e11.append_spatial(uuid,uuid)','EXECUTE')"
        ).fetchone() == (False,)
        assert conn.execute(
            "SELECT has_table_privilege(current_user,'ovg_e11.events','INSERT')"
        ).fetchone() == (False,)
    print(
        "PASS E11 rollback visibility, retention, RLS and runtime write denial",
        flush=True,
    )
    env = dict(
        os.environ,
        E11_RUNTIME_URL=runtime,
        E11_ORG=a[0],
        E11_PROJECT=a[1],
        E11_USER=f["user_a_id"],
        E11_SESSION=str(baseline.det_uuid(f["seed"], "session/alpha-owner")),
        E11_OTHER_ORG=b[0],
        E11_OTHER_PROJECT=b[1],
    )
    subprocess.run(
        [
            "cargo",
            "test",
            "--locked",
            "-p",
            "ovg-storage",
            "--test",
            "e11_spatial_replay",
            "--",
            "--ignored",
        ],
        cwd=ROOT,
        env=env,
        check=True,
    )


def replay_path(fixture, suffix):
    project = project_path(fixture[f"org_{suffix}_id"], fixture[f"project_{suffix}_id"])
    return f"{project}/collaboration/spatial-invalidations"


def check_http(client, owner, runtime, fixture):
    from fastapi.testclient import TestClient
    from openvietgis_api.main import create_app

    path = replay_path(fixture, "a")
    foreign = replay_path(fixture, "b")
    headers = session_headers(
        fixture["session_a_token"],
        **{"x-openvietgis-csrf": fixture["csrf_a_token"]},
    )
    foreign_headers = session_headers(fixture["session_b_token"])

    def get(status=200, *, target=path, auth=headers, **params):
        response = client.get(target, headers=auth, params=params)
        assert response.status_code == status, (response.status_code, response.text)
        assert response.headers["cache-control"] == "no-store"
        return response.json()

    get(401, auth={})
    get(401, auth=headers | {"authorization": "Bearer unavailable"})
    get(404, target=foreign)
    get(404, auth=foreign_headers)
    beta = get(target=foreign, auth=foreign_headers)
    assert beta["events"] == [{"sequence": 1, "kind": "spatial_change"}]

    expired = get()
    assert expired["mode"] == "snapshot_required"
    cursor = expired["latest_cursor"]
    assert cursor["sequence"] == 4097
    assert get(epoch=cursor["epoch"], after=4095, limit=1) == {
        "mode": "events",
        "events": [{"sequence": 4096, "kind": "spatial_change"}],
        "next_cursor": {"epoch": cursor["epoch"], "sequence": 4096},
        "has_more": True,
    }
    assert get(epoch=str(uuid4()), after=4097) == expired
    assert get(epoch=cursor["epoch"], after=4098) == expired
    assert get(422, after=3)["code"] == "INVALID_CURSOR"
    assert get(422, epoch=cursor["epoch"], after=-1)["code"] == "INVALID_CURSOR"
    get(400, limit=501)

    scope_user, scope_membership, scope_token = scoped_member(owner, fixture)
    base = project_path(fixture["org_a_id"], fixture["project_a_id"])
    with TestClient(create_app()) as oracle:
        setup_gis_scope(
            oracle,
            fixture,
            base,
            fixture["layers"]["epsg4326_1k"]["layer_id"],
            scope_membership,
            scope_user,
        )
    get(403, auth=session_headers(scope_token))

    source = fixture["layers"]["edit_4326"]["source_id"]
    with psycopg.connect(owner) as conn:
        revision = conn.execute(
            "SELECT content_revision FROM spatial.data_sources WHERE id=%s", (source,)
        ).fetchone()[0]

    write_path = f"{base}/sources/{source}/changesets"
    commit = {
        "operation_id": "e11-http-commit",
        "base_content_revision": revision,
        "summary": "E11 transactional spatial invalidation",
        "operations": [
            {
                "op_type": "create",
                "feature_id": "e11-http-probe",
                "geometry": {"type": "Point", "coordinates": [105.84, 21.02]},
                "properties": {
                    "code": "E11",
                    "ward": "W1",
                    "metric_val": 7,
                    "active": True,
                },
            }
        ],
    }

    with psycopg.connect(runtime, autocommit=True) as listener:
        listener.execute("LISTEN ovg_team_collaboration")
        result = client.post(write_path, headers=headers, json=commit)
        assert result.status_code == 201, result.text
        receipt = result.json()
        notice = next(listener.notifies(timeout=3, stop_after=1), None)
        assert notice is not None
        assert json.loads(notice.payload) == {
            "o": fixture["org_a_id"],
            "p": fixture["project_a_id"],
            "a": None,
            "b": None,
            "kind": "spatial_change",
        }

    echoed = client.post(write_path, headers=headers, json=commit)
    assert echoed.status_code == 201 and echoed.json() == receipt
    stale = client.post(
        write_path, headers=headers, json=commit | {"operation_id": "e11-stale"}
    )
    assert stale.status_code == 409, stale.text
    fresh = get(epoch=cursor["epoch"], after=cursor["sequence"])
    assert fresh["events"] == [{"sequence": 4098, "kind": "spatial_change"}]
    assert fresh["next_cursor"]["sequence"] == 4098

    reverted = client.post(
        f"{base}/changesets/{receipt['id']}/revert",
        headers=headers,
        json={"operation_id": "e11-http-revert"},
    )
    assert reverted.status_code == 201, reverted.text
    assert get(epoch=cursor["epoch"], after=4098)["events"] == [
        {"sequence": 4099, "kind": "spatial_change"},
    ]
    with psycopg.connect(owner) as conn:
        assert conn.execute(
            "SELECT kind FROM integration_hub.project_events WHERE changeset_id=%s",
            (receipt["id"],),
        ).fetchone() == ("changeset",)
        conn.execute(
            "UPDATE identity.web_sessions SET revoked_at=now() WHERE id=%s",
            (session_id(fixture["session_b_token"]),),
        )
    get(401, target=foreign, auth=foreign_headers)
    print(
        "PASS E11 staged HTTP authorization, cursor/paging and E03 durability",
        flush=True,
    )

    with psycopg.connect(owner, autocommit=True) as conn:
        conn.execute((ROOT / "infra/rust-platform/e11/rollback.sql").read_text())
        assert conn.execute("SELECT to_regnamespace('ovg_e11')").fetchone() == (None,)
    assert get(503)["detail"] == "spatial invalidations unavailable"
    print("PASS E11 candidate schema rollback (HTTP fails closed)", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    binary = parser.parse_args().candidate.resolve()
    if not binary.is_file():
        parser.error("build the Rust API with --features e11-candidate first")
    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with (
        TemporaryDirectory(prefix="ovg-e11-") as work,
        database(admin) as (owner, runtime),
    ):
        configure_python(owner, runtime, Path(work))
        baseline.run_alembic_upgrade(owner, runtime)
        fixture = baseline.seed_benchmark_database(
            owner,
            seed=20261126,
            scales=["1k"],
            csrf_secret=baseline.DEFAULT_CSRF_SECRET,
        )
        check_storage(owner, runtime, fixture)
        with rust_client(binary, runtime, Path(work)) as client:
            check_http(client, owner, runtime, fixture)


if __name__ == "__main__":
    main()
