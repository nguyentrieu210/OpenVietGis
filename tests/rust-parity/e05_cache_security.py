"""Exercise warm-cache authorization and stale pins against an isolated candidate.

Uses the existing guarded disposable database harness. Never accepts a live
database name, prints a cookie, or builds the candidate.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import tempfile
from pathlib import Path

import httpx
import psycopg
from e04_benchmark import server
from openvietgis_api.security.sessions import parse_session_token
from run import SEED, baseline, configure_python, database, verify_database_role


def exercise(url, owner_url, fixture):
    layer = fixture["layers"]["epsg4326_1k"]
    path = (f"/api/v1/organizations/{fixture['org_a_id']}/projects/{fixture['project_a_id']}"
            f"/layers/{layer['layer_id']}/tiles/0/0/0.mvt")
    pins = {key: layer[key] for key in ("content_revision", "schema_revision")}
    owner = {"cookie": f"ovg_session={fixture['session_a_token']}"}
    results = {}
    with httpx.Client(base_url=url, trust_env=False, timeout=30) as client:
        cold = client.get(path, params=pins, headers=owner)
        warm = client.get(path, params=pins, headers=owner)
        results["warm_identity"] = (
            cold.status_code == warm.status_code == 200
            and cold.content == warm.content
            and warm.headers.get("cache-control") == "private, no-store"
        )
        metrics = client.get("/api/v1/ops/metrics").text
        hits = next((int(line.split()[1]) for line in metrics.splitlines()
                     if line.startswith("ovg_tile_cache_hits_total ")), 0)
        results["cache_hit_observed"] = hits >= 1
        results["anonymous_denied"] = client.get(path, params=pins).status_code == 401
        results["foreign_tenant_denied"] = client.get(
            path, params=pins, headers={"cookie": f"ovg_session={fixture['session_b_token']}"},
        ).status_code in (403, 404)
        results["invalid_bearer_precedes_warm_cookie"] = client.get(
            path, params=pins, headers={**owner, "authorization": "Bearer invalid"},
        ).status_code == 401

        # Mutate only this disposable harness DB, then restore its source pin.
        # A cache hit must never turn an otherwise STALE_READ_PIN into HTTP 200.
        with psycopg.connect(owner_url) as conn:
            conn.execute("UPDATE spatial.sources SET content_revision=%s WHERE id=%s",
                         ("f" * 64, layer["source_id"]))
        try:
            response = client.get(path, params=pins, headers=owner)
            results["warm_old_revision_rejected"] = (
                response.status_code == 409 and response.json().get("code") == "STALE_READ_PIN"
            )
        finally:
            with psycopg.connect(owner_url) as conn:
                conn.execute("UPDATE spatial.sources SET content_revision=%s WHERE id=%s",
                             (layer["content_revision"], layer["source_id"]))
        restored = client.get(path, params=pins, headers=owner)
        results["restored_authorized_pin"] = restored.status_code == 200

        # Retirement and explicit revocation are distinct from revision and
        # expiry; neither may be hidden by the warmed cache entry.
        with psycopg.connect(owner_url) as conn:
            conn.execute("UPDATE spatial.layers SET lifecycle='retired' WHERE id=%s",
                         (layer["layer_id"],))
        try:
            results["retired_layer_denied_on_warm_entry"] = client.get(
                path, params=pins, headers=owner,
            ).status_code == 404
        finally:
            with psycopg.connect(owner_url) as conn:
                conn.execute("UPDATE spatial.layers SET lifecycle='active' WHERE id=%s",
                             (layer["layer_id"],))
        results["reactivated_layer_authorized"] = client.get(
            path, params=pins, headers=owner,
        ).status_code == 200

        session_id = parse_session_token(fixture["session_a_token"]).session_id
        with psycopg.connect(owner_url) as conn:
            conn.execute("UPDATE identity.web_sessions SET revoked_at=clock_timestamp() WHERE id=%s",
                         (session_id,))
        try:
            results["revoked_session_denied_on_warm_entry"] = client.get(
                path, params=pins, headers=owner,
            ).status_code == 401
        finally:
            with psycopg.connect(owner_url) as conn:
                conn.execute("UPDATE identity.web_sessions SET revoked_at=NULL WHERE id=%s",
                             (session_id,))
        with psycopg.connect(owner_url) as conn:
            conn.execute("UPDATE identity.web_sessions SET expires_at=now()-interval '1 second' WHERE id=%s",
                         (session_id,))
        results["expired_session_denied_on_warm_entry"] = client.get(
            path, params=pins, headers=owner,
        ).status_code == 401
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error("Provide the final prebuilt candidate")
    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with tempfile.TemporaryDirectory(prefix="ovg-e05-cache-") as directory:
        work = Path(directory)
        with database(admin) as (owner_url, runtime_url):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            configure_python(owner_url, runtime_url, artifacts)
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            fixture = baseline.seed_benchmark_database(
                owner_url, seed=SEED, scales=["1k"], csrf_secret=baseline.DEFAULT_CSRF_SECRET,
            )
            verify_database_role(runtime_url)
            with server(binary, runtime_url, work, "rust") as (url, _):
                results = exercise(url, owner_url, fixture)
    report = {
        "schema": 1, "seed": SEED, "candidate_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "scope": "guarded disposable synthetic database and loopback candidate",
        "checks": results, "passed": all(results.values()),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return int(not report["passed"])


if __name__ == "__main__":
    raise SystemExit(main())
