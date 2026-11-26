#!/usr/bin/env python3
"""E18 ops differential parity: health, readiness, metrics, release inventory, entitlements."""

from __future__ import annotations

import argparse
import os
import sys
import tempfile
import time
from datetime import datetime
from pathlib import Path

import psycopg
from psycopg import sql

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "rust-parity"))
sys.path.insert(0, str(ROOT / "tools" / "benchmark" / "rust-platform"))

import benchmark_runner as baseline
import run as parity

API = "/api/v1"
# shortcut: identity fallback until PR #64 (parity.case) is in the base; then use @parity.case.
case = getattr(parity, "case", lambda compare: compare)


def require(condition, detail):
    if not condition:
        raise AssertionError(detail)


@case
def compare(label, expected, actual, status, normalize=None):
    require(
        expected.status_code == status,
        f"FastAPI status {expected.status_code}, wanted {status}",
    )
    require(
        actual.status_code == status,
        f"Rust status {actual.status_code}, wanted {status}: {actual.text[:300]}",
    )
    norm = normalize or (lambda body: body)
    mismatch = parity.difference(norm(expected.json()), norm(actual.json()))
    require(mismatch is None, mismatch)
    require(
        actual.headers.get("cache-control") == expected.headers.get("cache-control"),
        "Cache-Control differs from FastAPI",
    )
    print(f"PASS {label}", flush=True)
    return actual.json()


@case
def compare_metrics(label, expected, actual):
    require(expected.status_code == actual.status_code == 200, "metrics status differs")
    require(
        actual.headers["content-type"] == expected.headers["content-type"],
        "metrics Content-Type differs from FastAPI",
    )
    # Rust appends its own process counters after the FastAPI series.
    require(actual.text.startswith(expected.text), "FastAPI metric series differ")
    require("openvietgis_database_ready 1\n" in actual.text, "database not reported ready")
    print(f"PASS {label}", flush=True)


@case
def check(label, condition, detail):
    require(condition, detail)
    print(f"PASS {label}", flush=True)


def stamped(body):
    """Each runtime writes its own `updated_at`; the GET afterwards compares it exactly."""
    require(
        datetime.fromisoformat(body["updated_at"]).utcoffset() is not None,
        "updated_at must be an ISO 8601 timestamp with offset",
    )
    return {key: value for key, value in body.items() if key != "updated_at"}


