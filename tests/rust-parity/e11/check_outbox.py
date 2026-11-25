"""E11 candidate SQL/Rust integration against a disposable migrated database."""
import os
import subprocess
import sys
from pathlib import Path
from tempfile import TemporaryDirectory

import psycopg

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tests/rust-parity"))
from run import baseline, configure_python, database  # noqa: E402


def check(owner, runtime, f):
    a = f["org_a_id"], f["project_a_id"]
    b = f["org_b_id"], f["project_b_id"]
    with psycopg.connect(owner, autocommit=True) as conn:
        conn.execute((ROOT / "infra/rust-platform/e11/outbox.sql").read_text())
        assert conn.execute(
            "SELECT ovg_e11.append_spatial(%s, %s)", b
        ).fetchone() == (1,)
    with psycopg.connect(owner) as conn:
        conn.execute("SELECT ovg_e11.append_spatial(%s, %s)", a)
        with psycopg.connect(owner) as observer:
            assert observer.execute(
                "SELECT count(*) FROM ovg_e11.events WHERE organization_id=%s", (a[0],)
            ).fetchone() == (0,)
        conn.rollback()
    with psycopg.connect(owner) as conn:
        assert conn.execute(
            "SELECT ovg_e11.append_spatial(%s, %s)", a
        ).fetchone() == (1,)
        conn.execute(
            "SELECT ovg_e11.append_spatial(%s, %s) FROM generate_series(1,4096)", a
        )
        assert conn.execute(
            "SELECT count(*),min(sequence),max(sequence) FROM ovg_e11.events "
            "WHERE organization_id=%s", (a[0],)
        ).fetchone() == (4096, 2, 4097)
    with psycopg.connect(runtime) as conn:
        assert conn.execute("SELECT count(*) FROM ovg_e11.events").fetchone() == (0,)
        for name, value in zip(
            ("organization_id", "project_id", "actor_user_id"), (*a, f["user_a_id"])
        ):
            conn.execute("SELECT set_config(%s,%s,true)", ("openvietgis." + name, value))
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
    print("PASS E11 rollback visibility, retention, RLS and runtime write denial", flush=True)
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
        ["cargo", "test", "--locked", "-p", "ovg-storage", "--test",
         "e11_spatial_replay", "--", "--ignored"],
        cwd=ROOT, env=env, check=True,
    )
    with psycopg.connect(owner, autocommit=True) as conn:
        conn.execute((ROOT / "infra/rust-platform/e11/rollback.sql").read_text())
        assert conn.execute("SELECT to_regnamespace('ovg_e11')").fetchone() == (None,)
    print("PASS E11 candidate schema rollback", flush=True)


def main():
    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with TemporaryDirectory(prefix="ovg-e11-") as work, database(admin) as (owner, runtime):
        configure_python(owner, runtime, Path(work))
        baseline.run_alembic_upgrade(owner, runtime)
        fixture = baseline.seed_benchmark_database(
            owner, seed=20261126, scales=["1k"],
            csrf_secret=baseline.DEFAULT_CSRF_SECRET,
        )
        check(owner, runtime, fixture)


if __name__ == "__main__":
    main()
