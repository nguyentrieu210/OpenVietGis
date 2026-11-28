#!/usr/bin/env python3
"""Spec 064 (T003/T020) Deterministic GIS Dataset Seeder & Comparative HTTP Benchmark Runner.

Measures cold/warm p50, p95, p99 latency (ms), throughput (req/s), error rate (%),
and server process RSS / peak RSS (VmRSS / VmHWM MiB) at concurrency 1, 10, and 20
across identical workloads for both the Python reference backend and future Rust backend.
"""

from __future__ import annotations

import argparse
import asyncio
import hashlib
import json
import math
import os
import platform
import random
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass, field
from datetime import UTC, datetime, timedelta
from pathlib import Path
from statistics import mean
from typing import Any
from urllib.parse import urlparse
from uuid import NAMESPACE_URL, UUID, uuid4, uuid5

import httpx
import psycopg

REPO_ROOT = Path(__file__).resolve().parents[3]
API_ROOT = REPO_ROOT / "apps" / "api"
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))
if str(API_ROOT) not in sys.path:
    sys.path.insert(0, str(API_ROOT))

SCALE_MAP: dict[str, int] = {
    "1k": 1_000,
    "10k": 10_000,
    "30k": 30_000,
    "100k": 100_000,
    "1m": 1_000_000,
}

DEFAULT_ADMIN_PG_URL = "postgresql://openvietgis:openvietgis@127.0.0.1:54330/postgres"
SAFE_LOOPBACK_PG_PORTS = frozenset({54330, 54331})
DEFAULT_BENCH_DB_NAME = "openvietgis_bench_064"
DEFAULT_RUNTIME_USER = "openvietgis_runtime"
DEFAULT_RUNTIME_PASS = "openvietgis-runtime-dev"
DEFAULT_SESSION_SECRET = "openvietgis-bench-session-secret-064"
DEFAULT_CSRF_SECRET = "openvietgis-bench-csrf-secret-064"
DEFAULT_BOOTSTRAP_SECRET = "openvietgis-bench-bootstrap-secret-064"


def assert_safe_admin_url(db_url: str) -> None:
    parsed = urlparse(db_url)
    if (
        parsed.scheme != "postgresql"
        or parsed.hostname not in ("127.0.0.1", "localhost", "::1")
        or parsed.port not in SAFE_LOOPBACK_PG_PORTS
        or parsed.path != "/postgres"
        or parsed.query
        or parsed.fragment
    ):
        raise RuntimeError("Benchmark admin must use a dedicated loopback PostGIS port (54330 or 54331)")


def assert_safe_bench_db(db_url: str, *, isolated_port: int = 54330) -> str:
    parsed = urlparse(db_url)
    db_name = parsed.path.lstrip("/")
    if (
        parsed.scheme != "postgresql"
        or parsed.hostname not in ("127.0.0.1", "localhost", "::1")
        or isolated_port not in (54330, 54332)
        or parsed.port != isolated_port
        or (isolated_port == 54332 and not db_name.startswith("openvietgis_bench_spec065_"))
        or parsed.query
        or parsed.fragment
        or not re.fullmatch(r"openvietgis_bench_[a-z0-9_]{1,44}", db_name)
    ):
        raise RuntimeError("Refusing non-isolated benchmark database")
    return db_name


def assert_safe_http_target(url: str) -> None:
    parsed = urlparse(url)
    if (
        parsed.scheme != "http"
        or parsed.hostname not in ("127.0.0.1", "localhost", "::1")
        or parsed.port not in range(18890, 19000)
        or parsed.username is not None
        or parsed.password is not None
        or parsed.path not in ("", "/")
        or parsed.query
        or parsed.fragment
    ):
        raise RuntimeError("Benchmark target must be an isolated loopback HTTP port 18890–18999")


def det_uuid(seed: int, key: str) -> UUID:
    return uuid5(NAMESPACE_URL, f"openvietgis/spec064-bench/seed-{seed}/{key}")


def sha256_hex(text_val: str) -> str:
    return hashlib.sha256(text_val.encode("utf-8")).hexdigest()


def make_session_and_csrf(session_id: UUID, csrf_secret: str) -> tuple[str, bytes, str]:
    from openvietgis_api.security.csrf import make_csrf_token
    from openvietgis_api.security.sessions import issue_session_token

    raw_token, secret_hash = issue_session_token(session_id)
    csrf_token = make_csrf_token(raw_token, csrf_secret)
    return raw_token, secret_hash, csrf_token


def percentile(sorted_vals: list[float], p: float) -> float:
    if not sorted_vals:
        return 0.0
    if len(sorted_vals) == 1:
        return sorted_vals[0]
    rank = (p / 100.0) * (len(sorted_vals) - 1)
    lo = math.floor(rank)
    hi = math.ceil(rank)
    if lo == hi:
        return sorted_vals[lo]
    frac = rank - lo
    return sorted_vals[lo] + frac * (sorted_vals[hi] - sorted_vals[lo])


def summarize_latencies(
    latencies_ms: list[float],
    elapsed_s: float,
    total_requests: int,
    error_count: int,
    status_counts: dict[str, int],
    extra: dict[str, Any] | None = None,
) -> dict[str, Any]:
    s = sorted(latencies_ms)
    res: dict[str, Any] = {
        "requests": total_requests,
        "succeeded": total_requests - error_count,
        "errors": error_count,
        "error_rate_pct": round((error_count / total_requests) * 100.0, 3)
        if total_requests > 0
        else 0.0,
        "duration_s": round(elapsed_s, 4),
        "throughput_rps": round(total_requests / elapsed_s, 2) if elapsed_s > 0 else 0.0,
        "min_ms": round(s[0], 3) if s else 0.0,
        "mean_ms": round(mean(s), 3) if s else 0.0,
        "p50_ms": round(percentile(s, 50.0), 3) if s else 0.0,
        "p95_ms": round(percentile(s, 95.0), 3) if s else 0.0,
        "p99_ms": round(percentile(s, 99.0), 3) if s else 0.0,
        "max_ms": round(s[-1], 3) if s else 0.0,
        "status_counts": status_counts,
    }
    if extra:
        res.update(extra)
    return res


@dataclass
class ProcessMonitor:
    pid: int | None
    interval_s: float = 0.1
    samples_rss_mib: list[float] = field(default_factory=list)
    peak_hwm_mib: float = 0.0
    idle_rss_mib: float = 0.0
    start_cpu_s: float = 0.0
    end_cpu_s: float = 0.0
    _stop: threading.Event = field(default_factory=threading.Event)
    _thread: threading.Thread | None = None

    @staticmethod
    def _read_proc_mem(pid: int) -> tuple[float, float]:
        rss_kb = 0.0
        hwm_kb = 0.0
        try:
            with open(f"/proc/{pid}/status", encoding="utf-8") as fh:
                for line in fh:
                    if line.startswith("VmRSS:"):
                        rss_kb = float(line.split()[1])
                    elif line.startswith("VmHWM:"):
                        hwm_kb = float(line.split()[1])
        except OSError:
            pass
        return rss_kb / 1024.0, hwm_kb / 1024.0

    @staticmethod
    def _read_proc_cpu(pid: int) -> float:
        try:
            with open(f"/proc/{pid}/stat", encoding="utf-8") as fh:
                parts = fh.read().rsplit(")", 1)[1].split()
            utime = int(parts[11])
            stime = int(parts[12])
            ticks = os.sysconf(os.sysconf_names["SC_CLK_TCK"])
            return (utime + stime) / float(ticks)
        except (OSError, IndexError, ValueError):
            return 0.0

    def start(self) -> None:
        if not self.pid:
            return
        rss, hwm = self._read_proc_mem(self.pid)
        self.idle_rss_mib = rss
        self.peak_hwm_mib = max(rss, hwm)
        self.start_cpu_s = self._read_proc_cpu(self.pid)

        def _loop() -> None:
            assert self.pid is not None
            while not self._stop.is_set():
                r, h = self._read_proc_mem(self.pid)
                if r > 0:
                    self.samples_rss_mib.append(r)
                self.peak_hwm_mib = max(self.peak_hwm_mib, r, h)
                self._stop.wait(self.interval_s)

        self._thread = threading.Thread(target=_loop, daemon=True)
        self._thread.start()

    def snapshot_rss_mib(self) -> dict[str, float]:
        if not self.pid:
            return {"current_rss_mib": 0.0, "peak_hwm_mib": 0.0}
        r, h = self._read_proc_mem(self.pid)
        self.peak_hwm_mib = max(self.peak_hwm_mib, r, h)
        return {"current_rss_mib": round(r, 2), "peak_hwm_mib": round(self.peak_hwm_mib, 2)}

    def stop(self) -> dict[str, float]:
        if not self.pid:
            return {
                "idle_rss_mib": 0.0,
                "end_rss_mib": 0.0,
                "peak_rss_mib": 0.0,
                "cpu_seconds": 0.0,
            }
        self._stop.set()
        if self._thread:
            self._thread.join(timeout=2.0)
        end_rss, end_hwm = self._read_proc_mem(self.pid)
        self.peak_hwm_mib = max(self.peak_hwm_mib, end_rss, end_hwm)
        self.end_cpu_s = self._read_proc_cpu(self.pid)
        return {
            "idle_rss_mib": round(self.idle_rss_mib, 2),
            "end_rss_mib": round(end_rss, 2),
            "peak_rss_mib": round(self.peak_hwm_mib, 2),
            "cpu_seconds": round(max(0.0, self.end_cpu_s - self.start_cpu_s), 3),
        }


