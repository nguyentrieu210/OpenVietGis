#!/usr/bin/env python3
"""E10 3D Tiles catalog/manifest differential parity."""

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

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/"tests"/"rust-parity"))
sys.path.insert(0,str(ROOT/"tools"/"benchmark"/"rust-platform"))
import benchmark_runner as baseline
import run as parity

API="/api/v1"
def req(c,d):
    if not c: raise AssertionError(d)
@parity.case
def compare(label,e,a,headers=True):
    req(e.status_code==a.status_code,f"{label}: {e.status_code}!={a.status_code}; {e.text}/{a.text}")
    req(parity.difference(e.json(),a.json()) is None,f"{label}: {e.json()} != {a.json()}")
    if headers and e.status_code==200:
        for k in ("cache-control","x-content-type-options"):
            req(e.headers.get(k)==a.headers.get(k),f"{label} {k}")
    print("PASS",label,flush=True)
def main():
 ap=argparse.ArgumentParser();ap.add_argument("--candidate",type=Path,required=True);a=ap.parse_args()
 binary=a.candidate.resolve();req(binary.is_file(),"binary missing")
 admin=os.environ.get("OVG_PARITY_PG_ADMIN_URL",baseline.DEFAULT_ADMIN_PG_URL)
 with tempfile.TemporaryDirectory(prefix="ovg-e10-tilesets-") as d:
  work=Path(d)
  with parity.database(admin) as (owner,runtime):
   art=work/"artifacts";art.mkdir();parity.configure_python(owner,runtime,art)
   baseline.run_alembic_upgrade(owner,runtime)
   f=baseline.seed_benchmark_database(owner,seed=20261126,scales=["1k"],csrf_secret=baseline.DEFAULT_CSRF_SECRET)
   t1,t2=uuid4(),uuid4();base=datetime(2026,11,26,11,12,13,123456,tzinfo=UTC)
   manifest={"asset":{"version":"1.1","tilesetVersion":"demo"},"geometricError":10,
             "root":{"boundingVolume":{"region":[1,0.1,1.1,0.2,0,10]},"geometricError":0,
                     "refine":"REPLACE","content":{"uri":"root.glb"}}}
   with psycopg.connect(owner) as c:
    for tid,name,sec in ((t1,"First",0),(t2,"Second",1)):
     c.execute("""INSERT INTO spatial.tilesets
       (organization_id,project_id,id,name,manifest,byte_size,tile_count,actor_id,created_at,provenance)
       VALUES(%s,%s,%s,%s,%s::jsonb,%s,1,%s,%s,%s::jsonb)""",
       (f["org_a_id"],f["project_a_id"],tid,name,json.dumps(manifest),1234+sec,f["user_a_id"],base+timedelta(seconds=sec),
        json.dumps({"kind":"imported","feature_picking":False})))
   from fastapi.testclient import TestClient
   from openvietgis_api.adapters.persistence.db import get_engine
   from openvietgis_api.main import app
   root=f"{API}/organizations/{f['org_a_id']}/projects/{f['project_a_id']}/3d/tilesets"
   h=parity.session_headers(f["session_a_token"]); foreign=parity.session_headers(f["session_b_token"])
   try:
    with TestClient(app,raise_server_exceptions=False) as oracle, parity.rust_client(binary,runtime,work) as rust:
     compare("E10 tileset catalog",oracle.get(root,headers=h),rust.get(root,headers=h))
     ids=[x["id"] for x in rust.get(root,headers=h).json()["items"]]
     req(ids==[str(t2),str(t1)],f"order {ids}")
     print("PASS E10 tileset catalog order",flush=True)
     compare("E10 tileset manifest",oracle.get(f"{root}/{t1}/tileset.json",headers=h),rust.get(f"{root}/{t1}/tileset.json",headers=h))
     missing=uuid4()
     compare("E10 tileset missing",oracle.get(f"{root}/{missing}/tileset.json",headers=h),rust.get(f"{root}/{missing}/tileset.json",headers=h),headers=False)
     compare("E10 tileset foreign denied",oracle.get(root,headers=foreign),rust.get(root,headers=foreign),headers=False)
   finally:get_engine().dispose()
 print("E10 tileset read parity PASS",flush=True);return 0
if __name__=="__main__":raise SystemExit(main())
