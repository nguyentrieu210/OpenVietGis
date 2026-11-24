from __future__ import annotations

import json
import math
import platform
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from uuid import UUID, uuid4

from openvietgis_api.adapters.persistence.db import get_session_factory
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world

from modules.house_numbering.application import HouseNumberingService
from modules.house_numbering.exports import HouseNumberingExportService
from modules.house_numbering.migrate import upgrade as migrate_house_numbering

OUTPUT = Path("/tmp/hn019-performance.json")


def p95(values: list[float]) -> float:
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * 0.95) - 1)]


def _source(
    connection,
    *,
    source_id: UUID,
    org: UUID,
    project: UUID,
    key: str,
    family: str,
    count: int,
    extent: dict[str, float | int],
) -> None:
    connection.execute(
        text(
            """
            INSERT INTO spatial.data_sources (
                id,organization_id,project_id,name,kind,reference_key,fixture_version,
                manifest_digest,source_asset_id,lifecycle,validation_state,validation_reason,
                source_crs,stored_crs,schema_state,geometry_family,dimensions,
                feature_count_state,feature_count,extent_state,extent,catalog_revision,
                content_revision,schema_revision,created_at,updated_at,retired_at
            ) VALUES (
                :id,:org,:project,:name,'reference_fixture',:key,'hn019-benchmark',
                :manifest,NULL,'active','ready',NULL,3857,3857,'known',:family,'XY',
                'known',:count,'available',CAST(:extent AS jsonb),1,:content,:schema,
                now(),now(),NULL
            )
            """
        ),
        {
            "id": source_id,
            "org": org,
            "project": project,
            "name": key,
            "key": key,
            "manifest": "c" * 64,
            "family": family,
            "count": count,
            "extent": json.dumps(extent),
            "content": "a" * 64,
            "schema": "b" * 64,
        },
    )