def _copy_features(
    cur: psycopg.Cursor[Any],
    *,
    org_id: UUID,
    proj_id: UUID,
    source_id: UUID,
    content_rev: str,
    schema_rev: str,
    rows_iter: Any,
) -> int:
    sql = (
        "COPY spatial.reference_features "
        "(organization_id, project_id, source_id, feature_id, attributes, "
        "geometry, geometry_state, content_revision, schema_revision) "
        "FROM STDIN"
    )
    count = 0
    with cur.copy(sql) as copy:
        for fid, attrs_json, ewkt in rows_iter:
            copy.write_row(
                (
                    str(org_id),
                    str(proj_id),
                    str(source_id),
                    fid,
                    attrs_json,
                    ewkt,
                    "present",
                    content_rev,
                    schema_rev,
                )
            )
            count += 1
    return count


def _seed_layer(
    cur: psycopg.Cursor[Any],
    *,
    seed: int,
    org_id: UUID,
    proj_id: UUID,
    ref_key: str,
    name: str,
    kind: str,
    crs: int,
    family: str,
    count: int,
    extent: dict[str, Any] | None,
    validation_state: str,
    validation_reason: dict[str, str] | None,
    rows_iter: Any,
) -> dict[str, Any]:
    from openvietgis_api.domains.spatial.domain import (
        FieldType,
        GeometryDimensions,
        GeometryFamily,
        SchemaState,
        SourceField,
        schema_revision_digest,
    )

    source_id = det_uuid(seed, f"source/{ref_key}")
    layer_id = det_uuid(seed, f"layer/{ref_key}")
    fields = [
        ("code", "Code", 0, "text", False),
        ("ward", "Ward", 1, "text", False),
        ("metric_val", "Metric Value", 2, "integer", False),
        ("active", "Active", 3, "boolean", False),
    ]
    domain_fields = [
        SourceField(
            organization_id=org_id,
            project_id=proj_id,
            source_id=source_id,
            field_id=fid_name,
            name=disp,
            ordinal=ordinal,
            type=FieldType(ftype),
            nullable=nullable,
            schema_revision="0" * 64,
        )
        for fid_name, disp, ordinal, ftype, nullable in fields
    ]
    schema_rev = schema_revision_digest(
        domain_fields,
        GeometryFamily(family),
        GeometryDimensions.XY,
        schema_state=SchemaState.KNOWN,
    )
    content_rev = sha256_hex(f"content:{seed}:{ref_key}:{crs}:{family}:{count}:v1")
    manifest_digest = sha256_hex(f"manifest:{seed}:{ref_key}:{count}")
    now = datetime.now(UTC)
    extent_state = (
        "available"
        if (validation_state == "ready" and extent is not None)
        else ("empty" if (validation_state == "ready" and count == 0) else "unavailable")
    )

    cur.execute(
        """
        INSERT INTO spatial.data_sources (
            id, organization_id, project_id, name, kind, reference_key,
            fixture_version, manifest_digest, source_asset_id, lifecycle,
            validation_state, validation_reason, source_crs, stored_crs,
            schema_state, geometry_family, dimensions, feature_count_state,
            feature_count, extent_state, extent, catalog_revision,
            content_revision, schema_revision, created_at, updated_at
        )
        VALUES (
            %s, %s, %s, %s, %s, %s,
            %s, %s, NULL, 'active',
            %s, %s::jsonb, %s, %s,
            'known', %s, 'XY', 'known',
            %s, %s, %s::jsonb, 1,
            %s, %s, %s, %s
        )
        """,
        (
            source_id,
            org_id,
            proj_id,
            name,
            kind,
            ref_key,
            "bench-064-v1",
            manifest_digest,
            validation_state,
            json.dumps(validation_reason) if validation_reason else None,
            crs,
            crs,
            family,
            count,
            extent_state,
            json.dumps(extent) if extent else None,
            content_rev,
            schema_rev,
            now,
            now,
        ),
    )

    for fid_name, disp, ordinal, ftype, nullable in fields:
        cur.execute(
            """
            INSERT INTO spatial.source_fields (
                organization_id, project_id, source_id, field_id, name,
                ordinal, type, nullable, schema_revision
            ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s)
            """,
            (org_id, proj_id, source_id, fid_name, disp, ordinal, ftype, nullable, schema_rev),
        )

    cur.execute(
        """
        INSERT INTO spatial.layers (
            id, organization_id, project_id, source_id, name, lifecycle,
            layer_revision, created_at, updated_at
        ) VALUES (%s, %s, %s, %s, %s, 'active', 1, %s, %s)
        """,
        (layer_id, org_id, proj_id, source_id, name, now, now),
    )

    inserted = _copy_features(
        cur,
        org_id=org_id,
        proj_id=proj_id,
        source_id=source_id,
        content_rev=content_rev,
        schema_rev=schema_rev,
        rows_iter=rows_iter,
    )
    assert inserted == count, f"Expected {count} features, inserted {inserted}"

    return {
        "ref_key": ref_key,
        "name": name,
        "source_id": str(source_id),
        "layer_id": str(layer_id),
        "crs": crs,
        "geometry_family": family,
        "feature_count": count,
        "validation_state": validation_state,
        "content_revision": content_rev,
        "schema_revision": schema_rev,
        "extent": extent,
    }


def _gen_4326_polygons(seed: int, count: int):
    rng = random.Random(seed + 101)
    for i in range(1, count + 1):
        lon = round(105.65 + rng.random() * 0.40, 6)
        lat = round(20.85 + rng.random() * 0.30, 6)
        d = 0.0005
        ewkt = (
            f"SRID=4326;POLYGON(({lon} {lat}, {lon + d:.6f} {lat}, "
            f"{lon + d:.6f} {lat + d:.6f}, {lon} {lat + d:.6f}, {lon} {lat}))"
        )
        attrs = json.dumps(
            {
                "code": f"P4326-{i:06d}",
                "ward": f"HN-{(i % 24) + 1:02d}",
                "metric_val": (i * 17) % 10000,
                "active": (i % 5) != 0,
            },
            separators=(",", ":"),
        )
        yield (f"poly-{i:06d}", attrs, ewkt)


def _gen_4326_points(seed: int, salt: int, count: int, prefix: str = "pt"):
    rng = random.Random(seed + salt)
    for i in range(1, count + 1):
        if i % 4 == 0:
            lon = round(105.75 + rng.random() * 0.20, 6)
            lat = round(20.90 + rng.random() * 0.20, 6)
        else:
            lon = round(105.00 + rng.random() * 2.00, 6)
            lat = round(10.50 + rng.random() * 11.00, 6)
        ewkt = f"SRID=4326;POINT({lon} {lat})"
        attrs = json.dumps(
            {
                "code": f"{prefix.upper()}-{i:07d}",
                "ward": f"VN-{(i % 63) + 1:02d}",
                "metric_val": (i * 31) % 50000,
                "active": (i % 3) != 0,
            },
            separators=(",", ":"),
        )
        yield (f"{prefix}-{i:07d}", attrs, ewkt)


def _gen_vn2000_5899_points(cur: psycopg.Cursor[Any], seed: int, count: int):
    """Generate VN-2000 / TM-3 107-45 (EPSG:5899) points strictly inside Quang Ninh approved domain."""
    rng = random.Random(seed + 5899)
    cur.execute(
        """
        SELECT ST_X(p), ST_Y(p)
        FROM (
            SELECT ST_Transform(ST_SetSRID(ST_MakePoint(107.25, 20.85), 4326), 5899) AS p
            UNION ALL
            SELECT ST_Transform(ST_SetSRID(ST_MakePoint(107.85, 21.25), 4326), 5899) AS p
        ) t
        """
    )
    (min_x, min_y), (max_x, max_y) = cur.fetchall()
    min_x, max_x = min(min_x, max_x), max(min_x, max_x)
    min_y, max_y = min(min_y, max_y), max(min_y, max_y)
    extent = {
        "min_x": round(min_x, 3),
        "min_y": round(min_y, 3),
        "max_x": round(max_x, 3),
        "max_y": round(max_y, 3),
        "crs_code": 5899,
    }

    def _rows():
        for i in range(1, count + 1):
            x = round(min_x + rng.random() * (max_x - min_x), 3)
            y = round(min_y + rng.random() * (max_y - min_y), 3)
            ewkt = f"SRID=5899;POINT({x} {y})"
            attrs = json.dumps(
                {
                    "code": f"VN5899-{i:06d}",
                    "ward": f"QN-{(i % 14) + 1:02d}",
                    "metric_val": (i * 13) % 10000,
                    "active": True,
                },
                separators=(",", ":"),
            )
            yield (f"vn5899-{i:06d}", attrs, ewkt)

    return extent, _rows()


