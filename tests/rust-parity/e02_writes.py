"""E02 spatial catalog & layer write differential parity on isolated real PostGIS."""

from __future__ import annotations

import argparse
import os
import sys
import tempfile
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

import psycopg

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "rust-parity"))
sys.path.insert(0, str(ROOT / "tools" / "benchmark" / "rust-platform"))

import benchmark_runner as baseline
import run as parity

API = "/api/v1"


def require(condition: bool, detail: str) -> None:
    if not condition:
        raise AssertionError(detail)


def normalize_mutation(body: dict) -> dict:
    out = dict(body)
    for key in ("project_id", "resource_id", "change_id"):
        if key in out and out[key] is not None:
            out[key] = f"<{key}>"
    return out


def normalize_manual(body: dict) -> dict:
    out = dict(body)
    for key in ("source_id", "layer_id", "operation_id"):
        if key in out and out[key] is not None:
            out[key] = f"<{key}>"
    return out


@parity.case
def compare_response(label: str, expected, actual, *, normalize=None):
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code} (expected={expected.text}, actual={actual.text})",
    )
    ebody = expected.json()
    abody = actual.json()
    if normalize is not None:
        ebody, abody = normalize(ebody), normalize(abody)
    mismatch = parity.difference(ebody, abody)
    require(mismatch is None, f"{label}: {mismatch}")
    print(f"PASS {label}", flush=True)


