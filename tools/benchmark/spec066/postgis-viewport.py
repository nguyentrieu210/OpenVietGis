"""Isolated PostGIS query experiment. Run only against the disposable fixture."""
import hashlib
import json
import statistics
import subprocess
import sys

CONTAINER = "ovg066-postgis-fixture"
TABLE = "ovg066_fixture.features"
ORG = "00000000-0000-0000-0000-000000000001"
PROJECT = "00000000-0000-0000-0000-000000000002"
BOX = "ST_MakeEnvelope(105.51,20.51,105.52,20.52,4326)"
DOMAIN = "ST_CoveredBy(geometry, ST_MakeEnvelope(-180,-85.0511287798066,180,85.0511287798066,4326))"
VALID = f"content_revision=repeat('a',64) AND schema_revision=repeat('b',64) AND geometry_state='present' AND ST_SRID(geometry)=4326 AND ST_GeometryType(geometry)='ST_Point' AND {DOMAIN}"


def psql(sql):
    return subprocess.check_output(
        ["docker", "exec", "-i", CONTAINER, "psql", "-X", "-qAt",
         "-v", "ON_ERROR_STOP=1", "-U", "postgres", "-d", "ovg066"],
        input=sql, text=True,
    ).strip()


def scoped(sql, mode="full", org=ORG):
    return psql(
        "BEGIN READ ONLY; SET LOCAL ROLE openvietgis_runtime; "
        f"SET LOCAL openvietgis.organization_id='{org}'; "
        f"SET LOCAL openvietgis.project_id='{PROJECT}'; "
        f"SET LOCAL ovg066.scope='{mode}'; "
        "SET LOCAL statement_timeout='15s'; " + sql + "; ROLLBACK;"
    )


def where(scale):
    return (f"organization_id='{ORG}' AND project_id='{PROJECT}' "
            f"AND source_id='00000000-0000-0000-0000-{scale:012d}'")


def viewport(scale, mode, candidate=False, invalid=False):
    scope = where(scale)
    expected = scale if mode == "full" else scale // 3
    valid = VALID if not invalid else f"({VALID}) AND feature_id <> 'p00000003'"
    summary = ("summary AS MATERIALIZED (SELECT count(*) snapshot_count, "
               "coalesce(bool_and(valid IS TRUE),true) snapshot_valid FROM display)")
    display = (f"display AS {'NOT ' if candidate else ''}MATERIALIZED "
               f"(SELECT feature_id,geometry,({valid}) valid FROM {TABLE} WHERE {scope} LIMIT 50001)")
    if candidate:
        match = (f"SELECT feature_id,geometry,ST_NPoints(geometry) positions FROM {TABLE} CROSS JOIN summary "
                 f"WHERE {scope} AND geometry_state='present' AND snapshot_count={expected} AND snapshot_valid "
                 f"AND geometry && {BOX} AND ST_Intersects(geometry,{BOX})")
    else:
        match = (f"SELECT feature_id,geometry,ST_NPoints(geometry) positions FROM display CROSS JOIN summary "
                 f"WHERE CASE WHEN snapshot_count={expected} AND snapshot_valid AND valid "
                 f"THEN ST_Intersects(geometry,{BOX}) ELSE false END")
    return (f"WITH {display}, {summary}, candidate AS MATERIALIZED ({match} ORDER BY feature_id COLLATE \"C\" LIMIT 501) "
            "SELECT snapshot_count,snapshot_valid,coalesce(array_agg(feature_id ORDER BY feature_id) "
            "FILTER (WHERE feature_id IS NOT NULL),'{}') AS ids FROM summary LEFT JOIN candidate ON true "
            "GROUP BY snapshot_count,snapshot_valid")


def tile(scale):
    return (f"SELECT feature_id FROM {TABLE} WHERE {where(scale)} AND geometry_state='present' "
            "AND content_revision=repeat('a',64) AND schema_revision=repeat('b',64) "
            f"AND ST_SRID(geometry)=4326 AND geometry && {BOX} AND ST_Intersects(geometry,{BOX}) "
            'ORDER BY feature_id COLLATE "C" LIMIT 10001')


def indexes(plan):
    return [plan["Index Name"]] if "Index Name" in plan else [
        name for child in plan.get("Plans", []) for name in indexes(child)
    ]


def measure(query, mode, count=5):
    plans = [json.loads(scoped("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) " + query, mode))[0]
             for _ in range(count)]
    times = [p["Execution Time"] for p in plans]
    return {"samples_ms": times, "median_ms": statistics.median(times),
            "index_names": sorted(set(indexes(plans[-1]["Plan"]))),
            "plan": plans[-1]}


def main():
    if sys.argv[1:] != ["--disposable-fixture"]:
        raise SystemExit("Explicit disposable fixture flag required")
    assert psql("SELECT current_database()") == "ovg066"
    assert psql("SELECT count(*) FROM ovg066_fixture.features") == "1230000"
    report = {"database": psql("SELECT version() || ' / PostGIS ' || PostGIS_Lib_Version()"),
              "method": "SQL only, five EXPLAIN ANALYZE executions per query, synthetic point data, 2 CPU/1GiB, simplified RLS; no HTTP/production claim",
              "tile": {}, "viewport": {}}
    # Compare the existing migration 0045 effect only inside this disposable DB.
    for leakproof in (False, True):
        psql("ALTER FUNCTION geometry_overlaps(geometry,geometry) " + ("LEAKPROOF" if leakproof else "NOT LEAKPROOF"))
        for scale in (30000, 100000, 1000000):
            for mode in ("full", "restricted"):
                key = f"{scale}/{mode}/{leakproof}"
                query = tile(scale)
                report["tile"][key] = measure(query, mode)
                result = scoped(query, mode)
                report["tile"][key]["returned_rows"] = len(result.splitlines()) if result else 0
                report["tile"][key]["ordered_ids_sha256"] = hashlib.sha256(result.encode()).hexdigest()
                if leakproof:
                    previous = report["tile"][f"{scale}/{mode}/False"]
                    assert report["tile"][key]["ordered_ids_sha256"] == previous["ordered_ids_sha256"]
        if leakproof:
            for mode in ("full", "restricted"):
                base = viewport(30000, mode)
                new = viewport(30000, mode, True)
                assert scoped(base, mode) == scoped(new, mode)
                invalid_base = scoped(viewport(30000, mode, False, True), mode)
                assert invalid_base == scoped(viewport(30000, mode, True, True), mode)
                assert invalid_base.endswith("|f|{}"), invalid_base
                report["viewport"][mode] = {"baseline": measure(base, mode), "candidate": measure(new, mode)}
    assert not scoped(tile(100000), org="00000000-0000-0000-0000-000000000008")
    assert not scoped(tile(100000), org="")
    # Restore the extension's original setting in this disposable database.
    psql("ALTER FUNCTION geometry_overlaps(geometry,geometry) NOT LEAKPROOF")
    report["checks"] = {"viewport_result_parity": True, "invalid_outside_bbox_fails_closed": True,
                        "other_tenant_empty": True, "missing_tenant_empty": True}
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
