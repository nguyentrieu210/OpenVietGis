"""Isolated PostGIS query experiment. Run only against the disposable fixture."""
import hashlib
import json
import math
import platform
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone

CONTAINER = "ovg066-postgis-fixture"
TABLE = "ovg066_fixture.features"
ORG = "00000000-0000-0000-0000-000000000001"
PROJECT = "00000000-0000-0000-0000-000000000002"
BOX = "ST_MakeEnvelope(105.51,20.51,105.52,20.52,4326)"
DOMAIN = "ST_CoveredBy(geometry, ST_MakeEnvelope(-180,-85.0511287798066,180,85.0511287798066,4326))"
VALID = f"content_revision=repeat('a',64) AND schema_revision=repeat('b',64) AND geometry_state='present' AND ST_SRID(geometry)=4326 AND ST_GeometryType(geometry)='ST_Point' AND {DOMAIN}"


def utc_now():
    return datetime.now(timezone.utc).isoformat()


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


def percentile(values, quantile):
    ordered = sorted(values)
    return ordered[math.ceil(quantile * len(ordered)) - 1]


def measure(query, mode, count):
    plans = [json.loads(scoped("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) " + query, mode))[0]
             for _ in range(count)]
    times = [plan["Execution Time"] for plan in plans]
    return {"sample_count": len(times), "samples_ms": times,
            "median_ms": statistics.median(times), "p95_ms": percentile(times, .95),
            "p99_ms": percentile(times, .99),
            "index_names": sorted(set(indexes(plans[-1]["Plan"]))),
            "representative_plan": plans[-1]}


def main():
    args = sys.argv[1:]
    if not args or args[0] != "--disposable-fixture" or len(args) > 2:
        raise SystemExit("Usage: postgis-viewport.py --disposable-fixture [SAMPLES>=30]")
    count = int(args[1]) if len(args) == 2 else 30
    if not 30 <= count <= 100:
        raise SystemExit("SAMPLES must be an integer from 30 to 100 for p95 reporting")
    if psql("SELECT current_database()") != "ovg066":
        raise SystemExit("Refusing to run outside the ovg066 disposable database")
    if psql("SELECT count(*) FROM ovg066_fixture.features") != "1230000":
        raise SystemExit("Disposable fixture row-count check failed")
    source_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    started_at = utc_now()
    started = time.perf_counter()
    report = {"source_sha": source_sha,
              "database": psql("SELECT version() || ' / PostGIS ' || PostGIS_Lib_Version()"),
              "runtime": {"python": platform.python_version(), "platform": platform.platform()},
              "sample_count_per_query": count,
              "method": f"SQL only, {count} EXPLAIN ANALYZE executions per query, synthetic point data, 2 CPU/1GiB, simplified RLS; no HTTP/production claim",
              "started_at": started_at,
              "tile": {}, "viewport": {}}
    for leakproof in (False, True):
        psql("ALTER FUNCTION geometry_overlaps(geometry,geometry) " + ("LEAKPROOF" if leakproof else "NOT LEAKPROOF"))
        for scale in (30000, 100000, 1000000):
            for mode in ("full", "restricted"):
                key = f"{scale}/{mode}/{leakproof}"
                query = tile(scale)
                report["tile"][key] = measure(query, mode, count)
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
                report["viewport"][mode] = {"baseline": measure(base, mode, count),
                                            "candidate": measure(new, mode, count)}
    assert not scoped(tile(100000), org="00000000-0000-0000-0000-000000000008")
    assert not scoped(tile(100000), org="")
    psql("ALTER FUNCTION geometry_overlaps(geometry,geometry) NOT LEAKPROOF")
    report["checks"] = {"viewport_result_parity": True, "invalid_outside_bbox_fails_closed": True,
                        "other_tenant_empty": True, "missing_tenant_empty": True}
    report["completed_at"] = utc_now()
    report["duration_seconds"] = time.perf_counter() - started
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