def run_cases(oracle, rust, owner_url, fixture, member):
    def both(method, path, *, token=None, csrf=None, body=None, raw=None, content_type=None):
        headers = parity.session_headers(token)
        if csrf is not None:
            headers["x-openvietgis-csrf"] = csrf
        if content_type is not None:
            headers["content-type"] = content_type
        return tuple(
            client.request(method, path, headers=headers, json=body, content=raw)
            for client in (oracle, rust)
        )

    owner, csrf = fixture["session_a_token"], fixture["csrf_a_token"]
    from openvietgis_api.security.csrf import make_csrf_token

    member_csrf = make_csrf_token(member, baseline.DEFAULT_CSRF_SECRET)
    org_a = f"{API}/organizations/{fixture['org_a_id']}"
    org_b = f"{API}/organizations/{fixture['org_b_id']}"
    entitlement = f"{org_a}/ops/entitlement"
    foreign = f"{org_b}/ops/entitlement"

    compare("E18 health live", *both("GET", f"{API}/health/live"), 200)
    compare("E18 health ready", *both("GET", f"{API}/health/ready"), 200)
    compare("E18 ops liveness", *both("GET", f"{API}/ops/health/live"), 200)
    compare(
        "E18 ops readiness",
        *both("GET", f"{API}/ops/health/ready"),
        200,
        normalize=parity.readiness_contract,
    )
    compare("E18 release capabilities", *both("GET", f"{API}/ops/release-capabilities"), 200)
    compare_metrics("E18 Prometheus metrics", *both("GET", f"{API}/ops/metrics"))

    compare("E18 entitlement defaults for owner", *both("GET", entitlement, token=owner), 200)
    compare("E18 entitlement member read", *both("GET", entitlement, token=member), 200)
    compare("E18 entitlement anonymous 401", *both("GET", entitlement), 401)
    compare("E18 entitlement foreign tenant 404", *both("GET", foreign, token=owner), 404)
    compare(
        "E18 entitlement malformed organization 422",
        *both("GET", f"{API}/organizations/not-a-uuid/ops/entitlement", token=owner),
        422,
    )

    put = {"deployment_profile": "cloud_saas", "plan_tier": "team_standard"}
    compare("E18 entitlement put anonymous 401", *both("PUT", entitlement, body=put), 401)
    compare(
        "E18 entitlement put without CSRF 403",
        *both("PUT", entitlement, token=owner, body=put),
        403,
    )
    for label, body in (
        ("extra field", {**put, "unexpected": True}),
        ("quota below minimum", {"max_storage_mb": 63}),
        ("quota above int32", {"max_projects": 2_147_483_648}),
        ("non-string feature", {"features_enabled": [1]}),
        ("too many features", {"features_enabled": ["R001"] * 16}),
        ("null profile", {"deployment_profile": None}),
        ("boolean storage quota below minimum", {"max_storage_mb": True}),
        ("fractional quota string", {"max_projects": "5.5"}),
        ("non-numeric quota string", {"max_projects": "five"}),
    ):
        compare(
            f"E18 entitlement put {label} 422",
            *both("PUT", entitlement, token=owner, csrf=csrf, body=body),
            422,
        )
    compare(
        "E18 entitlement put by member without settings permission 403",
        *both("PUT", entitlement, token=member, csrf=member_csrf, body=put),
        403,
    )
    compare(
        "E18 entitlement put foreign tenant 404",
        *both("PUT", foreign, token=owner, csrf=csrf, body=put),
        404,
    )
    for label, body in (
        ("invalid deployment profile", {"deployment_profile": "on_prem"}),
        ("invalid plan tier", {"plan_tier": "gold"}),
        ("duplicate feature", {"features_enabled": ["R001", "R001"]}),
        ("unknown feature", {"features_enabled": ["R099"]}),
    ):
        compare(
            f"E18 entitlement put {label} 422",
            *both("PUT", entitlement, token=owner, csrf=csrf, body=body),
            422,
        )

    # FastAPI decodes a JSON body before authentication; other content types fail afterwards.
    malformed = {"raw": b'{"plan_tier": ', "content_type": "application/json"}
    compare(
        "E18 entitlement put malformed JSON anonymous 422",
        *both("PUT", entitlement, **malformed),
        422,
    )
    compare(
        "E18 entitlement put malformed JSON without CSRF 422",
        *both("PUT", entitlement, token=owner, **malformed),
        422,
    )
    text_body = {"raw": b"{}", "content_type": "text/plain"}
    compare(
        "E18 entitlement put text/plain anonymous 401",
        *both("PUT", entitlement, **text_body),
        401,
    )
    compare(
        "E18 entitlement put text/plain body 422",
        *both("PUT", entitlement, token=owner, csrf=csrf, **text_body),
        422,
    )

    upserts = 0
    for label, kwargs in (
        ("integer string quota", {"body": {"max_projects": "5"}}),
        ("padded integer string quota", {"body": {"max_projects": " 6 "}}),
        ("boolean quota", {"body": {"max_projects": True}}),
        (
            "merge-patch+json content type",
            {"raw": b'{"max_projects": 4}', "content_type": "application/merge-patch+json"},
        ),
    ):
        compare(
            f"E18 entitlement upsert {label}",
            *both("PUT", entitlement, token=owner, csrf=csrf, **kwargs),
            200,
            normalize=stamped,
        )
        upserts += 1

    configured = {
        "deployment_profile": "enterprise_private",
        "plan_tier": "enterprise",
        "max_projects": 7.0,
        "max_storage_mb": 2048,
        "features_enabled": ["R015", "R001", "R011"],
    }
    compare(
        "E18 entitlement upsert",
        *both("PUT", entitlement, token=owner, csrf=csrf, body=configured),
        200,
        normalize=stamped,
    )
    upserts += 1
    compare("E18 entitlement stored record", *both("GET", entitlement, token=owner), 200)
    with psycopg.connect(owner_url) as conn:
        rows = conn.execute(
            """SELECT actor_type, actor_id, actor_display_snapshot, resource_type,
                      resource_id, outcome, metadata
               FROM audit.audit_events
               WHERE organization_id = %s AND action = 'ops.entitlement.configure'
               ORDER BY created_at""",
            (fixture["org_a_id"],),
        ).fetchall()
        check(
            "E18 entitlement audit events match",
            # One FastAPI then one Rust write per upsert case.
            len(rows) == 2 * upserts and rows[0::2] == rows[1::2],
            f"expected {upserts} identical audit pairs, got {rows}",
        )
        conn.execute(
            "UPDATE org.organizations SET status = 'suspended' WHERE id = %s",
            (fixture["org_b_id"],),
        )
    try:
        compare(
            "E18 entitlement suspended organization 403",
            *both("GET", foreign, token=fixture["session_b_token"]),
            403,
        )
    finally:
        with psycopg.connect(owner_url) as conn:
            conn.execute(
                "UPDATE org.organizations SET status = 'active' WHERE id = %s",
                (fixture["org_b_id"],),
            )


