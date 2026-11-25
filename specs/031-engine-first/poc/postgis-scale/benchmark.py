"""R031 isolated SQL microbenchmark. Run only on a fresh disposable database."""
from __future__ import annotations

import json
import math
import os
import statistics
import time
from urllib.parse import urlsplit

import psycopg

URL = os.environ["OVG_SCALE_TEST_DSN"]
target = urlsplit(URL)
if target.hostname != "127.0.0.1" or target.path != "/openvietgis_scale_test":
    raise SystemExit("Only a dedicated loopback scale-test database is permitted.")

BASE = """
WITH bounds AS (SELECT ST_TileEnvelope(%s::int,%s::int,%s::int) AS b),
source AS (SELECT feature_id, ST_Transform(geometry,3857) AS g,b
 FROM r031_scale_points CROSS JOIN bounds
 WHERE geometry && ST_Transform(b,4326))
"""
CLUSTER = BASE + """
, clusters AS (SELECT ST_SnapToGrid(g,(ST_XMax(b)-ST_XMin(b))/32) AS p,
 ST_Centroid(ST_Collect(g)) AS g,count(*) AS count,min(feature_id) AS fid,b
 FROM source GROUP BY p,b)
, tile AS (SELECT CASE WHEN count=1 THEN fid ELSE '' END AS feature_id,
 count AS cluster_count,ST_AsMVTGeom(g,b,4096,64,true) AS geom FROM clusters)
 SELECT ST_AsMVT(tile,'features',4096,'geom') FROM tile
"""
DETAIL = BASE + """
, tile AS (SELECT feature_id,1 AS cluster_count,
 ST_AsMVTGeom(ST_SimplifyPreserveTopology(g,(ST_XMax(b)-ST_XMin(b))/4096),
 b,4096,64,true) AS geom FROM source ORDER BY feature_id LIMIT 20000)
 SELECT ST_AsMVT(tile,'features',4096,'geom') FROM tile
"""


def tile(zoom: int) -> tuple[int, int, int]:
    n = 2**zoom
    x = int((106.9 + 180) / 360 * n)
    y = int((1 - math.asinh(math.tan(math.radians(11.5))) / math.pi) / 2 * n)
    return zoom, x, y


def main() -> None:
    out = []
    with psycopg.connect(URL, autocommit=True) as conn:
        conn.execute("SET statement_timeout='30s'")
        version = conn.execute("SELECT postgis_full_version()").fetchone()[0]
        # Never drop or modify existing tables; a second invocation must use a fresh DB.
        conn.execute("CREATE TABLE r031_scale_points(feature_id text PRIMARY KEY, geometry geometry(Point,4326))")
        conn.execute("CREATE INDEX r031_scale_geometry_idx ON r031_scale_points USING gist(geometry)")
        for n in (10_000, 100_000, 1_000_000):
            previous = out[-1]["features"] if out else 0
            conn.execute(
                """INSERT INTO r031_scale_points
                SELECT 'f-'||i,ST_SetSRID(ST_MakePoint(
                  106.8+((i::bigint*7919)%%1000000)/5000000.0,
                  11.4+((i::bigint*104729)%%1000000)/5000000.0),4326)
                FROM generate_series(%s::bigint,%s::bigint) i""",
                (previous+1,n),
            )
            conn.execute("ANALYZE r031_scale_points")
            result = {"features": n, "scenarios": []}
            for name, sql, z in (("cluster_z14", CLUSTER, 14), ("detail_z17", DETAIL, 17)):
                params = tile(z)
                sizes = []
                samples = []
                for i in range(35):
                    start = time.perf_counter()
                    data = conn.execute(sql, params).fetchone()[0]
                    elapsed = (time.perf_counter()-start)*1000
                    if i >= 5:
                        sizes.append(len(data))
                        samples.append(elapsed)
                assert min(sizes)>0 and len(set(sizes))==1
                samples.sort()
                result["scenarios"].append({
                    "name":name,"samples":len(samples),"p50_ms":round(statistics.median(samples),3),
                    "p95_ms":round(samples[math.ceil(len(samples)*.95)-1],3),"tile_bytes":sizes[0],
                })
            out.append(result)
            print(json.dumps(result), flush=True)
    print(json.dumps({"version":version,"scope":"synthetic indexed SQL and local tile bytes, no RLS/HTTP/browser/WAN","results":out},indent=2))


if __name__ == "__main__":
    main()