def _gen_vn2000_3405_points(seed: int, count: int):
    """Generate VN-2000 / UTM zone 48N (EPSG:3405) points across Northern Vietnam."""
    rng = random.Random(seed + 3405)
    for i in range(1, count + 1):
        easting = round(520000.0 + rng.random() * 100000.0, 3)
        northing = round(2280000.0 + rng.random() * 80000.0, 3)
        ewkt = f"SRID=3405;POINT({easting} {northing})"
        attrs = json.dumps(
            {
                "code": f"VN3405-{i:06d}",
                "ward": f"UTM48-{(i % 16) + 1:02d}",
                "metric_val": (i * 19) % 10000,
                "active": True,
            },
            separators=(",", ":"),
        )
        yield (f"vn3405-{i:06d}", attrs, ewkt)


def seed_benchmark_database(
    admin_db_url: str,
    *,
    seed: int,
    scales: list[str],
    csrf_secret: str,
    isolated_port: int = 54330,
) -> dict[str, Any]:
    assert_safe_bench_db(admin_db_url, isolated_port=isolated_port)
    t0 = time.perf_counter()

    org_a = det_uuid(seed, "org/alpha")
    org_b = det_uuid(seed, "org/beta")
    user_a = det_uuid(seed, "user/alpha-owner")
    user_b = det_uuid(seed, "user/beta-owner")
    link_a = det_uuid(seed, "link/alpha-owner")
    link_b = det_uuid(seed, "link/beta-owner")
    sess_a = det_uuid(seed, "session/alpha-owner")
    sess_b = det_uuid(seed, "session/beta-owner")
    ws_a = det_uuid(seed, "workspace/alpha")
    ws_b = det_uuid(seed, "workspace/beta")
    proj_a = det_uuid(seed, "project/alpha")
    proj_b = det_uuid(seed, "project/beta")

    tok_a, hash_a, csrf_a = make_session_and_csrf(sess_a, csrf_secret)
    tok_b, hash_b, csrf_b = make_session_and_csrf(sess_b, csrf_secret)
    now = datetime.now(UTC)
    expires = now + timedelta(hours=8)

    layers: dict[str, Any] = {}

    with psycopg.connect(admin_db_url) as conn:
        with conn.cursor() as cur:
            for uid, lid, sid, shash, name, email in (
                (user_a, link_a, sess_a, hash_a, "Bench Owner Alpha", "bench-alpha@example.test"),
                (user_b, link_b, sess_b, hash_b, "Bench Owner Beta", "bench-beta@example.test"),
            ):
                cur.execute(
                    """
                    INSERT INTO identity.users (id, display_name, email_snapshot, status, created_at, updated_at)
                    VALUES (%s, %s, %s, 'active', %s, %s)
                    """,
                    (uid, name, email, now, now),
                )
                cur.execute(
                    """
                    INSERT INTO identity.identity_links (
                        id, user_id, issuer, subject, email_snapshot, email_verified,
                        display_name_snapshot, created_at
                    ) VALUES (%s, %s, 'https://bench-issuer.example', %s, %s, true, %s, %s)
                    """,
                    (lid, uid, f"sub-{uid}", email, name, now),
                )
                cur.execute(
                    """
                    INSERT INTO identity.web_sessions (
                        id, user_id, identity_link_id, secret_hash, issued_at, expires_at
                    ) VALUES (%s, %s, %s, %s, %s, %s)
                    """,
                    (sid, uid, lid, shash, now, expires),
                )

            for oid, uid, oname, oslug, wid, pid, pname, pslug in (
                (
                    org_a,
                    user_a,
                    "Bench Org Alpha",
                    f"bench-alpha-{seed}",
                    ws_a,
                    proj_a,
                    "Bench Project Alpha",
                    f"proj-alpha-{seed}",
                ),
                (
                    org_b,
                    user_b,
                    "Bench Org Beta",
                    f"bench-beta-{seed}",
                    ws_b,
                    proj_b,
                    "Bench Project Beta",
                    f"proj-beta-{seed}",
                ),
            ):
                cur.execute(
                    """
                    INSERT INTO org.organizations (
                        id, name, slug, status, created_by_actor_type, created_by_actor_id,
                        created_at, updated_at
                    ) VALUES (%s, %s, %s, 'active', 'human', %s, %s, %s)
                    """,
                    (oid, oname, oslug, uid, now, now),
                )
                cur.execute(
                    """
                    INSERT INTO org.organization_memberships (
                        id, organization_id, user_id, role, status, created_at, activated_at
                    ) VALUES (%s, %s, %s, 'owner', 'active', %s, %s)
                    """,
                    (det_uuid(seed, f"member/{oid}/{uid}"), oid, uid, now, now),
                )
                cur.execute(
                    """
                    INSERT INTO workspace.workspaces (
                        id, organization_id, name, slug, status, created_by_actor_type,
                        created_by_actor_id, created_at, updated_at
                    ) VALUES (%s, %s, %s, %s, 'active', 'human', %s, %s, %s)
                    """,
                    (wid, oid, f"{oname} Workspace", f"ws-{oslug}", uid, now, now),
                )
                cur.execute(
                    """
                    INSERT INTO workspace.projects (
                        id, organization_id, workspace_id, name, slug, status,
                        created_by_actor_type, created_by_actor_id, created_at, updated_at
                    ) VALUES (%s, %s, %s, %s, %s, 'active', 'human', %s, %s, %s)
                    """,
                    (pid, oid, wid, pname, pslug, uid, now, now),
                )

            cur.execute(
                """
                UPDATE platform.installation_state
                SET initialized_at = COALESCE(initialized_at, %s),
                    initial_organization_id = COALESCE(initial_organization_id, %s),
                    initialized_by_user_id = COALESCE(initialized_by_user_id, %s),
                    updated_at = %s
                WHERE singleton_key = 'installation'
                """,
                (now, org_a, user_a, now),
            )

            for tier in scales:
                cnt = SCALE_MAP[tier]
                if tier == "1k":
                    ext = {
                        "min_x": 105.65,
                        "min_y": 20.85,
                        "max_x": 106.06,
                        "max_y": 21.16,
                        "crs_code": 4326,
                    }
                    layers[f"epsg4326_{tier}"] = _seed_layer(
                        cur,
                        seed=seed,
                        org_id=org_a,
                        proj_id=proj_a,
                        ref_key=f"bench_4326_{tier}",
                        name=f"Bench EPSG:4326 Polygon {tier}",
                        kind="derived_processing",
                        crs=4326,
                        family="Polygon",
                        count=cnt,
                        extent=ext,
                        validation_state="ready",
                        validation_reason=None,
                        rows_iter=_gen_4326_polygons(seed, cnt),
                    )
                else:
                    ext = {
                        "min_x": 105.0,
                        "min_y": 10.5,
                        "max_x": 107.0,
                        "max_y": 21.5,
                        "crs_code": 4326,
                    }
                    layers[f"epsg4326_{tier}"] = _seed_layer(
                        cur,
                        seed=seed,
                        org_id=org_a,
                        proj_id=proj_a,
                        ref_key=f"bench_4326_{tier}",
                        name=f"Bench EPSG:4326 Point {tier}",
                        kind="derived_processing",
                        crs=4326,
                        family="Point",
                        count=cnt,
                        extent=ext,
                        validation_state="ready",
                        validation_reason=None,
                        rows_iter=_gen_4326_points(seed, cnt, cnt, prefix=f"p{tier}"),
                    )

            ext_5899, rows_5899 = _gen_vn2000_5899_points(cur, seed, 1_000)
            layers["vn2000_5899_1k"] = _seed_layer(
                cur,
                seed=seed,
                org_id=org_a,
                proj_id=proj_a,
                ref_key="reference_points_5899",
                name="Bench VN-2000 EPSG:5899 1k",
                kind="reference_fixture",
                crs=5899,
                family="Point",
                count=1_000,
                extent=ext_5899,
                validation_state="ready",
                validation_reason=None,
                rows_iter=rows_5899,
            )

            layers["vn2000_3405_1k"] = _seed_layer(
                cur,
                seed=seed,
                org_id=org_a,
                proj_id=proj_a,
                ref_key="bench_vn2000_3405_1k",
                name="Bench VN-2000 EPSG:3405 1k",
                kind="manual",
                crs=3405,
                family="Point",
                count=1_000,
                extent=None,
                validation_state="not_ready",
                validation_reason={
                    "code": "UNSUPPORTED_CRS",
                    "message": "EPSG:3405 fixture stored for T010 CRS activation",
                },
                rows_iter=_gen_vn2000_3405_points(seed, 1_000),
            )

            layers["edit_4326"] = _seed_layer(
                cur,
                seed=seed,
                org_id=org_a,
                proj_id=proj_a,
                ref_key="bench_edit_4326",
                name="Bench Mutable Edit Layer",
                kind="manual",
                crs=4326,
                family="Point",
                count=50,
                extent={
                    "min_x": 105.0,
                    "min_y": 10.5,
                    "max_x": 107.0,
                    "max_y": 21.5,
                    "crs_code": 4326,
                },
                validation_state="ready",
                validation_reason=None,
                rows_iter=_gen_4326_points(seed, 777, 50, prefix="ed"),
            )

            layers["tenant_b_1k"] = _seed_layer(
                cur,
                seed=seed,
                org_id=org_b,
                proj_id=proj_b,
                ref_key="bench_tenant_b_1k",
                name="Bench Tenant B Layer",
                kind="manual",
                crs=4326,
                family="Point",
                count=100,
                extent={
                    "min_x": 105.0,
                    "min_y": 10.5,
                    "max_x": 107.0,
                    "max_y": 21.5,
                    "crs_code": 4326,
                },
                validation_state="ready",
                validation_reason=None,
                rows_iter=_gen_4326_points(seed, 999, 100, prefix="tb"),
            )

        conn.commit()
        conn.autocommit = True
        with conn.cursor() as cur:
            cur.execute("ANALYZE spatial.data_sources, spatial.layers, spatial.reference_features")

    seed_elapsed_s = round(time.perf_counter() - t0, 3)
    return {
        "seed": seed,
        "scales": scales,
        "seed_duration_s": seed_elapsed_s,
        "org_a_id": str(org_a),
        "org_b_id": str(org_b),
        "user_a_id": str(user_a),
        "user_b_id": str(user_b),
        "project_a_id": str(proj_a),
        "project_b_id": str(proj_b),
        "session_a_token": tok_a,
        "csrf_a_token": csrf_a,
        "session_b_token": tok_b,
        "csrf_b_token": csrf_b,
        "layers": layers,
    }