def check_privileged_role(binary, owner_url, work):
    """The owner role is refused as runtime identity, and quickly (not after the pool timeout)."""
    with parity.rust_client(binary, owner_url, work, ready_status=503) as rust:
        for path in (f"{API}/health/ready", f"{API}/ops/health/ready", f"{API}/ops/metrics"):
            started = time.monotonic()
            response = rust.get(path)
            elapsed = time.monotonic() - started
            if path.endswith("metrics"):
                refused = "openvietgis_database_ready 0\n" in response.text
            else:
                refused = response.json().get("database") == "unavailable"
            check(
                f"E18 privileged database role fails fast {path}",
                refused
                and response.status_code == (200 if path.endswith("metrics") else 503)
                and elapsed < 1.5,
                f"status {response.status_code} after {elapsed:.2f}s",
            )


def check_role_recovery(binary, admin_url, runtime_url, work):
    """Readiness refuses a runtime role that gains a privilege, then recovers once it is dropped."""
    role = sql.Identifier(baseline.DEFAULT_RUNTIME_USER)

    def alter(privilege):
        with psycopg.connect(admin_url, autocommit=True) as admin:
            admin.execute(sql.SQL("ALTER ROLE {} " + privilege).format(role))

    def timed(rust, path):
        started = time.monotonic()
        response = rust.get(path)
        return response, time.monotonic() - started

    alter("CREATEDB")
    try:
        with parity.rust_client(binary, runtime_url, work, ready_status=503) as rust:
            response, elapsed = timed(rust, f"{API}/health/ready")
            check(
                "E18 runtime role with CREATEDB refused fast 503",
                response.status_code == 503 and elapsed < 1.5,
                f"status {response.status_code} after {elapsed:.2f}s",
            )
            alter("NOCREATEDB")
            for path in (f"{API}/health/ready", f"{API}/ops/health/ready"):
                response, elapsed = timed(rust, path)
                check(
                    f"E18 readiness recovers after the role is fixed {path}",
                    response.status_code == 200 and elapsed < 1.5,
                    f"status {response.status_code} after {elapsed:.2f}s",
                )
    finally:
        alter("NOCREATEDB")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e18-parity-") as directory:
        work = Path(directory)
        with parity.database(admin_url) as (owner_url, runtime_url):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            parity.configure_python(owner_url, runtime_url, artifacts)
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            key = baseline.DEFAULT_CSRF_SECRET
            fixture = baseline.seed_benchmark_database(
                owner_url,
                seed=20261126,
                scales=["1k"],
                csrf_secret=key,
            )
            _, _, member = parity.scoped_member(owner_url, fixture)

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    run_cases(oracle, rust, owner_url, fixture, member)
            finally:
                get_engine().dispose()
            check_privileged_role(binary, owner_url, work)
            check_role_recovery(binary, admin_url, runtime_url, work)

    print("E18 ops parity done", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())