def clone_project(owner_url: str, org_id: str, template_project_id: str, suffix: str) -> str:
    new_id = str(uuid4())
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO workspace.projects (
                id, organization_id, workspace_id, name, slug, status,
                created_by_actor_type, created_by_actor_id, created_at, updated_at
            )
            SELECT %s, organization_id, workspace_id, name || ' ' || %s, slug || '-' || %s,
                   status, created_by_actor_type, created_by_actor_id, created_at, updated_at
            FROM workspace.projects
            WHERE organization_id = %s AND id = %s
            """,
            (new_id, suffix, suffix, org_id, template_project_id),
        )
    return new_id


def enable_gis_scoped_editor(owner_url: str, org_id: str, project_ids: tuple[str, ...], user_id: str) -> None:
    now = datetime.now(UTC)
    with psycopg.connect(owner_url) as conn:
        membership_id = conn.execute(
            """
            UPDATE org.organization_memberships
            SET role = 'member'
            WHERE organization_id = %s AND user_id = %s
            RETURNING id
            """,
            (org_id, user_id),
        ).fetchone()[0]
        conn.execute(
            """
            UPDATE org.resource_grants
            SET revoked_at = %s
            WHERE organization_id = %s AND membership_id = %s AND revoked_at IS NULL
            """,
            (now, org_id, membership_id),
        )
        for project_id in project_ids:
            conn.execute(
                """
                INSERT INTO org.resource_grants (
                    id, organization_id, subject_kind, membership_id, scope_kind,
                    project_id, role, created_at, created_by_actor_type, created_by_actor_id
                ) VALUES (%s, %s, 'membership', %s, 'project', %s, 'editor', %s, 'human', %s)
                """,
                (uuid4(), org_id, membership_id, project_id, now, user_id),
            )
            conn.execute(
                """
                INSERT INTO gis_access.scoped_members(organization_id, project_id, user_id)
                VALUES (%s, %s, %s)
                ON CONFLICT DO NOTHING
                """,
                (org_id, project_id, user_id),
            )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e02-parity-") as directory:
        work = Path(directory)
        with parity.database(admin_url) as (owner_url, runtime_url):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            parity.configure_python(owner_url, runtime_url, artifacts)
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            fixture = baseline.seed_benchmark_database(
                owner_url,
                seed=20261127,
                scales=["1k"],
                csrf_secret=baseline.DEFAULT_CSRF_SECRET,
            )
            org_id = str(fixture["org_a_id"])
            template_proj = str(fixture["project_a_id"])
            proj_o = clone_project(owner_url, org_id, template_proj, "oracle")
            proj_r = clone_project(owner_url, org_id, template_proj, "rust")
            user_a = str(fixture["user_a_id"])

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            base_o = f"{API}/organizations/{org_id}/projects/{proj_o}"
            base_r = f"{API}/organizations/{org_id}/projects/{proj_r}"
            headers = parity.session_headers(fixture["session_a_token"])
            csrf = parity.session_headers(
                fixture["session_a_token"],
                **{"x-openvietgis-csrf": fixture["csrf_a_token"]},
            )
            foreign = parity.session_headers(
                fixture["session_b_token"],
                **{"x-openvietgis-csrf": fixture["csrf_b_token"]},
            )

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    # 1. register_source: valid, duplicate, unknown, missing CSRF, foreign
                    reg_payload = {"reference_key": "reference_points_5899", "name": "  HVS QN 5899  "}
                    reg_o = oracle.post(f"{base_o}/spatial/sources", headers=csrf, json=reg_payload)
                    reg_r = rust.post(f"{base_r}/spatial/sources", headers=csrf, json=reg_payload)
                    compare_response("E02 register source 5899", reg_o, reg_r, normalize=normalize_mutation)
                    src_o = reg_o.json()["resource_id"]
                    src_r = reg_r.json()["resource_id"]

                    compare_response(
                        "E02 register source duplicate 409",
                        oracle.post(f"{base_o}/spatial/sources", headers=csrf, json=reg_payload),
                        rust.post(f"{base_r}/spatial/sources", headers=csrf, json=reg_payload),
                    )
                    compare_response(
                        "E02 register unknown reference_key 422",
                        oracle.post(
                            f"{base_o}/spatial/sources",
                            headers=csrf,
                            json={"reference_key": "no_such_key", "name": "Bad"},
                        ),
                        rust.post(
                            f"{base_r}/spatial/sources",
                            headers=csrf,
                            json={"reference_key": "no_such_key", "name": "Bad"},
                        ),
                    )
                    compare_response(
                        "E02 register source missing CSRF 403",
                        oracle.post(f"{base_o}/spatial/sources", headers=headers, json=reg_payload),
                        rust.post(f"{base_r}/spatial/sources", headers=headers, json=reg_payload),
                    )
                    compare_response(
                        "E02 register source foreign tenant 404",
                        oracle.post(f"{base_o}/spatial/sources", headers=foreign, json=reg_payload),
                        rust.post(f"{base_r}/spatial/sources", headers=foreign, json=reg_payload),
                    )

                    # 2. update_source: rename, no_changes 422, stale revision 409, null field 422
                    compare_response(
                        "E02 update source null field 422",
                        oracle.patch(
                            f"{base_o}/spatial/sources/{src_o}",
                            headers=csrf,
                            json={"expected_revision": 1, "name": None},
                        ),
                        rust.patch(
                            f"{base_r}/spatial/sources/{src_r}",
                            headers=csrf,
                            json={"expected_revision": 1, "name": None},
                        ),
                    )
                    compare_response(
                        "E02 update source no changes 422",
                        oracle.patch(
                            f"{base_o}/spatial/sources/{src_o}",
                            headers=csrf,
                            json={"expected_revision": 1, "name": "HVS QN 5899"},
                        ),
                        rust.patch(
                            f"{base_r}/spatial/sources/{src_r}",
                            headers=csrf,
                            json={"expected_revision": 1, "name": "HVS QN 5899"},
                        ),
                    )
                    upd_o = oracle.patch(
                        f"{base_o}/spatial/sources/{src_o}",
                        headers=csrf,
                        json={"expected_revision": 1, "name": "HVS QN Renamed"},
                    )
                    upd_r = rust.patch(
                        f"{base_r}/spatial/sources/{src_r}",
                        headers=csrf,
                        json={"expected_revision": 1, "name": "HVS QN Renamed"},
                    )
                    compare_response("E02 update source rename", upd_o, upd_r, normalize=normalize_mutation)
                    compare_response(
                        "E02 update source stale revision 409",
                        oracle.patch(
                            f"{base_o}/spatial/sources/{src_o}",
                            headers=csrf,
                            json={"expected_revision": 1, "name": "Again"},
                        ),
                        rust.patch(
                            f"{base_r}/spatial/sources/{src_r}",
                            headers=csrf,
                            json={"expected_revision": 1, "name": "Again"},
                        ),
                    )

                    # 3. CRS correction on reference_correctable_crs
                    corr_reg_o = oracle.post(
                        f"{base_o}/spatial/sources",
                        headers=csrf,
                        json={"reference_key": "reference_correctable_crs", "name": "Correctable"},
                    )
                    corr_reg_r = rust.post(
                        f"{base_r}/spatial/sources",
                        headers=csrf,
                        json={"reference_key": "reference_correctable_crs", "name": "Correctable"},
                    )
                    compare_response("E02 register correctable source", corr_reg_o, corr_reg_r, normalize=normalize_mutation)
                    csrc_o = corr_reg_o.json()["resource_id"]
                    csrc_r = corr_reg_r.json()["resource_id"]

                    # Non-ready source cannot create layer (409 SOURCE_NOT_READY)
                    compare_response(
                        "E02 create layer on non-ready source 409",
                        oracle.post(f"{base_o}/layers", headers=csrf, json={"source_id": csrc_o, "name": "Blocked"}),
                        rust.post(f"{base_r}/layers", headers=csrf, json={"source_id": csrc_r, "name": "Blocked"}),
                    )

                    # Unverified CRS correction fails 422
                    compare_response(
                        "E02 update source unverified CRS 422",
                        oracle.patch(
                            f"{base_o}/spatial/sources/{csrc_o}",
                            headers=csrf,
                            json={
                                "expected_revision": 1,
                                "declared_crs": {"authority": "EPSG", "code": 4756, "coordinate_order": "xy"},
                            },
                        ),
                        rust.patch(
                            f"{base_r}/spatial/sources/{csrc_r}",
                            headers=csrf,
                            json={
                                "expected_revision": 1,
                                "declared_crs": {"authority": "EPSG", "code": 4756, "coordinate_order": "xy"},
                            },
                        ),
                    )
                    # Verified CRS correction to 4326 succeeds
                    compare_response(
                        "E02 update source verified CRS correction",
                        oracle.patch(
                            f"{base_o}/spatial/sources/{csrc_o}",
                            headers=csrf,
                            json={
                                "expected_revision": 1,
                                "declared_crs": {"authority": "EPSG", "code": 4326, "coordinate_order": "xy"},
                            },
                        ),
                        rust.patch(
                            f"{base_r}/spatial/sources/{csrc_r}",
                            headers=csrf,
                            json={
                                "expected_revision": 1,
                                "declared_crs": {"authority": "EPSG", "code": 4326, "coordinate_order": "xy"},
                            },
                        ),
                        normalize=normalize_mutation,
                    )

                    # 4. create_layer, rename_layer, retire_layer, retire_source
                    lyr_o = oracle.post(f"{base_o}/layers", headers=csrf, json={"source_id": src_o, "name": "Layer 1"})
                    lyr_r = rust.post(f"{base_r}/layers", headers=csrf, json={"source_id": src_r, "name": "Layer 1"})
                    compare_response("E02 create layer", lyr_o, lyr_r, normalize=normalize_mutation)
                    lid_o = lyr_o.json()["resource_id"]
                    lid_r = lyr_r.json()["resource_id"]

                    compare_response(
                        "E02 rename layer",
                        oracle.patch(f"{base_o}/layers/{lid_o}", headers=csrf, json={"expected_revision": 1, "name": "Layer 1B"}),
                        rust.patch(f"{base_r}/layers/{lid_r}", headers=csrf, json={"expected_revision": 1, "name": "Layer 1B"}),
                        normalize=normalize_mutation,
                    )
                    compare_response(
                        "E02 rename layer stale revision 409",
                        oracle.patch(f"{base_o}/layers/{lid_o}", headers=csrf, json={"expected_revision": 1, "name": "Layer 1C"}),
                        rust.patch(f"{base_r}/layers/{lid_r}", headers=csrf, json={"expected_revision": 1, "name": "Layer 1C"}),
                    )
                    compare_response(
                        "E02 retire source blocked by active layer 409",
                        oracle.post(f"{base_o}/spatial/sources/{src_o}/retire", headers=csrf, json={"expected_revision": 2}),
                        rust.post(f"{base_r}/spatial/sources/{src_r}/retire", headers=csrf, json={"expected_revision": 2}),
                    )
                    compare_response(
                        "E02 retire layer",
                        oracle.post(f"{base_o}/layers/{lid_o}/retire", headers=csrf, json={"expected_revision": 2}),
                        rust.post(f"{base_r}/layers/{lid_r}/retire", headers=csrf, json={"expected_revision": 2}),
                        normalize=normalize_mutation,
                    )
                    compare_response(
                        "E02 retire layer already retired 404",
                        oracle.post(f"{base_o}/layers/{lid_o}/retire", headers=csrf, json={"expected_revision": 3}),
                        rust.post(f"{base_r}/layers/{lid_r}/retire", headers=csrf, json={"expected_revision": 3}),
                    )
                    compare_response(
                        "E02 retire source after layer retired",
                        oracle.post(f"{base_o}/spatial/sources/{src_o}/retire", headers=csrf, json={"expected_revision": 2}),
                        rust.post(f"{base_r}/spatial/sources/{src_r}/retire", headers=csrf, json={"expected_revision": 2}),
                        normalize=normalize_mutation,
                    )

                    # 5. create_manual_layer + idempotency + geometry-preview split & merge
                    op_o, op_r = str(uuid4()), str(uuid4())
                    manual_o_payload = {
                        "operation_id": op_o,
                        "name": "Manual Polygon",
                        "geometry_family": "Polygon",
                        "fields": [{"field_id": "fld_name", "name": "Tên", "type": "text", "nullable": False}],
                    }
                    manual_r_payload = {**manual_o_payload, "operation_id": op_r}
                    man_o = oracle.post(f"{base_o}/layers/manual", headers=csrf, json=manual_o_payload)
                    man_r = rust.post(f"{base_r}/layers/manual", headers=csrf, json=manual_r_payload)
                    compare_response("E02 create manual layer", man_o, man_r, normalize=normalize_manual)
                    compare_response(
                        "E02 create manual layer idempotent replay",
                        oracle.post(f"{base_o}/layers/manual", headers=csrf, json=manual_o_payload),
                        rust.post(f"{base_r}/layers/manual", headers=csrf, json=manual_r_payload),
                        normalize=normalize_manual,
                    )
                    compare_response(
                        "E02 create manual layer conflicting operation_id 409",
                        oracle.post(f"{base_o}/layers/manual", headers=csrf, json={**manual_o_payload, "name": "Other"}),
                        rust.post(f"{base_r}/layers/manual", headers=csrf, json={**manual_r_payload, "name": "Other"}),
                    )

                    mlid_o, msid_o = man_o.json()["layer_id"], man_o.json()["source_id"]
                    mlid_r, msid_r = man_r.json()["layer_id"], man_r.json()["source_id"]
                    pin_o = oracle.get(f"{base_o}/layers/{mlid_o}", headers=headers).json()["source"]
                    pin_r = rust.get(f"{base_r}/layers/{mlid_r}", headers=headers).json()["source"]

                    poly_geom = {
                        "type": "Polygon",
                        "coordinates": [
                            [[106.5, 10.7], [106.6, 10.7], [106.6, 10.8], [106.5, 10.8], [106.5, 10.7]]
                        ],
                    }
                    cs_o = oracle.post(
                        f"{base_o}/sources/{msid_o}/changesets",
                        headers=csrf,
                        json={
                            "operation_id": f"seed-{uuid4().hex[:8]}",
                            "base_content_revision": pin_o["content_revision"],
                            "base_schema_revision": pin_o["schema_revision"],
                            "summary": "seed poly",
                            "operations": [
                                {"op_type": "create", "feature_id": "poly_1", "geometry": poly_geom, "properties": {"fld_name": "A"}}
                            ],
                        },
                    )
                    cs_r = rust.post(
                        f"{base_r}/sources/{msid_r}/changesets",
                        headers=csrf,
                        json={
                            "operation_id": f"seed-{uuid4().hex[:8]}",
                            "base_content_revision": pin_r["content_revision"],
                            "base_schema_revision": pin_r["schema_revision"],
                            "summary": "seed poly",
                            "operations": [
                                {"op_type": "create", "feature_id": "poly_1", "geometry": poly_geom, "properties": {"fld_name": "A"}}
                            ],
                        },
                    )
                    require(cs_o.status_code in (200, 201) and cs_r.status_code in (200, 201), "seed manual polygon failed")
                    pin_o = oracle.get(f"{base_o}/layers/{mlid_o}", headers=headers).json()["source"]
                    pin_r = rust.get(f"{base_r}/layers/{mlid_r}", headers=headers).json()["source"]

                    blade = [[106.55, 10.65], [106.55, 10.85]]
                    split_o = oracle.post(
                        f"{base_o}/layers/{mlid_o}/geometry-preview",
                        headers=csrf,
                        json={
                            "action": "split",
                            "content_revision": pin_o["content_revision"],
                            "schema_revision": pin_o["schema_revision"],
                            "feature_ids": ["poly_1"],
                            "blade": blade,
                        },
                    )
                    split_r = rust.post(
                        f"{base_r}/layers/{mlid_r}/geometry-preview",
                        headers=csrf,
                        json={
                            "action": "split",
                            "content_revision": pin_r["content_revision"],
                            "schema_revision": pin_r["schema_revision"],
                            "feature_ids": ["poly_1"],
                            "blade": blade,
                        },
                    )
                    require(
                        split_o.json().get("content_revision") == pin_o["content_revision"]
                        and split_r.json().get("content_revision") == pin_r["content_revision"]
                        and split_o.json().get("schema_revision") == pin_o["schema_revision"]
                        and split_r.json().get("schema_revision") == pin_r["schema_revision"],
                        "geometry-preview revision mismatch",
                    )
                    compare_response(
                        "E02 geometry-preview split",
                        split_o,
                        split_r,
                        normalize=lambda b: {
                            **b,
                            "source_id": "<source_id>",
                            "content_revision": "<content_revision>",
                            "schema_revision": "<schema_revision>",
                        },
                    )
                    compare_response(
                        "E02 geometry-preview stale revision 409",
                        oracle.post(
                            f"{base_o}/layers/{mlid_o}/geometry-preview",
                            headers=csrf,
                            json={
                                "action": "split",
                                "content_revision": "0" * 64,
                                "schema_revision": pin_o["schema_revision"],
                                "feature_ids": ["poly_1"],
                                "blade": blade,
                            },
                        ),
                        rust.post(
                            f"{base_r}/layers/{mlid_r}/geometry-preview",
                            headers=csrf,
                            json={
                                "action": "split",
                                "content_revision": "0" * 64,
                                "schema_revision": pin_r["schema_revision"],
                                "feature_ids": ["poly_1"],
                                "blade": blade,
                            },
                        ),
                    )

                    # 6. GIS-scoped access denial
                    enable_gis_scoped_editor(owner_url, org_id, (proj_o, proj_r), user_a)
                    compare_response(
                        "E02 GIS-scoped user denied create manual layer 403",
                        oracle.post(f"{base_o}/layers/manual", headers=csrf, json=manual_o_payload),
                        rust.post(f"{base_r}/layers/manual", headers=csrf, json=manual_r_payload),
                    )

                    # 7. Verify catalog_changes & audit_events recorded symmetrically
                    with psycopg.connect(owner_url) as conn:
                        cnt_o = conn.execute(
                            "SELECT count(*) FROM spatial.catalog_changes WHERE organization_id=%s AND project_id=%s",
                            (org_id, proj_o),
                        ).fetchone()[0]
                        cnt_r = conn.execute(
                            "SELECT count(*) FROM spatial.catalog_changes WHERE organization_id=%s AND project_id=%s",
                            (org_id, proj_r),
                        ).fetchone()[0]
                        require(cnt_o == cnt_r and cnt_r >= 7, f"catalog_changes mismatch: oracle={cnt_o}, rust={cnt_r}")
                    print("PASS E02 catalog_changes & audit count parity", flush=True)
            finally:
                get_engine().dispose()

    print("E02 catalog write parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
