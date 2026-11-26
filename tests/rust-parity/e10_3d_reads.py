#!/usr/bin/env python3
"""E10 3D model/scene catalog differential parity."""

from __future__ import annotations
import argparse, json, os, sys, tempfile
from datetime import UTC, datetime, timedelta
from pathlib import Path
from uuid import uuid4
import psycopg

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/"tests"/"rust-parity"))
sys.path.insert(0,str(ROOT/"tools"/"benchmark"/"rust-platform"))
import run as parity
import benchmark_runner as baseline
API="/api/v1"

def require(c,d):
    if not c: raise AssertionError(d)

def compare(label,e,a):
    require(e.status_code==a.status_code,f"{label}: {e.status_code}!={a.status_code}; {e.text} / {a.text}")
    mismatch=parity.difference(e.json(),a.json())
    require(mismatch is None,f"{label}: {mismatch}")
    for k in ("cache-control","x-content-type-options"):
        require(e.headers.get(k)==a.headers.get(k),f"{label} header {k}")
    print("PASS",label,flush=True)

def seed(owner,fixture):
    base=datetime(2026,11,26,9,0,0,123456,tzinfo=UTC)
    m1,m2=uuid4(),uuid4()
    s1,s2=uuid4(),uuid4()
    with psycopg.connect(owner) as c:
        for mid,name,fmt in ((m1,"one.glb","glb"),(m2,"two.gltf","gltf")):
            payload=(b"x" if fmt=="glb" else b"{}")
            import hashlib
            c.execute("""INSERT INTO spatial.model_assets
              (organization_id,project_id,id,filename,format,sha256,byte_size,payload,actor_id,created_at)
              VALUES(%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)""",
              (fixture["org_a_id"],fixture["project_a_id"],mid,name,fmt,
               hashlib.sha256(payload).hexdigest(),len(payload),payload,fixture["user_a_id"],base))
        for mid,ver,sec in ((m1,1,0),(m1,2,1),(m2,1,2)):
            c.execute("""INSERT INTO spatial.model_placements
              (organization_id,project_id,model_id,version,profile,actor_id,created_at)
              VALUES(%s,%s,%s,%s,%s::jsonb,%s,%s)""",
              (fixture["org_a_id"],fixture["project_a_id"],mid,ver,
               json.dumps({"source_id":str(uuid4()),"feature_id":f"f{ver}","scale":1.0,
                           "rotation_deg":0.0,"offset_m":0.0,"dem_id":None,
                           "vertical_reference":"local","reference_label":""}),
               fixture["user_a_id"],base+timedelta(seconds=sec)))
        profile={"models":[{"id":str(m1),"version":2,"visible":True}],
                 "camera":{"crs_code":4326,"longitude":106.0,"latitude":10.0,
                           "zoom":12.0,"bearing":0.0,"pitch":30.0},
                 "dem":None,"footprint":None}
        for sid,ver,name,sec in ((s1,1,"Scene A v1",3),(s1,2,"Scene A v2",4),(s2,1,"Scene B",5)):
            c.execute("""INSERT INTO spatial.scene_versions
             (organization_id,project_id,scene_id,version,name,profile,actor_id,created_at)
             VALUES(%s,%s,%s,%s,%s,%s::jsonb,%s,%s)""",
             (fixture["org_a_id"],fixture["project_a_id"],sid,ver,name,json.dumps(profile),
              fixture["user_a_id"],base+timedelta(seconds=sec)))
    return m1,m2,s1,s2

def main():
    ap=argparse.ArgumentParser(); ap.add_argument("--candidate",type=Path,required=True); args=ap.parse_args()
    binary=args.candidate.resolve(); require(binary.is_file(),"binary missing")
    admin=os.environ.get("OVG_PARITY_PG_ADMIN_URL",baseline.DEFAULT_ADMIN_PG_URL)
    with tempfile.TemporaryDirectory(prefix="ovg-e10-3d-") as d:
      work=Path(d)
      with parity.database(admin) as (owner,runtime):
        art=work/"artifacts"; art.mkdir(); parity.configure_python(owner,runtime,art)
        baseline.run_alembic_upgrade(owner,runtime)
        fixture=baseline.seed_benchmark_database(owner,seed=20261126,scales=["1k"],csrf_secret=baseline.DEFAULT_CSRF_SECRET)
        m1,m2,s1,s2=seed(owner,fixture)
        from fastapi.testclient import TestClient
        from openvietgis_api.adapters.persistence.db import get_engine
        from openvietgis_api.main import app
        base=f"{API}/organizations/{fixture['org_a_id']}/projects/{fixture['project_a_id']}/3d"
        h=parity.session_headers(fixture["session_a_token"]); foreign=parity.session_headers(fixture["session_b_token"])
        try:
          with TestClient(app,raise_server_exceptions=False) as oracle, parity.rust_client(binary,runtime,work) as rust:
            compare("E10 3D model latest",oracle.get(f"{base}/models",headers=h),rust.get(f"{base}/models",headers=h))
            items=rust.get(f"{base}/models",headers=h).json()["items"]
            latest={x["id"]:x["version"] for x in items}
            require(latest[str(m1)]==2 and latest[str(m2)]==1,f"latest {latest}")
            print("PASS E10 3D model latest version",flush=True)
            compare("E10 3D model history",oracle.get(f"{base}/models/{m1}/placements",headers=h),rust.get(f"{base}/models/{m1}/placements",headers=h))
            require([x["version"] for x in rust.get(f"{base}/models/{m1}/placements",headers=h).json()["items"]]==[2,1],"model history order")
            compare("E10 3D missing model",oracle.get(f"{base}/models/{uuid4()}/placements",headers=h),rust.get(f"{base}/models/{uuid4()}/placements",headers=h))
            compare("E10 3D scene latest",oracle.get(f"{base}/scenes",headers=h),rust.get(f"{base}/scenes",headers=h))
            scenes={x["id"]:x["version"] for x in rust.get(f"{base}/scenes",headers=h).json()["items"]}
            require(scenes[str(s1)]==2 and scenes[str(s2)]==1,f"scenes {scenes}")
            print("PASS E10 3D scene latest version",flush=True)
            compare("E10 3D scene history",oracle.get(f"{base}/scenes/{s1}/versions",headers=h),rust.get(f"{base}/scenes/{s1}/versions",headers=h))
            require([x["version"] for x in rust.get(f"{base}/scenes/{s1}/versions",headers=h).json()["items"]]==[2,1],"scene history order")
            compare("E10 3D foreign tenant denied",oracle.get(f"{base}/models",headers=foreign),rust.get(f"{base}/models",headers=foreign))
        finally: get_engine().dispose()
    print("E10 3D read parity PASS",flush=True); return 0
if __name__=="__main__": raise SystemExit(main())