def seed() -> tuple[UUID, UUID, UUID, UUID, list[UUID]]:
    migrate_house_numbering()
    world = seed_access_world()
    org = world.organization_id
    project = world.project_id
    owner = world.owner_user_id
    rule_id = uuid4()
    with get_admin_engine().begin() as c:
        c.execute(
            text(
                """
                INSERT INTO house_numbering.rules (
                    id,organization_id,project_id,version,status,authority_ref,
                    left_odd,suffix_style,max_distance_m,revision,
                    created_by_actor_id,updated_by_actor_id,created_at,updated_at
                ) VALUES (
                    :id,:org,:project,'PERF-019','authoritative','SYNTH-PERF-019',
                    true,'letters',150,1,:actor,:actor,now(),now()
                )
                """
            ),
            {
                "id": rule_id,
                "org": org,
                "project": project,
                "actor": owner,
            },
        )

        manifest_id = c.execute(
            text(
                """
                SELECT id FROM extensions.manifests
                WHERE organization_id=:org AND extension_key='house_numbering'
                ORDER BY created_at DESC LIMIT 1
                """
            ),
            {"org": org},
        ).scalar_one_or_none()
        if manifest_id is None:
            manifest = json.loads(
                (Path(__file__).resolve().parents[2] / "manifest.json").read_text()
            )
            manifest_id = uuid4()
            c.execute(
                text(
                    """
                    INSERT INTO extensions.manifests (
                        id,organization_id,extension_key,version,api_compatibility,
                        display_name,description,permissions,commands,capabilities,
                        entity_schemas,ui_contributions,enabled,registered_by_actor_id,created_at
                    ) VALUES (
                        :id,:org,'house_numbering',:version,:api,:name,
                        :description,CAST(:permissions AS jsonb),CAST(:commands AS jsonb),
                        CAST(:capabilities AS jsonb),CAST(:schemas AS jsonb),
                        CAST(:ui AS jsonb),true,:actor,now()
                    )
                    """
                ),
                {
                    "id": manifest_id,
                    "org": org,
                    "actor": owner,
                    "version": manifest["version"],
                    "api": manifest["api_compatibility"],
                    "name": manifest["display_name"],
                    "description": manifest["description"],
                    "permissions": json.dumps(manifest["permissions"]),
                    "commands": json.dumps(manifest["commands"]),
                    "capabilities": json.dumps(manifest["capabilities"]),
                    "schemas": json.dumps(manifest["entity_schemas"]),
                    "ui": json.dumps(manifest["ui_contributions"]),
                },
            )
        c.execute(
            text(
                """
                INSERT INTO extensions.project_activations (
                    id,organization_id,project_id,manifest_id,extension_key,status,revision,
                    activated_by_actor_id,activated_at,disabled_by_actor_id,disabled_at,updated_at
                ) VALUES (
                    :id,:org,:project,:manifest,'house_numbering','active',1,
                    :actor,now(),NULL,NULL,now()
                )
                ON CONFLICT (organization_id,project_id,extension_key)
                DO UPDATE SET status='active',manifest_id=EXCLUDED.manifest_id,
                  disabled_by_actor_id=NULL,disabled_at=NULL,updated_at=now()
                """
            ),
            {
                "id": uuid4(),
                "org": org,
                "project": project,
                "manifest": manifest_id,
                "actor": owner,
            },
        )

        line_source = uuid4()
        points10 = uuid4()
        points100 = uuid4()
        _source(
            c,
            source_id=line_source,
            org=org,
            project=project,
            key="hn019_bench_lines",
            family="LineString",
            count=2,
            extent={
                "min_x": 0,
                "min_y": 0,
                "max_x": 100000,
                "max_y": 1000,
                "crs_code": 3857,
            },
        )
        _source(
            c,
            source_id=points10,
            org=org,
            project=project,
            key="hn019_bench_points_10k",
            family="Point",
            count=10000,
            extent={
                "min_x": 1,
                "min_y": -10,
                "max_x": 10000,
                "max_y": -10,
                "crs_code": 3857,
            },
        )
        _source(
            c,
            source_id=points100,
            org=org,
            project=project,
            key="hn019_bench_points_100k",
            family="Point",
            count=100000,
            extent={
                "min_x": 1,
                "min_y": 990,
                "max_x": 100000,
                "max_y": 990,
                "crs_code": 3857,
            },
        )
        c.execute(
            text(
                """
                INSERT INTO spatial.reference_features (
                    organization_id,project_id,source_id,feature_id,attributes,geometry,
                    geometry_state,content_revision,schema_revision
                ) VALUES
                (:org,:project,:source,'route10','{}'::jsonb,
                 ST_GeomFromText('LINESTRING(0 0,10001 0)',3857),'present',:content,:schema),
                (:org,:project,:source,'route100','{}'::jsonb,
                 ST_GeomFromText('LINESTRING(0 1000,100001 1000)',3857),'present',:content,:schema)
                """
            ),
            {
                "org": org,
                "project": project,
                "source": line_source,
                "content": "a" * 64,
                "schema": "b" * 64,
            },
        )
        c.execute(
            text(
                """
                INSERT INTO spatial.reference_features (
                    organization_id,project_id,source_id,feature_id,attributes,geometry,
                    geometry_state,content_revision,schema_revision
                )
                SELECT :org,:project,:source,'b10_'||g,'{}'::jsonb,
                       ST_SetSRID(ST_MakePoint(g,-10),3857),'present',:content,:schema
                FROM generate_series(1,10000) AS g
                """
            ),
            {
                "org": org,
                "project": project,
                "source": points10,
                "content": "a" * 64,
                "schema": "b" * 64,
            },
        )
        c.execute(
            text(
                """
                INSERT INTO spatial.reference_features (
                    organization_id,project_id,source_id,feature_id,attributes,geometry,
                    geometry_state,content_revision,schema_revision
                )
                SELECT :org,:project,:source,'b100_'||g,'{}'::jsonb,
                       ST_SetSRID(ST_MakePoint(g,990),3857),'present',:content,:schema
                FROM generate_series(1,100000) AS g
                """
            ),
            {
                "org": org,
                "project": project,
                "source": points100,
                "content": "a" * 64,
                "schema": "b" * 64,
            },
        )
        route10, route100 = uuid4(), uuid4()
        for route_id, feature, name in (
            (route10, "route10", "Benchmark 10k"),
            (route100, "route100", "Benchmark 100k"),
        ):
            c.execute(
                text(
                    """
                    INSERT INTO house_numbering.routes (
                        id,organization_id,project_id,name,route_type,parent_route_id,prefix,
                        linked_source_id,linked_feature_id,direction_state,source_label,status,
                        revision,created_by_actor_id,updated_by_actor_id,created_at,updated_at
                    ) VALUES (
                        :id,:org,:project,:name,'street',NULL,NULL,:source,:feature,
                        'forward','benchmark','confirmed',1,:actor,:actor,now(),now()
                    )
                    """
                ),
                {
                    "id": route_id,
                    "org": org,
                    "project": project,
                    "name": name,
                    "source": line_source,
                    "feature": feature,
                    "actor": owner,
                },
            )

        c.execute(
            text(
                """
                INSERT INTO house_numbering.buildings (
                    id,organization_id,project_id,route_id,area_id,parcel_ref,
                    linked_source_id,linked_feature_id,eligibility,ineligibility_reason,
                    state,revision,created_by_actor_id,updated_by_actor_id,created_at,updated_at
                )
                SELECT md5('hn10-building-'||g)::uuid,:org,:project,:route,NULL,g::text,
                       :source,'b10_'||g,'eligible',NULL,'active',1,:actor,:actor,now(),now()
                FROM generate_series(1,10000) AS g
                """
            ),
            {
                "org": org,
                "project": project,
                "route": route10,
                "source": points10,
                "actor": owner,
            },
        )
        c.execute(
            text(
                """
                INSERT INTO house_numbering.address_numbers (
                    id,organization_id,project_id,building_id,route_id,number_text,kind,
                    side,measure_m,rule_id,source_evidence_id,retired_at,revision,
                    created_by_actor_id,updated_by_actor_id,created_at,updated_at
                )
                SELECT md5('hn10-number-'||g)::uuid,:org,:project,
                       md5('hn10-building-'||g)::uuid,:route,(g*2)::text,'approved',
                       'right',g,:rule,NULL,NULL,1,:actor,:actor,now(),now()
                FROM generate_series(1,10000) AS g
                """
            ),
            {
                "org": org,
                "project": project,
                "route": route10,
                "rule": rule_id,
                "actor": owner,
            },
        )
        c.execute(
            text(
                """
                INSERT INTO house_numbering.buildings (
                    id,organization_id,project_id,route_id,area_id,parcel_ref,
                    linked_source_id,linked_feature_id,eligibility,ineligibility_reason,
                    state,revision,created_by_actor_id,updated_by_actor_id,created_at,updated_at
                )
                SELECT md5('hn100-building-'||g)::uuid,:org,:project,:route,NULL,g::text,
                       :source,'b100_'||g,'eligible',NULL,'active',1,:actor,:actor,now(),now()
                FROM generate_series(1,100000) AS g
                """
            ),
            {
                "org": org,
                "project": project,
                "route": route100,
                "source": points100,
                "actor": owner,
            },
        )
        c.execute(
            text(
                """
                INSERT INTO house_numbering.address_numbers (
                    id,organization_id,project_id,building_id,route_id,number_text,kind,
                    side,measure_m,rule_id,source_evidence_id,retired_at,revision,
                    created_by_actor_id,updated_by_actor_id,created_at,updated_at
                )
                SELECT md5('hn100-number-'||g)::uuid,:org,:project,
                       md5('hn100-building-'||g)::uuid,:route,(g*2)::text,'approved',
                       'right',g,:rule,NULL,NULL,1,:actor,:actor,now(),now()
                FROM generate_series(1,100000) AS g
                """
            ),
            {
                "org": org,
                "project": project,
                "route": route100,
                "rule": rule_id,
                "actor": owner,
            },
        )

        users: list[UUID] = []
        for index in range(50):
            user, membership = uuid4(), uuid4()
            users.append(user)
            c.execute(
                text(
                    """
                    INSERT INTO identity.users (
                        id,display_name,email_snapshot,status,created_at,updated_at
                    ) VALUES (:id,:name,:email,'active',now(),now())
                    """
                ),
                {
                    "id": user,
                    "name": f"Bench {index}",
                    "email": f"bench-{index}@example.test",
                },
            )
            c.execute(
                text(
                    """
                    INSERT INTO org.organization_memberships (
                        id,organization_id,user_id,role,status,created_at,activated_at
                    ) VALUES (:id,:org,:user,'member','active',now(),now())
                    """
                ),
                {"id": membership, "org": org, "user": user},
            )
            c.execute(
                text(
                    """
                    INSERT INTO org.resource_grants (
                        id,organization_id,subject_kind,membership_id,scope_kind,
                        project_id,role,created_at,created_by_actor_type,created_by_actor_id
                    ) VALUES (
                        :id,:org,'membership',:membership,'project',:project,'viewer',
                        now(),'human',:owner
                    )
                    """
                ),
                {
                    "id": uuid4(),
                    "org": org,
                    "membership": membership,
                    "project": project,
                    "owner": owner,
                },
            )
    return org, project, rule_id, route10, users


