#!/usr/bin/env python3
"""Compare the native E01/E02 read slice with FastAPI over isolated real PostGIS."""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import os
import socket
import subprocess
import sys
import tempfile
import time
from datetime import UTC, datetime
from pathlib import Path
from urllib.parse import quote, urlsplit
from uuid import UUID, uuid4

import httpx
import psycopg
from psycopg import sql

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools" / "benchmark" / "rust-platform"))
import benchmark_runner as baseline

SEED = 20261125
API = "/api/v1"
COOKIE_NAME = "ovg_session"


def require(condition, detail):
    if not condition:
        raise AssertionError(detail)


@contextlib.contextmanager
def database(admin_url):
    parsed = urlsplit(admin_url)
    allowed = (
        parsed.scheme == "postgresql"
        and parsed.hostname in {"127.0.0.1", "localhost", "::1"}
        and parsed.username == "openvietgis"
        and parsed.path == "/postgres"
        and not (parsed.query or parsed.fragment)
    )
    if not allowed:
        raise ValueError(
            "Parity requires the loopback development PostGIS admin endpoint."
        )

    name = f"openvietgis_bench_parity_test_{uuid4().hex}"
    owner_url = parsed._replace(path=f"/{name}").geturl()
    host = parsed.netloc.rsplit("@", 1)[-1]
    runtime_url = parsed._replace(
        netloc=f"{baseline.DEFAULT_RUNTIME_USER}:{quote(baseline.DEFAULT_RUNTIME_PASS, safe='')}@{host}",
        path=f"/{name}",
    ).geturl()
    identifier = sql.Identifier(name)
    with psycopg.connect(admin_url, autocommit=True) as admin:
        admin.execute(
            sql.SQL(
                "CREATE DATABASE {} TEMPLATE template_postgis OWNER openvietgis"
            ).format(identifier)
        )
        try:
            yield owner_url, runtime_url
        finally:
            admin.execute(sql.SQL("DROP DATABASE {} WITH (FORCE)").format(identifier))


def configure_python(owner_url, runtime_url, artifacts):
    for key in list(os.environ):
        if key.startswith("OPENVIETGIS_"):
            del os.environ[key]
    runtime = runtime_url.replace("postgresql://", "postgresql+psycopg://", 1)
    os.environ.update(
        OPENVIETGIS_ENVIRONMENT="test",
        OPENVIETGIS_DATABASE_URL=runtime,
        OPENVIETGIS_RUNTIME_DATABASE_URL=runtime,
        OPENVIETGIS_MIGRATION_DATABASE_URL=owner_url.replace(
            "postgresql://", "postgresql+psycopg://", 1
        ),
        OPENVIETGIS_APPLICATION_SESSION_SECRET=baseline.DEFAULT_SESSION_SECRET,
        OPENVIETGIS_CSRF_SECRET=baseline.DEFAULT_CSRF_SECRET,
        OPENVIETGIS_BOOTSTRAP_SECRET=baseline.DEFAULT_BOOTSTRAP_SECRET,
        OPENVIETGIS_SESSION_COOKIE_NAME=COOKIE_NAME,
        OPENVIETGIS_ARTIFACT_STORE_DIR=str(artifacts),
    )


@contextlib.contextmanager
def rust_client(binary, runtime_url, directory, *, ready_status=200):
    with socket.socket() as port_probe:
        port_probe.bind(("127.0.0.1", 0))
        port = port_probe.getsockname()[1]
    env = {
        key: value for key, value in os.environ.items() if not key.startswith("OVG_")
    }
    env.update(
        OPENVIETGIS_BIND_ADDR=f"127.0.0.1:{port}",
        OPENVIETGIS_DB_MAX_CONNECTIONS="1",
        OPENVIETGIS_DATABASE_URL=runtime_url,
        RUST_LOG="warn",
    )
    with (
        (directory / "rust.log").open("w+") as log,
        subprocess.Popen(
            [str(binary)], cwd=directory, env=env, stdout=log, stderr=log
        ) as proc,
    ):
        try:
            with httpx.Client(
                # Above the Rust pool acquire timeout (5 s): a rejected DB role answers 503 only then.
                base_url=f"http://127.0.0.1:{port}", trust_env=False, timeout=10
            ) as client:
                ready_path = f"{API}/health/ready"
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    if proc.poll() is not None:
                        break
                    try:
                        if client.get(ready_path).status_code == ready_status:
                            break
                    except httpx.TransportError:
                        pass
                    time.sleep(0.15)
                else:
                    raise RuntimeError(
                        "Rust did not report readiness within 15 seconds"
                    )
                if proc.poll() is not None:
                    raise RuntimeError(
                        f"Rust exited during startup ({proc.returncode})"
                    )
                yield client
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


