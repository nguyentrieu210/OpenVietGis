"""Authenticated Rust/PostGIS test. Only a fresh spec065 fixture DB at loopback 54332.
Run with the repository's migration Python environment and a newly built Rust binary.
Never pass a live DB URL. The companion shell script owns the disposable container.
"""
import argparse
import os
import sys
from pathlib import Path
from tempfile import TemporaryDirectory

import psycopg

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests/rust-parity"))
from run import baseline, configure_python, project_path, rust_client, session_headers

OWNER = "postgresql://openvietgis:openvietgis@127.0.0.1:54332/openvietgis_bench_spec065_query"
RUNTIME = "postgresql://openvietgis_runtime:openvietgis-runtime-dev@127.0.0.1:54332/openvietgis_bench_spec065_query"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", required=True, type=Path)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    if not binary.is_file():
        parser.error("candidate binary missing")
    baseline.assert_safe_bench_db(OWNER, isolated_port=54332)
    with psycopg.connect(OWNER) as conn:
        assert conn.execute("SELECT current_database()").fetchone()[0] == "openvietgis_bench_spec065_query"
        assert conn.execute("SELECT to_regclass('spatial.reference_features')").fetchone()[0] is None, "refusing nonfresh DB"
    with TemporaryDirectory(prefix="ovg065-query-") as directory:
        configure_python(OWNER, RUNTIME, Path(directory))
        baseline.run_alembic_upgrade(OWNER, RUNTIME)
        f = baseline.seed_benchmark_database(OWNER, seed=65001, scales=["1k", "30k"],
                                           csrf_secret=baseline.DEFAULT_CSRF_SECRET, isolated_port=54332)
        target = f["layers"]["epsg4326_1k"]
        large = f["layers"]["epsg4326_30k"]
        # Explicitly synthesize geometry-absent row, retaining original pins for this test only.
        with psycopg.connect(OWNER) as conn:
            row = conn.execute("""UPDATE spatial.reference_features SET geometry=NULL, geometry_state='absent'
                WHERE source_id=%s AND feature_id=(SELECT min(feature_id) FROM spatial.reference_features WHERE source_id=%s)
                RETURNING feature_id""", (target["source_id"],target["source_id"])).fetchone()
            missing = row[0]
        with rust_client(binary, RUNTIME, Path(directory)) as c:
            path = project_path(f["org_a_id"], f["project_a_id"])
            auth = session_headers(f["session_a_token"], **{"x-openvietgis-csrf": f["csrf_a_token"]})
            foreign = session_headers(f["session_b_token"], **{"x-openvietgis-csrf": f["csrf_b_token"]})
            def request(layer, body, expected=200, headers=auth):
                r=c.post(path+"/layers/"+layer["layer_id"]+"/query", json=body, headers=headers)
                assert r.status_code==expected, (r.status_code, r.text[:500])
                return r.json()
            def pinned(layer):
                return {"attribute_only":True,"limit":100,"expected_content_revision":layer["content_revision"],"expected_schema_revision":layer["schema_revision"]}
            q=pinned(target)
            first=request(target,q)
            assert first["attribute_only"] is True and first["total_count"]==1000
            assert first["items"][0]["feature_id"]==missing and first["items"][0]["geometry_state"]=="absent"
            assert all(r["geometry"] is None for r in first["items"])
            second=request(target,q|{"offset":100,"expected_query_fingerprint":first["query_fingerprint"]})
            assert len(second["items"])==100 and not set(first["selection_feature_ids"])&set(second["selection_feature_ids"])
            request(target,q|{"offset":100},409)
            request(target,q|{"expected_content_revision":"f"*64},409)
            request(target,q|{"expected_query_fingerprint":"f"*64},409)
            request(target,q|{"spatial_filter":{"predicate":"intersects"}},422)
            request(target,{"limit":100},422)
            request(target,q,404,foreign)
            request(target,q,401,{})
            result=request(target,q|{"attribute_filter":{"field":"metric_val","op":"gte","value":1000},"sort_field":"metric_val","sort_direction":"desc"})
            vals=[r["attributes"]["metric_val"] for r in result["items"]]
            assert vals and vals==sorted(vals,reverse=True) and min(vals)>=1000
            print("PASS authenticated attribute-only missing geometry, typed filter/sort, revision pins and foreign tenant denial",flush=True)
            q=pinned(large)
            start=request(large,q)
            end=request(large,q|{"offset":29900,"expected_query_fingerprint":start["query_fingerprint"]})
            assert end["total_count"]==30000 and len(end["items"])==100 and not end["has_more"]
            request(large,{"offset":29900,"limit":100},422)
            print("PASS 30,000-record source, final page offset 29900, legacy offset unchanged",flush=True)
            legacy=request(large,{"limit":1})
            modern=request(large,q|{"limit":1})
            assert legacy["query_fingerprint"]!=modern["query_fingerprint"] and legacy["items"][0]["geometry"] is not None
            print("PASS old spatial query response and new fingerprint isolation",flush=True)


if __name__=="__main__":
    main()