def benchmark() -> dict[str, object]:
    org, project, rule_id, route10, users = seed()
    # This benchmark intentionally bulk-loads 220k+ rows in one transaction. Refresh
    # planner statistics before measuring steady-state request latency; production
    # receives these rows incrementally and PostgreSQL autovacuum maintains stats.
    with get_admin_engine().begin() as connection:
        connection.execute(text("ANALYZE house_numbering.buildings"))
        connection.execute(text("ANALYZE house_numbering.address_numbers"))
        connection.execute(text("ANALYZE house_numbering.evidence"))
        connection.execute(text("ANALYZE spatial.reference_features"))
    with get_admin_engine().connect() as connection:
        owner = connection.execute(
            text("SELECT created_by_actor_id FROM house_numbering.rules WHERE id=:id"),
            {"id": rule_id},
        ).scalar_one()
    inspect_times: list[float] = []
    for offset in range(12):
        session = get_session_factory()()
        try:
            context = AuthorizationContext(
                actor=Actor("human", UUID(str(owner)), "Benchmark Owner"),
                organization_id=org,
                project_id=project,
            )
            service = HouseNumberingService(session, context)
            started = time.perf_counter()
            result = service.inspect(
                x=5000.25 + offset,
                y=-8,
                crs_code=3857,
                route_id=route10,
                rule_id=rule_id,
            )
            elapsed = time.perf_counter() - started
            if result["location"]["side"] != "right":
                raise RuntimeError("inspect benchmark returned wrong side")
            inspect_times.append(elapsed)
        finally:
            session.rollback()
            session.close()

    def reader(item: tuple[int, UUID]) -> float:
        index, user = item
        session = get_session_factory()()
        try:
            context = AuthorizationContext(
                actor=Actor("human", user, f"Bench {index}"),
                organization_id=org,
                project_id=project,
            )
            service = HouseNumberingExportService(session, context)
            started = time.perf_counter()
            rows = service.search(query=str(100000 + index * 2), limit=20)
            elapsed = time.perf_counter() - started
            if not rows:
                raise RuntimeError(f"reader {index} returned no row")
            return elapsed
        finally:
            session.rollback()
            session.close()

    concurrent_started = time.perf_counter()
    with ThreadPoolExecutor(max_workers=50) as pool:
        concurrent_times = list(pool.map(reader, enumerate(users)))
    concurrent_wall = time.perf_counter() - concurrent_started

    with get_admin_engine().connect() as c:
        counts = {
            "buildings": c.execute(
                text(
                    "SELECT count(*) FROM house_numbering.buildings "
                    "WHERE organization_id=:org AND project_id=:project"
                ),
                {"org": org, "project": project},
            ).scalar_one(),
            "numbers": c.execute(
                text(
                    "SELECT count(*) FROM house_numbering.address_numbers "
                    "WHERE organization_id=:org AND project_id=:project"
                ),
                {"org": org, "project": project},
            ).scalar_one(),
        }

    result: dict[str, object] = {
        "profile": {
            "python": platform.python_version(),
            "machine": platform.machine(),
            "processor": platform.processor(),
            "database": "PostgreSQL/PostGIS isolated HN019 recovery container",
        },
        "dataset": {
            "inspect_route_buildings": 10000,
            "total_buildings": counts["buildings"],
            "numbers": counts["numbers"],
        },
        "inspect_10k": {
            "samples": len(inspect_times),
            "p95_seconds": p95(inspect_times),
            "max_seconds": max(inspect_times),
            "median_seconds": sorted(inspect_times)[len(inspect_times) // 2],
            "criterion_p95_under_3s": p95(inspect_times) < 3.0,
        },
        "concurrent_100k": {
            "actors": 50,
            "wall_seconds": concurrent_wall,
            "p95_seconds": p95(concurrent_times),
            "max_seconds": max(concurrent_times),
            "all_returned_results": True,
        },
    }
    OUTPUT.write_text(json.dumps(result, indent=2), encoding="utf-8")
    return result


if __name__ == "__main__":
    print(json.dumps(benchmark(), indent=2))