def session_headers(token, **extra):
    if token is None:
        return extra
    return {"cookie": f"{COOKIE_NAME}={token}", **extra}


def session_id(token):
    return UUID(hex=token.split(".", 1)[0].removeprefix("ovgs_"))


def project_path(org_id, project_id):
    return f"{API}/organizations/{org_id}/projects/{project_id}"


def scoped_member(owner_url, fixture):
    uid = uuid4()
    lid = uuid4()
    sid = uuid4()
    mid = uuid4()
    token, digest, _ = baseline.make_session_and_csrf(sid, baseline.DEFAULT_CSRF_SECRET)
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """INSERT INTO identity.users
               (id, display_name, email_snapshot, status, created_at, updated_at)
               VALUES (%s, 'Parity Surveyor', 'surveyor@example.test', 'active', now(), now())""",
            (uid,),
        )
        conn.execute(
            """INSERT INTO identity.identity_links
               (id, user_id, issuer, subject, email_snapshot, email_verified,
                display_name_snapshot, created_at)
               VALUES (%s, %s, 'https://parity-issuer.example', 'surveyor',
                       'surveyor@example.test', true, 'Parity Surveyor', now())""",
            (lid, uid),
        )
        conn.execute(
            """INSERT INTO identity.web_sessions
               (id, user_id, identity_link_id, secret_hash, issued_at, expires_at)
               VALUES (%s, %s, %s, %s, now(), now() + interval '1 hour')""",
            (sid, uid, lid, digest),
        )
        conn.execute(
            """INSERT INTO org.organization_memberships
               (id, organization_id, user_id, role, status, created_at, activated_at)
               VALUES (%s, %s, %s, 'member', 'active', now(), now())""",
            (mid, fixture["org_a_id"], uid),
        )
    return str(uid), mid, token


def verify_database_role(runtime_url):
    with psycopg.connect(runtime_url) as conn:
        role, superuser, bypassrls, postgis = conn.execute(
            """SELECT current_user, rolsuper, rolbypassrls, PostGIS_Version()
               FROM pg_roles WHERE rolname=current_user"""
        ).fetchone()
        require(
            role == baseline.DEFAULT_RUNTIME_USER
            and not superuser
            and not bypassrls
            and postgis,
            "Runtime PostGIS connection must use the restricted migration role.",
        )
        count = conn.execute("SELECT count(*) FROM spatial.layers").fetchone()[0]
        require(count == 0, "Runtime role without tenant scope exposed spatial layers.")
        print("PASS DB role + unscoped RLS", flush=True)


class Differential:
    def __init__(self, oracle, rust):
        self.oracle = oracle
        self.rust = rust
        self.passed = 0
        self.failures = []

    def get(
        self,
        label,
        path,
        status=200,
        *,
        token=None,
        headers=None,
        params=None,
        check=None,
        normalize=None,
    ):
        request_headers = session_headers(token, **(headers or {}))
        expected_body = None
        try:
            expected = self.oracle.get(path, headers=request_headers, params=params)
            actual = self.rust.get(path, headers=request_headers, params=params)
            require(
                expected.status_code == status,
                f"FastAPI status {expected.status_code}, wanted {status}",
            )
            expected_body = expected.json()
            require(
                actual.status_code == status,
                f"Rust status {actual.status_code}, wanted {status}",
            )
            actual_body = actual.json()
            mismatch = (
                difference(expected_body, actual_body)
                if normalize is None
                else difference(normalize(expected_body), normalize(actual_body))
            )
            require(mismatch is None, mismatch)
            expected_cache = expected.headers.get("cache-control")
            require(
                actual.headers.get("cache-control") == expected_cache,
                "Cache-Control differs from FastAPI",
            )
            if check is not None:
                check(expected_body)
        except (AssertionError, ValueError, httpx.HTTPError) as exc:
            self.failures.append(label)
            print(f"FAIL {label}: {exc}", flush=True)
        else:
            self.passed += 1
            print(f"PASS {label}", flush=True)
        return expected_body


