#!/usr/bin/env python3
"""E09 raster tile/file/delete differential parity on isolated real PostGIS."""

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

def seed_layer(owner,f,label,created):
    rid=uuid4()
    cog=b"II*\x00"+bytes(range(1,65))
    png=b"\x89PNG\r\n\x1a\n"+b"parity-png-"+label.encode()
    meta={"method":"band","width":8,"height":8,"tile_count":1}
    with psycopg.connect(owner) as c:
        c.execute("""INSERT INTO raster.layers
          (organization_id,project_id,id,name,metadata,cog,byte_size,created_at)
          VALUES(%s,%s,%s,%s,%s::jsonb,%s,%s,%s)""",
          (f["org_a_id"],f["project_a_id"],rid,f"Raster {label}",json.dumps(meta),
           cog,len(cog)+len(png),created))
        c.execute("""INSERT INTO raster.tiles
          (organization_id,project_id,layer_id,z,x,y,png)
          VALUES(%s,%s,%s,2,1,2,%s)""",
          (f["org_a_id"],f["project_a_id"],rid,png))
    return str(rid),cog,png

@parity.case
def compare_json(label,e,a):
    req(e.status_code==a.status_code,f"{label}: {e.status_code}!={a.status_code}; {e.text}/{a.text}")
    req(parity.difference(e.json(),a.json()) is None,f"{label}: {e.json()} != {a.json()}")
    print("PASS",label,flush=True)

@parity.case
def compare_bytes(label,e,a,body=None):
    req(e.status_code==a.status_code,f"{label}: {e.status_code}!={a.status_code}; {e.text}/{a.text}")
    req(e.content==a.content,f"{label}: bytes differ")
    if body is not None: req(a.content==body,f"{label}: unexpected body")
    for k in ("content-type","cache-control","x-content-type-options"):
        req(e.headers.get(k)==a.headers.get(k),f"{label} {k}: {e.headers.get(k)} != {a.headers.get(k)}")
    print("PASS",label,flush=True)

def audit_count(owner,f):
    with psycopg.connect(owner) as c:
        return c.execute("""SELECT count(*) FROM audit.audit_events
          WHERE organization_id=%s AND action='raster.layer.delete'""",(f["org_a_id"],)).fetchone()[0]

