#!/usr/bin/env python3
"""E15 House Numbering read-side differential parity on isolated real PostGIS."""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
from datetime import UTC, datetime, timedelta
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


def seed_module_rows(owner_url: str, fixture: dict) -> dict[str, str]:
    org = fixture["org_a_id"]
    project = fixture["project_a_id"]
    actor = fixture["user_a_id"]
    base = datetime(2026, 11, 26, 8, 0, 0, 123456, tzinfo=UTC)

    rule_old, rule_new = uuid4(), uuid4()
    area_b, area_a = uuid4(), uuid4()
    route_street, route_alley = uuid4(), uuid4()
    building_active, building_removed, building_other = uuid4(), uuid4(), uuid4()

    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO house_numbering.rules (
                id, organization_id, project_id, version, status, authority_ref,
                left_odd, suffix_style, max_distance_m, sealed, revision,
                created_by_actor_id, updated_by_actor_id, created_at, updated_at
            ) VALUES
                (%s,%s,%s,'1.0.0','draft',NULL,true,'letters',150.00,true,1,%s,%s,%s,%s),
                (%s,%s,%s,'2.0.0','authoritative','QD-02',false,'dash',200.50,true,2,%s,%s,%s,%s)
            """,
            (
                rule_old, org, project, actor, actor, base, base,
                rule_new, org, project, actor, actor,
                base + timedelta(seconds=1), base + timedelta(seconds=1),
            ),
        )

        conn.execute(
            """
            INSERT INTO house_numbering.areas (
                id, organization_id, project_id, name, kind, parent_id,
                linked_source_id, linked_feature_id, revision,
                created_by_actor_id, updated_by_actor_id, created_at, updated_at
            ) VALUES
                (%s,%s,%s,'B Area','current',NULL,NULL,NULL,1,%s,%s,%s,%s),
                (%s,%s,%s,'A Area','historic',NULL,NULL,NULL,3,%s,%s,%s,%s)
            """,
            (
                area_b, org, project, actor, actor, base, base,
                area_a, org, project, actor, actor,
                base + timedelta(seconds=1), base + timedelta(seconds=1),
            ),
        )

        conn.execute(
            """
            INSERT INTO house_numbering.routes (
                id, organization_id, project_id, name, route_type, parent_route_id,
                prefix, linked_source_id, linked_feature_id, direction_state,
                source_label, status, revision, created_by_actor_id,
                updated_by_actor_id, created_at, updated_at, segments, reviewed_aliases
            ) VALUES
                (%s,%s,%s,'Main Street','street',NULL,NULL,NULL,NULL,'forward',
                 'manual','confirmed',4,%s,%s,%s,%s,'[]'::jsonb,'[]'::jsonb),
                (%s,%s,%s,'Alley 8B','alley',%s,'8B',NULL,NULL,'reverse',
                 'import','draft',2,%s,%s,%s,%s,
                 %s::jsonb,%s::jsonb)
            """,
            (
                route_street, org, project, actor, actor, base, base,
                route_alley, org, project, route_street, actor, actor,
                base + timedelta(seconds=1), base + timedelta(seconds=1),
                json.dumps([{"source_id": str(uuid4()), "feature_id": "seg-1", "code": "S1"}]),
                json.dumps([{"code": "A8B", "name": "Hẻm 8B"}]),
            ),
        )

        conn.execute(
            """
            INSERT INTO house_numbering.buildings (
                id, organization_id, project_id, route_id, area_id, parcel_ref,
                linked_source_id, linked_feature_id, eligibility, ineligibility_reason,
                state, revision, created_by_actor_id, updated_by_actor_id,
                created_at, updated_at, plaque_status, plaque_updated_at
            ) VALUES
                (%s,%s,%s,%s,%s,'P-001',NULL,NULL,'eligible',NULL,
                 'active',5,%s,%s,%s,%s,'installed',%s),
                (%s,%s,%s,%s,%s,'P-REMOVED',NULL,NULL,'unknown',NULL,
                 'removed',1,%s,%s,%s,%s,'removed',%s),
                (%s,%s,%s,%s,%s,'P-002',NULL,NULL,'ineligible','outside scope',
                 'active',2,%s,%s,%s,%s,'planned',NULL)
            """,
            (
                building_active, org, project, route_street, area_a, actor, actor,
                base, base, base + timedelta(seconds=2),
                building_removed, org, project, route_street, area_b, actor, actor,
                base + timedelta(seconds=1), base + timedelta(seconds=1),
                base + timedelta(seconds=2),
                building_other, org, project, route_alley, area_b, actor, actor,
                base + timedelta(seconds=2), base + timedelta(seconds=2),
            ),
        )

        # Current-number selection must prefer approved over a newer survey.
        for offset, kind, number in (
            (0, "survey", "SURVEY-9"),
            (1, "proposed", "PROP-9"),
            (2, "approved", "9"),
        ):
            now = base + timedelta(seconds=10 + offset)
            conn.execute(
                """
                INSERT INTO house_numbering.address_numbers (
                    id, organization_id, project_id, building_id, route_id,
                    number_text, kind, side, measure_m, rule_id, source_evidence_id,
                    retired_at, revision, created_by_actor_id, updated_by_actor_id,
                    created_at, updated_at
                ) VALUES (
                    %s,%s,%s,%s,%s,%s,%s,'left',12.5,%s,NULL,
                    NULL,1,%s,%s,%s,%s
                )
                """,
                (
                    uuid4(), org, project, building_active, route_street,
                    number, kind, rule_new, actor, actor, now, now,
                ),
            )

    return {
        "rule_old": str(rule_old),
        "rule_new": str(rule_new),
        "area_a": str(area_a),
        "area_b": str(area_b),
        "route_street": str(route_street),
        "route_alley": str(route_alley),
        "building_active": str(building_active),
        "building_other": str(building_other),
    }


def set_scoped_viewer(owner_url: str, fixture: dict) -> None:
    now = datetime.now(UTC)
    with psycopg.connect(owner_url) as conn:
        membership_id = conn.execute(
            """
            SELECT id FROM org.organization_memberships
            WHERE organization_id=%s AND user_id=%s
            """,
            (fixture["org_a_id"], fixture["user_a_id"]),
        ).fetchone()[0]
        conn.execute(
            """
            UPDATE org.organization_memberships
            SET role='member'
            WHERE organization_id=%s AND user_id=%s
            """,
            (fixture["org_a_id"], fixture["user_a_id"]),
        )
        conn.execute(
            """
            UPDATE org.resource_grants
            SET revoked_at=%s
            WHERE organization_id=%s AND membership_id=%s AND revoked_at IS NULL
            """,
            (now, fixture["org_a_id"], membership_id),
        )
        conn.execute(
            """
            INSERT INTO org.resource_grants (
                id, organization_id, subject_kind, membership_id, scope_kind,
                project_id, role, created_at, created_by_actor_type, created_by_actor_id
            ) VALUES (%s,%s,'membership',%s,'project',%s,'viewer',%s,'human',%s)
            """,
            (
                uuid4(), fixture["org_a_id"], membership_id,
                fixture["project_a_id"], now, fixture["user_a_id"],
            ),
        )
        conn.execute(
            """
            INSERT INTO gis_access.scoped_members(organization_id,project_id,user_id)
            VALUES (%s,%s,%s)
            ON CONFLICT DO NOTHING
            """,
            (fixture["org_a_id"], fixture["project_a_id"], fixture["user_a_id"]),
        )


@parity.case
def compare_response(label: str, expected, actual):
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}",
    )
    mismatch = parity.difference(expected.json(), actual.json())
    require(mismatch is None, f"{label}: {mismatch}")
    print(f"PASS {label}", flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e15-parity-") as directory:
        work = Path(directory)
        with parity.database(admin_url) as (owner_url, runtime_url):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            parity.configure_python(owner_url, runtime_url, artifacts)
            baseline.run_alembic_upgrade(owner_url, runtime_url)

            # House Numbering owns an independent Alembic version table.
            from modules.house_numbering.migrate import (
                upgrade as upgrade_house_numbering,
            )
            upgrade_house_numbering()

            fixture = baseline.seed_benchmark_database(
                owner_url,
                seed=20261126,
                scales=["1k"],
                csrf_secret=baseline.DEFAULT_CSRF_SECRET,
            )
            ids = seed_module_rows(owner_url, fixture)

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            org = f"{API}/organizations/{fixture['org_a_id']}"
            project = f"{org}/projects/{fixture['project_a_id']}"
            root = f"{project}/modules/house-numbering"
            headers = parity.session_headers(fixture["session_a_token"])
            csrf = parity.session_headers(
                fixture["session_a_token"],
                **{"x-openvietgis-csrf": fixture["csrf_a_token"]},
            )

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    registered = oracle.post(
                        f"{org}/extensions/official/house_numbering/register",
                        headers=csrf,
                    )
                    require(registered.status_code == 201, registered.text)
                    manifest_id = registered.json()["id"]
                    activated = oracle.post(
                        f"{project}/extensions/activations",
                        json={"manifest_id": manifest_id},
                        headers=csrf,
                    )
                    require(activated.status_code == 201, activated.text)

                    for suffix, label in (
                        ("rules", "E15 list rules"),
                        ("areas", "E15 list areas"),
                        ("routes", "E15 list routes"),
                        ("buildings", "E15 list buildings"),
                    ):
                        compare_response(
                            label,
                            oracle.get(f"{root}/{suffix}", headers=headers),
                            rust.get(f"{root}/{suffix}", headers=headers),
                        )

                    rules = rust.get(f"{root}/rules", headers=headers).json()["items"]
                    require(
                        [row["version"] for row in rules] == ["2.0.0", "1.0.0"],
                        f"rule order mismatch: {rules}",
                    )
                    areas = rust.get(f"{root}/areas", headers=headers).json()["items"]
                    require(
                        [row["name"] for row in areas] == ["A Area", "B Area"],
                        f"area order mismatch: {areas}",
                    )
                    routes = rust.get(f"{root}/routes", headers=headers).json()["items"]
                    route_by_id = {row["id"]: row for row in routes}
                    require(
                        route_by_id[ids["route_street"]]["building_count"] == 1,
                        "removed building leaked into route building_count",
                    )
                    require(
                        route_by_id[ids["route_alley"]]["building_count"] == 1,
                        "alley building_count mismatch",
                    )
                    print("PASS E15 route building counts", flush=True)

                    buildings = rust.get(f"{root}/buildings", headers=headers).json()["items"]
                    require(len(buildings) == 2, f"removed building leaked: {len(buildings)}")
                    by_id = {row["id"]: row for row in buildings}
                    require(
                        by_id[ids["building_active"]]["current_number"] == "9"
                        and by_id[ids["building_active"]]["number_kind"] == "approved",
                        "current-number priority mismatch",
                    )
                    print("PASS E15 current-number priority", flush=True)

                    compare_response(
                        "E15 buildings route filter",
                        oracle.get(
                            f"{root}/buildings",
                            params={"route_id": ids["route_street"]},
                            headers=headers,
                        ),
                        rust.get(
                            f"{root}/buildings",
                            params={"route_id": ids["route_street"]},
                            headers=headers,
                        ),
                    )

                    foreign_headers = parity.session_headers(fixture["session_b_token"])
                    compare_response(
                        "E15 foreign tenant denied",
                        oracle.get(f"{root}/rules", headers=foreign_headers),
                        rust.get(f"{root}/rules", headers=foreign_headers),
                    )

                    # Active metadata must match the shipped runtime contract exactly.
                    with psycopg.connect(owner_url) as conn:
                        original_caps = conn.execute(
                            "SELECT capabilities FROM extensions.manifests WHERE id=%s",
                            (manifest_id,),
                        ).fetchone()[0]
                        conn.execute(
                            "UPDATE extensions.manifests SET capabilities=%s::jsonb WHERE id=%s",
                            (json.dumps(["different_capability"]), manifest_id),
                        )
                    compare_response(
                        "E15 official activation mismatch",
                        oracle.get(f"{root}/routes", headers=headers),
                        rust.get(f"{root}/routes", headers=headers),
                    )
                    with psycopg.connect(owner_url) as conn:
                        conn.execute(
                            "UPDATE extensions.manifests SET capabilities=%s::jsonb WHERE id=%s",
                            (json.dumps(original_caps), manifest_id),
                        )

                    set_scoped_viewer(owner_url, fixture)
                    compare_response(
                        "E15 scoped GIS denied",
                        oracle.get(f"{root}/routes", headers=headers),
                        rust.get(f"{root}/routes", headers=headers),
                    )
            finally:
                get_engine().dispose()

    print("E15 House Numbering read parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