def difference(expected, actual, path="$"):
    """Locate a mismatch without writing session or CSRF values to evidence."""
    if isinstance(expected, dict) and isinstance(actual, dict):
        missing = sorted(expected.keys() - actual.keys())
        extra = sorted(actual.keys() - expected.keys())
        if missing or extra:
            return f"{path}: missing keys {missing}; extra keys {extra}"
        for key in sorted(expected):
            mismatch = difference(expected[key], actual[key], f"{path}.{key}")
            if mismatch is not None:
                return mismatch
    elif isinstance(expected, list) and isinstance(actual, list):
        if len(expected) != len(actual):
            return (
                f"{path}: Rust has {len(actual)} entries, FastAPI has {len(expected)}"
            )
        for index, (left, right) in enumerate(zip(expected, actual)):
            mismatch = difference(left, right, f"{path}[{index}]")
            if mismatch is not None:
                return mismatch
    elif type(expected) is not type(actual) or expected != actual:
        return f"{path}: value or JSON type differs from FastAPI"
    return None


def setup_gis_scope(oracle, fixture, base, layer_id, member_id, user_id):
    headers = session_headers(
        fixture["session_a_token"], **{"x-openvietgis-csrf": fixture["csrf_a_token"]}
    )
    response = oracle.post(
        f"{API}/organizations/{fixture['org_a_id']}/grants",
        headers=headers,
        json={
            "subject_kind": "membership",
            "subject_id": str(member_id),
            "scope_kind": "project",
            "project_id": fixture["project_a_id"],
            "role": "viewer",
        },
    )
    require(
        response.status_code == 201,
        f"FastAPI fixture grant failed ({response.status_code})",
    )
    response = oracle.post(
        f"{base}/access/rules",
        headers=headers,
        json={
            "user_id": user_id,
            "layer_id": layer_id,
            "actions": ["read"],
            "attribute_filter": {"code": "P4326-000001"},
        },
    )
    require(
        response.status_code == 201,
        f"FastAPI fixture GIS rule failed ({response.status_code})",
    )
    return response.json()["id"], headers


def readiness_contract(response):
    checked = response.get("checked_at")
    require(
        isinstance(checked, str)
        and datetime.fromisoformat(checked).utcoffset() == UTC.utcoffset(None),
        "readiness checked_at must be an ISO 8601 UTC timestamp",
    )
    return {key: value for key, value in response.items() if key != "checked_at"}


def compare_feature_reads(suite, features, selected, owner, outsider, surveyor):
    first_id, second_id = "poly-000001", "poly-000002"
    pins = {key: selected[key] for key in ("content_revision", "schema_revision")}
    detail = f"{features}/{first_id}"
    suite.get(
        "E02 feature page native CRS",
        features,
        token=owner,
        params=pins | {"limit": 1},
    )
    suite.get(
        "E02 feature page cursor",
        features,
        token=owner,
        params=pins | {"after_feature_id": first_id, "limit": 1},
    )
    suite.get("E02 native feature detail", detail, token=owner, params=pins)
    suite.get(
        "E02 feature detail attributes only",
        detail,
        token=owner,
        params=pins | {"include_geometry": "false"},
    )
    suite.get(
        "E02 feature transformed to Web Mercator",
        detail,
        token=owner,
        params=pins | {"response_crs": 3857},
    )
    suite.get(
        "E02 foreign tenant feature detail", detail, 404, token=outsider, params=pins
    )
    suite.get(
        "E02 stale feature revision pin",
        detail,
        409,
        token=owner,
        params=pins | {"content_revision": "0" * 64},
    )
    suite.get(
        "E02 invalid feature cursor",
        features,
        422,
        token=owner,
        params=pins | {"after_feature_id": "does-not-exist"},
    )

    def allowed_only(body):
        require(
            [item["feature_id"] for item in body["items"]] == [first_id],
            "Feature page leaked data outside the GIS rule.",
        )

    suite.get(
        "E02 scoped feature page",
        features,
        token=surveyor,
        params=pins,
        check=allowed_only,
    )
    suite.get(
        "E02 scoped feature detail denied",
        f"{features}/{second_id}",
        404,
        token=surveyor,
        params=pins,
    )
    suite.get(
        "E02 scoped feature cursor cannot reveal hidden data",
        features,
        422,
        token=surveyor,
        params=pins | {"after_feature_id": second_id},
    )