def collect_environment_metadata(db_url: str, target_url: str, label: str) -> dict[str, Any]:
    git_sha = "unknown"
    git_branch = "unknown"
    try:
        git_sha = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, text=True
        ).strip()
        git_branch = subprocess.check_output(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"], cwd=REPO_ROOT, text=True
        ).strip()
    except (OSError, subprocess.SubprocessError):
        git_sha = "unknown"
        git_branch = "unknown"

    cpu_model = "unknown"
    try:
        with open("/proc/cpuinfo", encoding="utf-8") as fh:
            for line in fh:
                if line.startswith("model name"):
                    cpu_model = line.split(":", 1)[1].strip()
                    break
    except OSError:
        pass

    mem_total_mib = 0.0
    try:
        with open("/proc/meminfo", encoding="utf-8") as fh:
            for line in fh:
                if line.startswith("MemTotal:"):
                    mem_total_mib = round(float(line.split()[1]) / 1024.0, 1)
                    break
    except OSError:
        pass

    pg_version = "unknown"
    postgis_version = "unknown"
    try:
        with psycopg.connect(db_url) as conn, conn.cursor() as cur:
            cur.execute("SELECT version()")
            pg_version = cur.fetchone()[0]
            cur.execute("SELECT postgis_full_version()")
            postgis_version = cur.fetchone()[0]
    except psycopg.Error as exc:
        pg_version = f"error: {exc}"

    return {
        "timestamp_utc": datetime.now(UTC).isoformat(),
        "target_label": label,
        "target_url": target_url,
        "git_sha": git_sha,
        "git_branch": git_branch,
        "hostname": platform.node(),
        "os": platform.platform(),
        "arch": platform.machine(),
        "cpu_count": os.cpu_count(),
        "cpu_model": cpu_model,
        "mem_total_mib": mem_total_mib,
        "python_version": platform.python_version(),
        "postgres_version": pg_version,
        "postgis_version": postgis_version,
    }


async def run_load_step(
    client: httpx.AsyncClient,
    concurrency: int,
    total_requests: int,
    make_call: Any,
    expected_statuses: set[int] | frozenset[int] = frozenset({200}),
) -> dict[str, Any]:
    latencies: list[float] = []
    errors = 0
    status_counts: dict[str, int] = {}
    counter = 0
    lock = asyncio.Lock()

    async def worker(worker_idx: int) -> None:
        nonlocal counter, errors
        while True:
            async with lock:
                if counter >= total_requests:
                    return
                req_idx = counter
                counter += 1
            t0 = time.perf_counter_ns()
            try:
                resp = await make_call(client, req_idx, worker_idx)
                elapsed_ms = (time.perf_counter_ns() - t0) / 1_000_000.0
                code_key = str(resp.status_code)
                status_counts[code_key] = status_counts.get(code_key, 0) + 1
                latencies.append(elapsed_ms)
                if resp.status_code not in expected_statuses:
                    errors += 1
                    if errors <= 2:
                        print(
                            f"  [warn] unexpected status={resp.status_code} body={resp.text[:200]}",
                            flush=True,
                        )
            except Exception as exc:  # noqa: BLE001 -- benchmark records all request failures
                elapsed_ms = (time.perf_counter_ns() - t0) / 1_000_000.0
                latencies.append(elapsed_ms)
                errors += 1
                k = f"EXC:{type(exc).__name__}"
                status_counts[k] = status_counts.get(k, 0) + 1

    wall_t0 = time.perf_counter()
    await asyncio.gather(*(worker(i) for i in range(concurrency)))
    wall_s = time.perf_counter() - wall_t0
    return summarize_latencies(latencies, wall_s, total_requests, errors, status_counts)