def main():
    ap=argparse.ArgumentParser();ap.add_argument("--candidate",type=Path,required=True);a=ap.parse_args()
    binary=a.candidate.resolve();req(binary.is_file(),"binary missing")
    admin=os.environ.get("OVG_PARITY_PG_ADMIN_URL",baseline.DEFAULT_ADMIN_PG_URL)
    with tempfile.TemporaryDirectory(prefix="ovg-e09-content-") as d:
      work=Path(d)
      with parity.database(admin) as (owner,runtime):
        art=work/"artifacts";art.mkdir();parity.configure_python(owner,runtime,art)
        baseline.run_alembic_upgrade(owner,runtime)
        f=baseline.seed_benchmark_database(owner,seed=20261126,scales=["1k"],csrf_secret=baseline.DEFAULT_CSRF_SECRET)
        now=datetime(2026,11,26,12,0,0,123456,tzinfo=UTC)
        rid,cog,png=seed_layer(owner,f,"content",now)
        oracle_del,_,_=seed_layer(owner,f,"delete-oracle",now+timedelta(seconds=1))
        rust_del,_,_=seed_layer(owner,f,"delete-rust",now+timedelta(seconds=2))
        from fastapi.testclient import TestClient
        from openvietgis_api.adapters.persistence.db import get_engine
        from openvietgis_api.main import app
        root=f"{API}/organizations/{f['org_a_id']}/projects/{f['project_a_id']}/raster/layers"
        h=parity.session_headers(f["session_a_token"])
        csrf=parity.session_headers(f["session_a_token"],**{"x-openvietgis-csrf":f["csrf_a_token"]})
        foreign=parity.session_headers(f["session_b_token"])
        foreign_csrf=parity.session_headers(f["session_b_token"],**{"x-openvietgis-csrf":f["csrf_b_token"]})
        try:
          with TestClient(app,raise_server_exceptions=False) as oracle, parity.rust_client(binary,runtime,work) as rust:
            e=oracle.get(f"{root}/{rid}/tiles/2/1/2.png",headers=h);r=rust.get(f"{root}/{rid}/tiles/2/1/2.png",headers=h)
            compare_bytes("E09 raster tile",e,r,png)
            for path,label in (("2/4/0.png","invalid tile coordinate"),("21/0/0.png","invalid tile zoom"),("2/1/1.png","missing tile")):
                compare_json(f"E09 {label}",oracle.get(f"{root}/{rid}/tiles/{path}",headers=h),rust.get(f"{root}/{rid}/tiles/{path}",headers=h))

            e=oracle.get(f"{root}/{rid}/file",headers=h);r=rust.get(f"{root}/{rid}/file",headers=h)
            compare_bytes("E09 full raster file",e,r,cog)
            for k in ("accept-ranges","content-disposition"):
                req(e.headers.get(k)==r.headers.get(k),f"full file {k}")
            ranges=[
                ("bytes=0-3",cog[0:4]),
                ("bytes=4-",cog[4:]),
                ("bytes=-4",cog[-4:]),
                ("bytes=0-999",cog),
                ("bytes=-999",cog),
            ]
            for hdr,expected in ranges:
                eh={"range":hdr,**h};rh={"range":hdr,**h}
                e=oracle.get(f"{root}/{rid}/file",headers=eh);r=rust.get(f"{root}/{rid}/file",headers=rh)
                compare_bytes(f"E09 range {hdr}",e,r,expected)
                req(e.headers.get("content-range")==r.headers.get("content-range"),f"{hdr} content-range")
                req(e.status_code==206,f"{hdr} expected 206")
            for hdr in ("bytes=999-1000","bytes=-0","bytes=abc-def","bytes=1-2,4-5"):
                eh={"range":hdr,**h}
                e=oracle.get(f"{root}/{rid}/file",headers=eh);r=rust.get(f"{root}/{rid}/file",headers=eh)
                req(e.status_code==r.status_code==416,f"{hdr}: {e.status_code}/{r.status_code}")
                req(e.headers.get("content-range")==r.headers.get("content-range"),f"{hdr} content-range")
                print("PASS E09 invalid range",hdr,flush=True)

            missing=str(uuid4())
            compare_json("E09 missing raster file",oracle.get(f"{root}/{missing}/file",headers=h),rust.get(f"{root}/{missing}/file",headers=h))
            compare_json("E09 foreign raster denied",oracle.get(f"{root}/{rid}/file",headers=foreign),rust.get(f"{root}/{rid}/file",headers=foreign))

            before=audit_count(owner,f)
            e=oracle.delete(f"{root}/{oracle_del}",headers=csrf);r=rust.delete(f"{root}/{rust_del}",headers=csrf)
            req(e.status_code==r.status_code==204,(e.status_code,e.text,r.status_code,r.text))
            req(e.content==r.content==b"","delete 204 body")
            req(audit_count(owner,f)==before+2,"delete audit rows")
            print("PASS E09 raster delete + audit",flush=True)

            compare_json("E09 delete missing",oracle.delete(f"{root}/{oracle_del}",headers=csrf),rust.delete(f"{root}/{rust_del}",headers=csrf))
            no_csrf=rust.delete(f"{root}/{rid}",headers=h)
            req(no_csrf.status_code==403,no_csrf.text)
            print("PASS E09 delete CSRF fail-closed",flush=True)
            compare_json("E09 foreign delete denied",oracle.delete(f"{root}/{rid}",headers=foreign_csrf),rust.delete(f"{root}/{rid}",headers=foreign_csrf))
        finally:get_engine().dispose()
    print("E09 raster content/delete parity PASS",flush=True);return 0
if __name__=="__main__":raise SystemExit(main())