def compare(suite, owner_url, fixture, user_id, mid, surveyor, *, include_features):

    a, b = fixture["session_a_token"], fixture["session_b_token"]
    root_a = project_path(fixture["org_a_id"], fixture["project_a_id"])
    root_b = project_path(fixture["org_b_id"], fixture["project_b_id"])
    layers_a = f"{root_a}/layers"
    layers_b = f"{root_b}/layers"
    selected = fixture["layers"]["epsg4326_1k"]
    layer_a = f"{layers_a}/{selected['layer_id']}"
    layer_b_id = fixture["layers"]["tenant_b_1k"]["layer_id"]
    layer_a_other = f"{layers_a}/{fixture['layers']['edit_4326']['layer_id']}"
    token_parts = a.split(".", 1)
    forged = token_parts[0] + "." + "forged-parity-token"
    suite.get("E01 health live", f"{API}/health/live")
    suite.get("E01 health ready", f"{API}/health/ready")
    suite.get("E18 ops liveness", f"{API}/ops/health/live")
    suite.get(
        "E18 ops ready on PostGIS and artifact store",
        f"{API}/ops/health/ready",
        normalize=readiness_contract,
    )
    suite.get("E01 bootstrap status", f"{API}/bootstrap/status")
    for endpoint in (
        "me",
        "me/organizations",
        "me/identity-links",
        "me/avatar",
        "auth/csrf",
    ):
        suite.get(f"E01 owner A {endpoint}", f"{API}/{endpoint}", token=a)
        suite.get(f"E01 unauthenticated {endpoint}", f"{API}/{endpoint}", 401)
    suite.get("E01 owner B me", f"{API}/me", token=b)
    suite.get("E01 malformed session", f"{API}/me", 401, token="invalid")
    suite.get("E01 incorrect secret", f"{API}/me", 401, token=forged)
    suite.get(
        "E01 invalid bearer takes precedence over valid cookie",
        f"{API}/me",
        401,
        token=a,
        headers={"authorization": "Bearer invalid"},
    )
    suite.get("E01 member organizations", f"{API}/me/organizations", token=surveyor)
    org_a = f"{API}/organizations/{fixture['org_a_id']}"
    org_b = f"{API}/organizations/{fixture['org_b_id']}"
    suite.get("E01 owner organization detail", org_a, token=a)
    suite.get("E01 member organization detail", org_a, token=surveyor)
    suite.get("E01 foreign organization detail", org_b, 404, token=a)
    suite.get("E02 member without project grant", layers_a, 403, token=surveyor)
    suite.get("E02 owner A layer list", layers_a, token=a)
    suite.get("E02 owner A layer detail", layer_a, token=a)
    first = suite.get(
        "E02 layer cursor first page", layers_a, token=a, params={"limit": 1}
    )
    if first and first.get("next_after_id"):
        suite.get(
            "E02 layer cursor next page",
            layers_a,
            token=a,
            params={"limit": 1, "after_id": first["next_after_id"]},
        )
    else:
        suite.failures.append("E02 layer cursor next page")
        print(
            "FAIL E02 layer cursor next page: FastAPI fixture has no second page",
            flush=True,
        )
    suite.get(
        "E02 foreign layer under own tenant", f"{layers_a}/{layer_b_id}", 404, token=a
    )
    suite.get(
        "E02 missing layer under own tenant", f"{layers_a}/{uuid4()}", 404, token=a
    )
    suite.get(
        "E02 foreign project under own organization",
        f"{project_path(fixture['org_a_id'], fixture['project_b_id'])}/layers",
        404,
        token=a,
    )
    suite.get("E02 bounded page limit", layers_a, 422, token=a, params={"limit": 101})
    suite.get(
        "E02 invalid cursor syntax",
        layers_a,
        422,
        token=a,
        params={"after_id": "invalid"},
    )
    for attempt in range(1, 4):
        suite.get(f"E02 tenant A pool reuse {attempt}", layers_a, token=a)
        suite.get(f"E02 tenant B denied A detail {attempt}", layer_a, 404, token=b)
        suite.get(f"E02 tenant B owns B {attempt}", layers_b, token=b)
        suite.get(f"E02 no session after pool reuse {attempt}", layers_b, 401)
    sources = f"{root_a}/spatial/sources"
    suite.get("E02 source list", sources, token=a)
    suite.get("E02 source detail", f"{sources}/{selected['source_id']}", token=a)
    suite.get("E02 ordinary CRS profiles", f"{root_a}/spatial/crs-profiles", token=a)
    rule, admin_headers = setup_gis_scope(
        suite.oracle,
        fixture,
        root_a,
        selected["layer_id"],
        mid,
        user_id,
    )

    def redacted(body):
        source = body["source"]
        require(source["feature_count"] == 1, "Scoped actor must see only one feature")
        require(source["extent"] is None, "Scoped actor received the full extent")
        require(
            all(value is None for value in source["provenance"].values()),
            "Provenance leaked",
        )

    def only_allowed(body):
        require(
            [item["id"] for item in body["items"]] == [selected["layer_id"]],
            "ACL list leaked layers",
        )
        redacted(body["items"][0])

    suite.get("E02 scoped user cannot enumerate sources", sources, 403, token=surveyor)
    suite.get(
        "E02 scoped CRS endpoint denied",
        f"{root_a}/spatial/crs-profiles",
        403,
        token=surveyor,
    )
    suite.get("E02 scoped layer list", layers_a, token=surveyor, check=only_allowed)
    suite.get(
        "E02 scoped layer source redaction", layer_a, token=surveyor, check=redacted
    )
    suite.get("E02 scoped layer hidden", layer_a_other, 404, token=surveyor)
    if include_features:
        compare_feature_reads(suite, f"{layer_a}/features", selected, a, b, surveyor)
    response = suite.oracle.delete(
        f"{root_a}/access/rules/{rule}", headers=admin_headers
    )
    require(
        response.status_code == 204,
        f"FastAPI fixture revoke failed ({response.status_code})",
    )
    suite.get("E02 scoped grant revoked immediately", layer_a, 404, token=surveyor)
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            "UPDATE org.organization_memberships SET status='revoked', revoked_at=now() WHERE id=%s",
            (mid,),
        )
        conn.execute(
            "UPDATE identity.web_sessions SET expires_at=issued_at WHERE id=%s",
            (session_id(a),),
        )
        conn.execute(
            "UPDATE identity.web_sessions SET revoked_at=now() WHERE id=%s",
            (session_id(b),),
        )
    suite.get("E01 expired session", f"{API}/me", 401, token=a)
    suite.get("E01 revoked session", f"{API}/me", 401, token=b)
    suite.get(
        "E01 revoked membership omitted", f"{API}/me/organizations", token=surveyor
    )
    suite.get(
        "E02 revoked membership invalidates layer access", layers_a, 404, token=surveyor
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--candidate",
        type=Path,
        default=ROOT / "target" / "debug" / "openvietgis-api-rust",
    )
    parser.add_argument("--include-features", action="store_true")
    args = parser.parse_args()
    binary = args.candidate.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error("build the Rust API candidate before running differential parity")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="openvietgis-parity-") as directory:
        work = Path(directory)
        os.chdir(work)
        with database(admin_url) as (owner_url, runtime_url):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            configure_python(owner_url, runtime_url, artifacts)
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            fixture = baseline.seed_benchmark_database(
                owner_url,
                seed=SEED,
                scales=["1k"],
                csrf_secret=baseline.DEFAULT_CSRF_SECRET,
            )
            verify_database_role(runtime_url)
            user_id, mid, surveyor = scoped_member(owner_url, fixture)
            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    rust_client(binary, runtime_url, work) as rust,
                ):
                    suite = Differential(oracle, rust)
                    compare(
                        suite,
                        owner_url,
                        fixture,
                        user_id,
                        mid,
                        surveyor,
                        include_features=args.include_features,
                    )
                    artifacts.rmdir()
                    suite.get(
                        "E18 ops not ready when artifact store is missing",
                        f"{API}/ops/health/ready",
                        503,
                        normalize=readiness_contract,
                    )
                    artifacts.mkdir()
            finally:
                get_engine().dispose()
            with rust_client(binary, owner_url, work, ready_status=503) as rust:
                response = rust.get(f"{API}/health/ready")
                require(
                    response.status_code == 503
                    and response.json()["database"] == "unavailable",
                    "Rust health must reject the privileged database owner as runtime identity.",
                )
                print("PASS E01 privileged database owner rejected", flush=True)
    with binary.open("rb") as candidate:
        sha = hashlib.file_digest(candidate, "sha256").hexdigest()
    print(f"Rust candidate SHA-256 {sha}")
    print(f"HTTP parity {suite.passed} passed, {len(suite.failures)} failed")
    return int(bool(suite.failures))


if __name__ == "__main__":
    raise SystemExit(main())