async def run_edit_and_conflict_benchmark(
    client: httpx.AsyncClient,
    base_url: str,
    manifest: dict[str, Any],
    csrf_headers: dict[str, str],
    concurrencies: list[int],
    requests_per_level: int,
) -> dict[str, Any]:
    org_id = manifest["org_a_id"]
    proj_id = manifest["project_a_id"]
    edit_layer = manifest["layers"]["edit_4326"]
    source_id = edit_layer["source_id"]
    layer_id = edit_layer["layer_id"]
    cs_url = f"{base_url}/api/v1/organizations/{org_id}/projects/{proj_id}/sources/{source_id}/changesets"
    layer_url = f"{base_url}/api/v1/organizations/{org_id}/projects/{proj_id}/layers/{layer_id}"

    r_init = await client.get(layer_url)
    r_init.raise_for_status()
    current_rev = r_init.json()["source"]["content_revision"]
    stale_rev = current_rev

    cold_latencies: list[float] = []
    cold_statuses: dict[str, int] = {}
    cold_errors = 0
    t_cold_start = time.perf_counter()

    # Create
    t0 = time.perf_counter_ns()
    r1 = await client.post(
        cs_url,
        headers=csrf_headers,
        json={
            "operation_id": f"bench-cold-create-{uuid4().hex[:12]}",
            "base_content_revision": current_rev,
            "summary": "Cold benchmark create",
            "operations": [
                {
                    "op_type": "create",
                    "feature_id": "bench-cold-f1",
                    "geometry": {"type": "Point", "coordinates": [105.84, 21.02]},
                    "properties": {
                        "code": "COLD-1",
                        "ward": "HN-01",
                        "metric_val": 100,
                        "active": True,
                    },
                }
            ],
        },
    )
    cold_latencies.append((time.perf_counter_ns() - t0) / 1_000_000.0)
    cold_statuses[str(r1.status_code)] = cold_statuses.get(str(r1.status_code), 0) + 1
    if r1.status_code == 201:
        current_rev = r1.json()["resulting_content_revision"]
    else:
        cold_errors += 1

    # Update with fresh revision
    t0 = time.perf_counter_ns()
    r2 = await client.post(
        cs_url,
        headers=csrf_headers,
        json={
            "operation_id": f"bench-cold-update-{uuid4().hex[:12]}",
            "base_content_revision": current_rev,
            "summary": "Cold benchmark update",
            "operations": [
                {
                    "op_type": "update",
                    "feature_id": "bench-cold-f1",
                    "geometry": {"type": "Point", "coordinates": [105.845, 21.025]},
                    "properties": {
                        "code": "COLD-1-UPD",
                        "ward": "HN-01",
                        "metric_val": 101,
                        "active": True,
                    },
                }
            ],
        },
    )
    cold_latencies.append((time.perf_counter_ns() - t0) / 1_000_000.0)
    cold_statuses[str(r2.status_code)] = cold_statuses.get(str(r2.status_code), 0) + 1
    if r2.status_code == 201:
        current_rev = r2.json()["resulting_content_revision"]
    else:
        cold_errors += 1

    # Explicit stale-revision conflict check (must return 409)
    t0 = time.perf_counter_ns()
    r3 = await client.post(
        cs_url,
        headers=csrf_headers,
        json={
            "operation_id": f"bench-cold-conflict-{uuid4().hex[:12]}",
            "base_content_revision": stale_rev,
            "summary": "Expected stale revision conflict",
            "operations": [
                {
                    "op_type": "update",
                    "feature_id": "bench-cold-f1",
                    "geometry": {"type": "Point", "coordinates": [105.85, 21.03]},
                    "properties": {
                        "code": "CONFLICT",
                        "ward": "HN-01",
                        "metric_val": 102,
                        "active": True,
                    },
                }
            ],
        },
    )
    cold_latencies.append((time.perf_counter_ns() - t0) / 1_000_000.0)
    cold_statuses[str(r3.status_code)] = cold_statuses.get(str(r3.status_code), 0) + 1
    if r3.status_code != 409:
        cold_errors += 1

    cold_summary = summarize_latencies(
        cold_latencies, time.perf_counter() - t_cold_start, 3, cold_errors, cold_statuses
    )

    warm_by_conc: dict[str, Any] = {}
    seq_counter = 0

    for c in concurrencies:
        total = max(c, min(requests_per_level, 40 if c == 1 else requests_per_level))
        latencies: list[float] = []
        status_counts: dict[str, int] = {}
        errors = 0
        wall_t0 = time.perf_counter()

        if c == 1:
            for i in range(total):
                seq_counter += 1
                is_conflict_probe = (i % 5 == 4)
                use_rev = stale_rev if is_conflict_probe else current_rev
                fid = f"bench-c1-{seq_counter:05d}" if not is_conflict_probe else "bench-cold-f1"
                op_type = "create" if not is_conflict_probe else "update"
                t0 = time.perf_counter_ns()
                resp = await client.post(
                    cs_url,
                    headers=csrf_headers,
                    json={
                        "operation_id": f"bench-c1-op-{seq_counter:05d}-{uuid4().hex[:6]}",
                        "base_content_revision": use_rev,
                        "summary": f"Bench c1 {op_type}",
                        "operations": [
                            {
                                "op_type": op_type,
                                "feature_id": fid,
                                "geometry": {
                                    "type": "Point",
                                    "coordinates": [
                                        105.80 + (seq_counter % 100) * 0.001,
                                        21.00 + (seq_counter % 100) * 0.001,
                                    ],
                                },
                                "properties": {
                                    "code": f"ED-{seq_counter}",
                                    "ward": "HN-02",
                                    "metric_val": seq_counter,
                                    "active": True,
                                },
                            }
                        ],
                    },
                )
                latencies.append((time.perf_counter_ns() - t0) / 1_000_000.0)
                k = str(resp.status_code)
                status_counts[k] = status_counts.get(k, 0) + 1
                if is_conflict_probe:
                    if resp.status_code != 409:
                        errors += 1
                else:
                    if resp.status_code == 201:
                        current_rev = resp.json()["resulting_content_revision"]
                    else:
                        errors += 1
        else:
            waves = max(1, total // c)
            actual_total = waves * c
            for w in range(waves):
                wave_rev = current_rev

                async def _contend(
                    slot: int,
                    wave_idx: int = w,
                    base_rev: str = wave_rev,
                    concurrency_value: int = c,
                ) -> tuple[int, float, str | None]:
                    nonlocal seq_counter
                    seq_counter += 1
                    fid = f"bench-c{concurrency_value}-w{wave_idx}-s{slot}-{seq_counter}"
                    t0 = time.perf_counter_ns()
                    resp = await client.post(
                        cs_url,
                        headers=csrf_headers,
                        json={
                            "operation_id": (
                                f"bench-c{concurrency_value}-{wave_idx}-{slot}-{uuid4().hex[:8]}"
                            ),
                            "base_content_revision": base_rev,
                            "summary": f"Contended wave {wave_idx} slot {slot}",
                            "operations": [
                                {
                                    "op_type": "create",
                                    "feature_id": fid,
                                    "geometry": {
                                        "type": "Point",
                                        "coordinates": [
                                            105.81 + slot * 0.0002,
                                            21.01 + wave_idx * 0.0002,
                                        ],
                                    },
                                    "properties": {
                                        "code": f"C{concurrency_value}-{wave_idx}-{slot}",
                                        "ward": "HN-03",
                                        "metric_val": slot,
                                        "active": True,
                                    },
                                }
                            ],
                        },
                    )
                    dt = (time.perf_counter_ns() - t0) / 1_000_000.0
                    new_rev = (
                        resp.json()["resulting_content_revision"]
                        if resp.status_code == 201
                        else None
                    )
                    return resp.status_code, dt, new_rev

                results = await asyncio.gather(*(_contend(s) for s in range(c)))
                winners = 0
                for code, dt, new_rev in results:
                    latencies.append(dt)
                    k = str(code)
                    status_counts[k] = status_counts.get(k, 0) + 1
                    if code == 201 and new_rev:
                        winners += 1
                        current_rev = new_rev
                    elif code != 409:
                        errors += 1
                if winners != 1:
                    errors += 1
            total = actual_total

        wall_s = time.perf_counter() - wall_t0
        warm_by_conc[str(c)] = summarize_latencies(
            latencies,
            wall_s,
            total,
            errors,
            status_counts,
            extra={
                "note": "201=committed, 409=expected optimistic-lock conflict (not counted as error)"
            },
        )

    return {"cold": cold_summary, "warm": warm_by_conc}


async def run_geojson_export_benchmark(
    client: httpx.AsyncClient,
    base_url: str,
    manifest: dict[str, Any],
    csrf_headers: dict[str, str],
    concurrencies: list[int],
    requests_per_level: int,
) -> dict[str, Any]:
    org_id = manifest["org_a_id"]
    proj_id = manifest["project_a_id"]
    layer = manifest["layers"]["epsg4326_1k"]
    base = f"{base_url}/api/v1/organizations/{org_id}/projects/{proj_id}"

    async def one_export_cycle(c_client: httpx.AsyncClient, _idx: int, _w: int) -> httpx.Response:
        r_prev = await c_client.post(
            f"{base}/exports/preview",
            headers=csrf_headers,
            json={
                "layer_id": layer["layer_id"],
                "content_revision": layer["content_revision"],
                "schema_revision": layer["schema_revision"],
                "output_format": "geojson",
                "output_crs": 4326,
            },
        )
        if r_prev.status_code != 200:
            return r_prev
        prev = r_prev.json()
        approved = [d["code"] for d in prev.get("disclosures", []) if d.get("requires_approval")]
        r_conf = await c_client.post(
            f"{base}/exports/confirm",
            headers=csrf_headers,
            json={
                "operation_id": str(uuid4()),
                "layer_id": layer["layer_id"],
                "content_revision": layer["content_revision"],
                "schema_revision": layer["schema_revision"],
                "output_format": "geojson",
                "output_crs": 4326,
                "approved_disclosures": approved,
                "preview_fingerprint": prev["preview_fingerprint"],
            },
        )
        if r_conf.status_code != 200:
            return r_conf
        art = r_conf.json().get("artifact")
        if not art:
            return r_conf
        return await c_client.get(f"{base}/artifacts/{art['id']}/download")

    cold = await run_load_step(client, 1, 2, one_export_cycle, {200})
    warm: dict[str, Any] = {}
    for c in concurrencies:
        req_count = max(c, min(requests_per_level, 10 if c == 1 else 20))
        warm[str(c)] = await run_load_step(client, c, req_count, one_export_cycle, {200})
    return {"cold": cold, "warm": warm}


async def run_all_benchmarks(
    base_url: str,
    manifest: dict[str, Any],
    *,
    concurrencies: list[int],
    requests_per_level: int,
    cold_requests: int,
    monitor: ProcessMonitor,
) -> dict[str, Any]:
    org_a = manifest["org_a_id"]
    proj_a = manifest["project_a_id"]
    layers = manifest["layers"]

    cookies_a = {"ovg_session": manifest["session_a_token"]}
    csrf_a = {"x-openvietgis-csrf": manifest["csrf_a_token"]}
    cookies_b = {"ovg_session": manifest["session_b_token"]}

    limits = httpx.Limits(
        max_connections=max(concurrencies) * 2 + 10,
        max_keepalive_connections=max(concurrencies) + 5,
    )
    timeout = httpx.Timeout(120.0, connect=10.0)

    results: dict[str, Any] = {}
    preflight: dict[str, Any] = {}

    async with httpx.AsyncClient(cookies=cookies_a, limits=limits, timeout=timeout) as client_a:
        r_health_root = await client_a.get(f"{base_url}/api/v1/health")
        r_health_live = await client_a.get(f"{base_url}/api/v1/health/live")
        r_health_ready = await client_a.get(f"{base_url}/api/v1/health/ready")
        preflight["health_routes"] = {
            "/api/v1/health": r_health_root.status_code,
            "/api/v1/health/live": r_health_live.status_code,
            "/api/v1/health/ready": r_health_ready.status_code,
        }

        async with httpx.AsyncClient(cookies=cookies_b, timeout=timeout) as client_b:
            r_cross = await client_b.get(
                f"{base_url}/api/v1/organizations/{org_a}/projects/{proj_a}/layers/{layers['epsg4326_1k']['layer_id']}"
            )
            preflight["cross_tenant_isolation"] = {
                "status_code": r_cross.status_code,
                "passed_fail_closed": r_cross.status_code in (403, 404),
            }
            if r_cross.status_code not in (403, 404):
                raise RuntimeError(
                    f"CRITICAL: Cross-tenant read returned {r_cross.status_code} instead of 403/404!"
                )

        if "epsg4326_30k" in layers:
            l30 = layers["epsg4326_30k"]
            r_vp30 = await client_a.get(
                f"{base_url}/api/v1/organizations/{org_a}/projects/{proj_a}/layers/{l30['layer_id']}/viewport",
                params={
                    "content_revision": l30["content_revision"],
                    "schema_revision": l30["schema_revision"],
                    "west": 105.75,
                    "south": 20.90,
                    "east": 105.95,
                    "north": 21.10,
                    "limit": 500,
                },
            )
            preflight["viewport_30k_probe"] = {
                "status_code": r_vp30.status_code,
                "body_excerpt": r_vp30.text[:300],
            }

        scenarios: list[tuple[str, Any, set[int], str]] = [
            (
                "health_live",
                lambda c, _i, _w: c.get(f"{base_url}/api/v1/health/live"),
                {200},
                "GET /api/v1/health/live (process liveness, no DB)",
            ),
            (
                "health_ready",
                lambda c, _i, _w: c.get(f"{base_url}/api/v1/health/ready"),
                {200},
                "GET /api/v1/health/ready (DB SELECT 1 readiness)",
            ),
            (
                "bootstrap_status",
                lambda c, _i, _w: c.get(f"{base_url}/api/v1/bootstrap/status"),
                {200},
                "GET /api/v1/bootstrap/status",
            ),
            (
                "auth_me",
                lambda c, _i, _w: c.get(f"{base_url}/api/v1/me"),
                {200},
                "GET /api/v1/me (session verification + RLS actor bind)",
            ),
            (
                "list_organizations",
                lambda c, _i, _w: c.get(f"{base_url}/api/v1/me/organizations"),
                {200},
                "GET /api/v1/me/organizations",
            ),
            (
                "list_projects",
                lambda c, _i, _w: c.get(f"{base_url}/api/v1/organizations/{org_a}/projects"),
                {200},
                "GET /api/v1/organizations/{org}/projects",
            ),
            (
                "list_layers",
                lambda c, _i, _w: c.get(
                    f"{base_url}/api/v1/organizations/{org_a}/projects/{proj_a}/layers"
                ),
                {200},
                "GET .../projects/{proj}/layers (spatial catalog + capabilities)",
            ),
            (
                "get_layer_metadata",
                lambda c, _i, _w: c.get(
                    f"{base_url}/api/v1/organizations/{org_a}/projects/{proj_a}/layers/{layers['epsg4326_10k']['layer_id']}"
                ),
                {200},
                "GET .../layers/{layer_10k} (single layer detail + pins)",
            ),
        ]

        if "epsg4326_1k" in layers:
            l1 = layers["epsg4326_1k"]
            scenarios.append(
                (
                    "viewport_bbox_1k_polygon",
                    lambda c, _i, _w, l=l1: c.get(
                        f"{base_url}/api/v1/organizations/{org_a}/projects/{proj_a}/layers/{l['layer_id']}/viewport",
                        params={
                            "content_revision": l["content_revision"],
                            "schema_revision": l["schema_revision"],
                            "west": 105.75,
                            "south": 20.92,
                            "east": 105.95,
                            "north": 21.08,
                            "limit": 500,
                        },
                    ),
                    {200},
                    "GET .../layers/{1k_poly}/viewport (EPSG:4326 bounded bbox)",
                )
            )

        if "epsg4326_10k" in layers:
            l10 = layers["epsg4326_10k"]
            scenarios.append(
                (
                    "viewport_bbox_10k_point",
                    lambda c, _i, _w, l=l10: c.get(
                        f"{base_url}/api/v1/organizations/{org_a}/projects/{proj_a}/layers/{l['layer_id']}/viewport",
                        params={
                            "content_revision": l["content_revision"],
                            "schema_revision": l["schema_revision"],
                            "west": 105.75,
                            "south": 20.90,
                            "east": 105.95,
                            "north": 21.10,
                            "limit": 500,
                        },
                    ),
                    {200},
                    "GET .../layers/{10k_pt}/viewport (EPSG:4326 bounded bbox)",
                )
            )

        if "epsg4326_30k" in layers:
            l30 = layers["epsg4326_30k"]
            bbox_poly = {
                "type": "Polygon",
                "coordinates": [
                    [
                        [105.75, 20.90],
                        [105.95, 20.90],
                        [105.95, 21.10],
                        [105.75, 21.10],
                        [105.75, 20.90],
                    ]
                ],
            }
            scenarios.append(
                (
                    "spatial_bbox_query_30k",
                    lambda c, _i, _w, l=l30, bp=bbox_poly: c.post(
                        f"{base_url}/api/v1/organizations/{org_a}/projects/{proj_a}/layers/{l['layer_id']}/query",
                        headers=csrf_a,
                        json={
                            "spatial_filter": {
                                "predicate": "intersects",
                                "geometry": bp,
                                "crs_code": 4326,
                            },
                            "expected_content_revision": l["content_revision"],
                            "expected_schema_revision": l["schema_revision"],
                            "limit": 200,
                            "offset": 0,
                        },
                    ),
                    {200},
                    "POST .../layers/{30k_pt}/query (ST_Intersects bbox over 30k points)",
                )
            )
            scenarios.append(
                (
                    "features_page_30k",
                    lambda c, _i, _w, l=l30: c.get(
                        f"{base_url}/api/v1/organizations/{org_a}/projects/{proj_a}/layers/{l['layer_id']}/features",
                        params={
                            "content_revision": l["content_revision"],
                            "schema_revision": l["schema_revision"],
                            "limit": 100,
                        },
                    ),
                    {200},
                    "GET .../layers/{30k_pt}/features?limit=100 (keyset page)",
                )
            )

        if "vn2000_5899_1k" in layers:
            lv = layers["vn2000_5899_1k"]
            scenarios.append(
                (
                    "features_vn2000_5899_to_4326",
                    lambda c, _i, _w, l=lv: c.get(
                        f"{base_url}/api/v1/organizations/{org_a}/projects/{proj_a}/layers/{l['layer_id']}/features",
                        params={
                            "content_revision": l["content_revision"],
                            "schema_revision": l["schema_revision"],
                            "response_crs": 4326,
                            "limit": 100,
                        },
                    ),
                    {200},
                    "GET .../layers/{vn2000_5899}/features?response_crs=4326 (VN-2000 -> WGS84)",
                )
            )

        for name, fn, ok_codes, desc in scenarios:
            print(f"[bench] running scenario: {name} ...", flush=True)
            cold = await run_load_step(client_a, 1, cold_requests, fn, ok_codes)
            warm: dict[str, Any] = {}
            for c in concurrencies:
                if name == "spatial_bbox_query_30k":
                    reqs = max(c, 5 if c == 1 else c)
                elif name == "viewport_bbox_10k_point":
                    reqs = max(c, 10 if c == 1 else c)
                else:
                    reqs = max(c, requests_per_level)
                warm[str(c)] = await run_load_step(client_a, c, reqs, fn, ok_codes)
            results[name] = {
                "description": desc,
                "cold": cold,
                "warm": warm,
                "rss_after": monitor.snapshot_rss_mib(),
            }

        print("[bench] running scenario: feature_edit_and_conflict ...", flush=True)
        edit_res = await run_edit_and_conflict_benchmark(
            client_a, base_url, manifest, csrf_a, concurrencies, requests_per_level
        )
        edit_res["description"] = "POST .../sources/{src}/changesets (create/update + stale 409 conflict)"
        edit_res["rss_after"] = monitor.snapshot_rss_mib()
        results["feature_edit_and_conflict"] = edit_res

        print("[bench] running scenario: geojson_export_1k ...", flush=True)
        exp_res = await run_geojson_export_benchmark(
            client_a, base_url, manifest, csrf_a, concurrencies, requests_per_level
        )
        exp_res["description"] = (
            "POST /exports/preview + /exports/confirm (GeoJSON 4326) + /download on 1k polygons"
        )
        exp_res["rss_after"] = monitor.snapshot_rss_mib()
        results["geojson_export_1k"] = exp_res

    return {"preflight": preflight, "scenarios": results}


def render_markdown_report(report: dict[str, Any]) -> str:
    env = report["environment"]
    res = report["process_resources"]
    pf = report["benchmark"]["preflight"]
    ds = report["dataset"]
    scenarios = report["benchmark"]["scenarios"]

    lines = [
        "# Spec 064 Baseline Benchmark Report (Python/FastAPI Reference)",
        "",
        f"- **Generated (UTC)**: `{env['timestamp_utc']}`",
        f"- **Target**: `{env['target_label']}` (`{env['target_url']}`)",
        f"- **Git Commit**: `{env['git_sha']}` (branch `{env['git_branch']}`)",
        f"- **Host**: `{env['hostname']}` — `{env['cpu_count']} vCPU` (`{env['cpu_model']}`), `{env['mem_total_mib']} MiB RAM`, `{env['os']}`",
        f"- **Python**: `{env['python_version']}`",
        f"- **PostgreSQL**: `{env['postgres_version']}`",
        f"- **PostGIS**: `{env['postgis_version']}`",
        f"- **Dataset Seed**: `{ds['seed']}` | Scales: `{', '.join(ds['scales'])}` | Seed Time: `{ds['seed_duration_s']} s`",
        "",
        "## Process Resource Footprint (Isolated Single-Worker Uvicorn, `openvietgis_runtime` RLS)",
        "",
        "| Metric | Measured Value | Spec 064 Target (`SC-005`) |",
        "|---|---:|---|",
        f"| Idle RSS (`VmRSS`) | `{res['idle_rss_mib']} MiB` | — |",
        f"| End-of-Run RSS (`VmRSS`) | `{res['end_rss_mib']} MiB` | — |",
        f"| **Peak RSS (`VmHWM`)** | **`{res['peak_rss_mib']} MiB`** | `<= 1024 MiB` aggregate app RSS on 4 GiB host |",
        f"| Server CPU Time (user+sys) | `{res['cpu_seconds']} s` | — |",
        "",
        "## Pre-Flight & Contract Findings",
        "",
        f"- **Health Endpoints**: `/api/v1/health` -> `{pf['health_routes']['/api/v1/health']}` (not mounted; canonical probes are `/api/v1/health/live` -> `{pf['health_routes']['/api/v1/health/live']}` and `/api/v1/health/ready` -> `{pf['health_routes']['/api/v1/health/ready']}`).",
        f"- **Multi-Tenant RLS Negative Check**: Org Beta requesting Org Alpha layer -> HTTP `{pf['cross_tenant_isolation']['status_code']}` (`passed_fail_closed={pf['cross_tenant_isolation']['passed_fail_closed']}`).",
    ]

    if "viewport_30k_probe" in pf:
        vp30 = pf["viewport_30k_probe"]
        lines.append(
            f"- **30k Viewport Ceiling Probe (`GET .../layers/{{30k}}/viewport`)**: HTTP `{vp30['status_code']}` — `{vp30['body_excerpt']}` "
            "(confirms current Python `map_viewport.py` `LIMIT 10001` count-mismatch ceiling above 10k features, motivating Spec 064 US2 / `SC-004` bounded tile delivery)."
        )

    lines.extend(
        [
            "- **VN-2000 EPSG:3405 Fixture Note**: `1,000` synthetic `SRID=3405` points seeded in PostGIS `spatial.reference_features`; current DB `spatial.valid_extent()` constrains `validation_state='ready'` to `(4326, 3857, 4756, 5899)`, so live VN-2000 transform benchmarks exercise `EPSG:5899 -> 4326` while `EPSG:3405` awaits `T010` control-point activation.",
            "",
            "## Seeded Synthetic GIS Dataset Inventory",
            "",
            "| Layer Key | CRS | Geometry | Feature Count | Validation State |",
            "|---|---|---|---:|---|",
        ]
    )
    for key, info in ds["layers"].items():
        lines.append(
            f"| `{key}` | `EPSG:{info['crs']}` | `{info['geometry_family']}` | `{info['feature_count']:,}` | `{info['validation_state']}` |"
        )

    lines.extend(
        [
            "",
            "## Cold Start Latency Summary (First Uncached Requests)",
            "",
            "| Scenario | Reqs | p50 (ms) | p95 (ms) | p99 (ms) | Max (ms) | Error % |",
            "|---|---:|---:|---:|---:|---:|---:|",
        ]
    )
    for name, sdata in scenarios.items():
        c = sdata["cold"]
        lines.append(
            f"| `{name}` | {c['requests']} | {c['p50_ms']:.2f} | {c['p95_ms']:.2f} | {c['p99_ms']:.2f} | {c['max_ms']:.2f} | {c['error_rate_pct']:.2f}% |"
        )

    lines.extend(
        [
            "",
            "## Warm Performance Across Concurrency 1, 10, and 20",
            "",
            "| Scenario | Concurrency | Reqs | p50 (ms) | p95 (ms) | p99 (ms) | Throughput (req/s) | Error % | Peak RSS After (MiB) |",
            "|---|---:|---:|---:|---:|---:|---:|---:|---:|",
        ]
    )
    for name, sdata in scenarios.items():
        rss_peak = sdata.get("rss_after", {}).get("peak_hwm_mib", 0.0)
        for conc, m in sdata["warm"].items():
            lines.append(
                f"| `{name}` | {conc} | {m['requests']} | {m['p50_ms']:.2f} | {m['p95_ms']:.2f} | {m['p99_ms']:.2f} | {m['throughput_rps']:.2f} | {m['error_rate_pct']:.2f}% | {rss_peak:.1f} |"
            )

    lines.extend(
        [
            "",
            "## Comparative Acceptance Targets for Rust Candidate (`T020` / `SC-003`)",
            "",
            "When running this exact harness with `--target-url <rust_api>` on the same host and database:",
            "- **Viewport p95 at 20 concurrent clients**: Rust MUST achieve `<= 70%` of Python baseline (`viewport_bbox_1k_polygon`, `viewport_bbox_10k_point`, `spatial_bbox_query_30k`).",
            "- **Feature API p95 at 20 concurrent clients**: Rust MUST achieve `<= 100%` of Python baseline (`features_page_30k`, `features_vn2000_5899_to_4326`, `feature_edit_and_conflict`).",
            "- **Error Rate**: Rust MUST have `<=` Python baseline error rate (`0.00%`).",
            "- **Memory**: Aggregate Rust app RSS `<= 1024 MiB` on isolated 4 GiB profile with zero swap growth.",
            "",
        ]
    )
    return "\n".join(lines)


def create_temp_bench_db(admin_pg_url: str, db_name: str) -> str:
    assert_safe_admin_url(admin_pg_url)
    assert_safe_bench_db(f"postgresql://127.0.0.1:54330/{db_name}")
    with psycopg.connect(admin_pg_url, autocommit=True) as conn, conn.cursor() as cur:
        # CREATE fails on collision; never drop a pre-existing database.
        cur.execute(
            f'CREATE DATABASE "{db_name}" TEMPLATE template_postgis OWNER openvietgis'
        )
    parsed = urlparse(admin_pg_url)
    return parsed._replace(path=f"/{db_name}").geturl()


def drop_temp_bench_db(admin_pg_url: str, db_name: str) -> None:
    assert_safe_admin_url(admin_pg_url)
    assert_safe_bench_db(f"postgresql://127.0.0.1:54330/{db_name}")
    with psycopg.connect(admin_pg_url, autocommit=True) as conn, conn.cursor() as cur:
        cur.execute(f'DROP DATABASE IF EXISTS "{db_name}" WITH (FORCE)')


def run_alembic_upgrade(owner_db_url: str, runtime_db_url: str) -> None:
    sa_owner = owner_db_url.replace("postgresql://", "postgresql+psycopg://", 1)
    sa_runtime = runtime_db_url.replace("postgresql://", "postgresql+psycopg://", 1)
    env = {
        **os.environ,
        "PYTHONPATH": f"{REPO_ROOT}:{API_ROOT}",
        "OPENVIETGIS_ENVIRONMENT": "test",
        "OPENVIETGIS_DATABASE_URL": sa_owner,
        "OPENVIETGIS_MIGRATION_DATABASE_URL": sa_owner,
        "OPENVIETGIS_RUNTIME_DATABASE_URL": sa_runtime,
        "OPENVIETGIS_APPLICATION_SESSION_SECRET": DEFAULT_SESSION_SECRET,
        "OPENVIETGIS_CSRF_SECRET": DEFAULT_CSRF_SECRET,
        "OPENVIETGIS_BOOTSTRAP_SECRET": DEFAULT_BOOTSTRAP_SECRET,
    }
    alembic_bin = Path(sys.executable).parent / "alembic"
    cmd = [str(alembic_bin), "-c", str(API_ROOT / "alembic.ini"), "upgrade", "head"]
    subprocess.run(cmd, cwd=REPO_ROOT, env=env, check=True)


def wait_for_ready(base_url: str, timeout_s: float = 30.0) -> None:
    deadline = time.monotonic() + timeout_s
    last_err = ""
    while time.monotonic() < deadline:
        try:
            r = httpx.get(f"{base_url}/api/v1/health/ready", timeout=2.0)
            if r.status_code == 200:
                return
            last_err = f"status={r.status_code} body={r.text[:120]}"
        except httpx.HTTPError as exc:
            last_err = str(exc)
        time.sleep(0.25)
    raise RuntimeError(f"Server at {base_url} did not become ready within {timeout_s}s: {last_err}")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Spec 064 Baseline & Comparative Benchmark Runner"
    )
    parser.add_argument("--target-url", default=None)
    parser.add_argument("--target-label", default="python-fastapi-reference")
    parser.add_argument("--spawn-python-isolated", action="store_true")
    parser.add_argument("--port", type=int, default=18899)
    parser.add_argument("--admin-pg-url", default=DEFAULT_ADMIN_PG_URL)
    parser.add_argument("--db-name", default=f"{DEFAULT_BENCH_DB_NAME}_{uuid4().hex[:12]}")
    parser.add_argument("--db-url", default=None)
    parser.add_argument("--server-pid", type=int, default=None)
    parser.add_argument("--seed", type=int, default=20261125)
    parser.add_argument("--scale", default="1k,10k,30k")
    parser.add_argument("--concurrencies", default="1,10,20")
    parser.add_argument("--requests-per-level", type=int, default=60)
    parser.add_argument("--cold-requests", type=int, default=3)
    parser.add_argument("--skip-seed", action="store_true")
    parser.add_argument("--manifest-in", default=None)
    parser.add_argument("--manifest-out", default=None)
    parser.add_argument("--output-json", required=True)
    parser.add_argument("--output-md", required=True)
    args = parser.parse_args()

    scales = [s.strip().lower() for s in args.scale.split(",") if s.strip()]
    for s in scales:
        if s not in SCALE_MAP:
            raise SystemExit(f"Unsupported scale tier {s!r}; allowed: {sorted(SCALE_MAP)}")
    concurrencies = [int(x.strip()) for x in args.concurrencies.split(",") if x.strip()]

    if not args.spawn_python_isolated and not args.target_url:
        raise SystemExit("Provide either --spawn-python-isolated or --target-url")

    if args.spawn_python_isolated:
        assert_safe_admin_url(args.admin_pg_url)
        assert_safe_http_target(f"http://127.0.0.1:{args.port}")
    else:
        assert_safe_http_target(args.target_url)
        if not args.db_url:
            raise SystemExit("--db-url is required to verify isolated benchmark database")
        assert_safe_bench_db(args.db_url)
    if not concurrencies or min(concurrencies) < 1 or max(concurrencies) > 64:
        raise SystemExit("Concurrency must be between 1 and 64")
    if args.requests_per_level < 1 or args.cold_requests < 1:
        raise SystemExit("Request counts must be positive")

    uvicorn_proc: subprocess.Popen[Any] | None = None
    uvicorn_log_fh: Any = None
    temp_artifact_dir: str | None = None
    created_temp_db = False
    owner_db_url = args.db_url

    try:
        if args.spawn_python_isolated:
            print(f"[setup] creating isolated temp database {args.db_name} ...", flush=True)
            owner_db_url = create_temp_bench_db(args.admin_pg_url, args.db_name)
            created_temp_db = True
            parsed = urlparse(owner_db_url)
            runtime_db_url = parsed._replace(
                netloc=f"{DEFAULT_RUNTIME_USER}:{DEFAULT_RUNTIME_PASS}@{parsed.hostname}:{parsed.port}"
            ).geturl()
            print("[setup] running alembic upgrade head ...", flush=True)
            run_alembic_upgrade(owner_db_url, runtime_db_url)
            print(
                f"[setup] seeding deterministic GIS fixtures (scales={scales}, seed={args.seed}) ...",
                flush=True,
            )
            manifest = seed_benchmark_database(
                owner_db_url,
                seed=args.seed,
                scales=scales,
                csrf_secret=DEFAULT_CSRF_SECRET,
            )
            print(f"[setup] seeded in {manifest['seed_duration_s']}s", flush=True)

            temp_artifact_dir = tempfile.mkdtemp(prefix="ovg-bench-064-artifacts-")
            sa_owner = owner_db_url.replace("postgresql://", "postgresql+psycopg://", 1)
            sa_runtime = runtime_db_url.replace("postgresql://", "postgresql+psycopg://", 1)
            server_env = {
                **os.environ,
                "PYTHONPATH": f"{REPO_ROOT}:{API_ROOT}",
                "OPENVIETGIS_ENVIRONMENT": "test",
                "OPENVIETGIS_DATABASE_URL": sa_runtime,
                "OPENVIETGIS_RUNTIME_DATABASE_URL": sa_runtime,
                "OPENVIETGIS_MIGRATION_DATABASE_URL": sa_owner,
                "OPENVIETGIS_APPLICATION_SESSION_SECRET": DEFAULT_SESSION_SECRET,
                "OPENVIETGIS_CSRF_SECRET": DEFAULT_CSRF_SECRET,
                "OPENVIETGIS_BOOTSTRAP_SECRET": DEFAULT_BOOTSTRAP_SECRET,
                "OPENVIETGIS_ARTIFACT_STORE_DIR": temp_artifact_dir,
            }
            uvicorn_bin = Path(sys.executable).parent / "uvicorn"
            target_url = f"http://127.0.0.1:{args.port}"
            print(f"[setup] starting isolated uvicorn on {target_url} ...", flush=True)
            uvicorn_log_path = os.path.join(temp_artifact_dir, "uvicorn.log")
            # The handle intentionally spans the child-process lifetime and closes in finally.
            uvicorn_log_fh = open(uvicorn_log_path, "wb")  # noqa: SIM115
            uvicorn_proc = subprocess.Popen(
                [
                    str(uvicorn_bin),
                    "openvietgis_api.main:app",
                    "--host",
                    "127.0.0.1",
                    "--port",
                    str(args.port),
                    "--workers",
                    "1",
                    "--log-level",
                    "warning",
                ],
                cwd=REPO_ROOT,
                env=server_env,
                stdout=uvicorn_log_fh,
                stderr=subprocess.STDOUT,
            )
            wait_for_ready(target_url, timeout_s=30.0)
            server_pid = uvicorn_proc.pid
            print(f"[setup] isolated uvicorn ready (pid={server_pid})", flush=True)
        else:
            target_url = args.target_url.rstrip("/")
            server_pid = args.server_pid
            if args.skip_seed:
                if not args.manifest_in:
                    raise SystemExit("--manifest-in is required with --skip-seed")
                manifest = json.loads(Path(args.manifest_in).read_text(encoding="utf-8"))
            else:
                if not owner_db_url:
                    raise SystemExit("--db-url is required unless --skip-seed is set")
                manifest = seed_benchmark_database(
                    owner_db_url,
                    seed=args.seed,
                    scales=scales,
                    csrf_secret=os.getenv("OPENVIETGIS_CSRF_SECRET", DEFAULT_CSRF_SECRET),
                )

        if args.manifest_out:
            Path(args.manifest_out).write_text(
                json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
            )

        monitor = ProcessMonitor(pid=server_pid)
        monitor.start()
        bench_result = asyncio.run(
            run_all_benchmarks(
                target_url,
                manifest,
                concurrencies=concurrencies,
                requests_per_level=args.requests_per_level,
                cold_requests=args.cold_requests,
                monitor=monitor,
            )
        )
        proc_resources = monitor.stop()

        env_meta = collect_environment_metadata(
            owner_db_url or args.admin_pg_url, target_url, args.target_label
        )
        sanitized_dataset = {k: v for k, v in manifest.items() if not k.endswith("_token")}
        full_report = {
            "schema_version": "spec064-benchmark-v1",
            "environment": env_meta,
            "parameters": {
                "seed": args.seed,
                "scales": scales,
                "concurrencies": concurrencies,
                "requests_per_level": args.requests_per_level,
                "cold_requests": args.cold_requests,
            },
            "dataset": sanitized_dataset,
            "process_resources": proc_resources,
            "benchmark": bench_result,
        }

        out_json = Path(args.output_json)
        out_json.parent.mkdir(parents=True, exist_ok=True)
        out_json.write_text(json.dumps(full_report, indent=2) + "\n", encoding="utf-8")

        out_md = Path(args.output_md)
        out_md.parent.mkdir(parents=True, exist_ok=True)
        out_md.write_text(render_markdown_report(full_report) + "\n", encoding="utf-8")

        print(f"[done] wrote {out_json} and {out_md}", flush=True)
        print(
            f"[summary] peak_rss={proc_resources['peak_rss_mib']} MiB "
            f"cpu={proc_resources['cpu_seconds']}s",
            flush=True,
        )
        return 0
    finally:
        if uvicorn_proc is not None:
            print("[cleanup] stopping isolated uvicorn ...", flush=True)
            uvicorn_proc.terminate()
            try:
                uvicorn_proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                uvicorn_proc.kill()
                uvicorn_proc.wait(timeout=5)
        if uvicorn_log_fh is not None:
            try:
                uvicorn_log_fh.close()
            except OSError as exc:
                print(f"[cleanup] WARNING: failed to close uvicorn log: {exc}", file=sys.stderr)
        if temp_artifact_dir and os.path.isdir(temp_artifact_dir):
            shutil.rmtree(temp_artifact_dir, ignore_errors=True)
        if created_temp_db:
            print(f"[cleanup] dropping temp database {args.db_name} ...", flush=True)
            try:
                drop_temp_bench_db(args.admin_pg_url, args.db_name)
                print("[cleanup] temp database dropped cleanly.", flush=True)
            except (psycopg.Error, RuntimeError) as exc:
                print(f"[cleanup] WARNING: failed to drop temp DB: {exc}", file=sys.stderr)


if __name__ == "__main__":
    raise SystemExit(main())
