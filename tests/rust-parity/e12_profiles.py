#!/usr/bin/env python3
"""E12 adapter profile differential parity on real isolated PostGIS."""

from __future__ import annotations

import argparse
import os
import sys
import tempfile
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

import psycopg

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/"tests"/"rust-parity"))
sys.path.insert(0,str(ROOT/"tools"/"benchmark"/"rust-platform"))
import benchmark_runner as baseline
import run as parity
from e12_catalog import set_project_role

API="/api/v1"
def req(c,d):
    if not c: raise AssertionError(d)

@parity.case
def compare(label,e,a):
    req(e.status_code==a.status_code,f"{label}: {e.status_code}!={a.status_code}; {e.text} / {a.text}")
    req(parity.difference(e.json(),a.json()) is None,f"{label}: {e.json()} != {a.json()}")
    if e.status_code==200:
        req(e.headers.get("cache-control")==a.headers.get("cache-control")=="no-store",f"{label} no-store")
    print("PASS",label,flush=True)

def add_rule(owner,f,user,layer,actions):
    with psycopg.connect(owner) as c:
        row=c.execute("""SELECT l.id,l.source_id FROM spatial.layers l
          WHERE l.organization_id=%s AND l.project_id=%s AND l.id=%s""",
          (f["org_a_id"],f["project_a_id"],layer)).fetchone()
        req(row is not None,"rule layer missing")
        c.execute("""INSERT INTO gis_access.rules
          (organization_id,project_id,id,user_id,layer_id,source_id,actions,created_at)
          VALUES(%s,%s,%s,%s,%s,%s,%s,%s)""",
          (f["org_a_id"],f["project_a_id"],uuid4(),user,row[0],row[1],actions,datetime.now(UTC)))

def main():
    ap=argparse.ArgumentParser(); ap.add_argument("--candidate",type=Path,required=True); a=ap.parse_args()
    binary=a.candidate.resolve(); req(binary.is_file(),"binary missing")
    admin=os.environ.get("OVG_PARITY_PG_ADMIN_URL",baseline.DEFAULT_ADMIN_PG_URL)
    with tempfile.TemporaryDirectory(prefix="ovg-e12-profile-") as d:
      work=Path(d)
      with parity.database(admin) as (owner,runtime):
        art=work/"artifacts"; art.mkdir(); parity.configure_python(owner,runtime,art)
        baseline.run_alembic_upgrade(owner,runtime)
        f=baseline.seed_benchmark_database(owner,seed=20261126,scales=["1k"],csrf_secret=baseline.DEFAULT_CSRF_SECRET)
        from fastapi.testclient import TestClient
        from openvietgis_api.adapters.persistence.db import get_engine
        from openvietgis_api.main import app
        base=f"{API}/organizations/{f['org_a_id']}/projects/{f['project_a_id']}/integration-hub/adapters"
        h=parity.session_headers(f["session_a_token"])
        try:
          with TestClient(app,raise_server_exceptions=False) as oracle, parity.rust_client(binary,runtime,work) as rust:
            set_project_role(owner,f,"viewer",scoped=False)
            for key in ("qgis","postgis","autocad","ogc_services","arcgis","microstation_v8i"):
              compare(f"E12 profile {key} unscoped",oracle.get(f"{base}/{key}",headers=h),rust.get(f"{base}/{key}",headers=h))
            unknown="does_not_exist"
            compare("E12 profile unknown",oracle.get(f"{base}/{unknown}",headers=h),rust.get(f"{base}/{unknown}",headers=h))

            set_project_role(owner,f,"viewer",scoped=True)
            add_rule(owner,f,f["user_a_id"],f["layers"]["edit_4326"]["layer_id"],["read","export"])
            add_rule(owner,f,f["user_a_id"],f["layers"]["epsg4326_1k"]["layer_id"],["read"])
            e=oracle.get(f"{base}/qgis",headers=h); r=rust.get(f"{base}/qgis",headers=h)
            compare("E12 qgis scoped profile",e,r)
            items=r.json()["layers"]
            req(len(items)==2,f"expected 2 scoped layers, got {len(items)}")
            perms={x["layer_id"]:x["export_permitted"] for x in items}
            req(perms[str(f["layers"]["edit_4326"]["layer_id"])] is True,perms)
            req(perms[str(f["layers"]["epsg4326_1k"]["layer_id"])] is False,perms)
            print("PASS E12 scoped export ACL",flush=True)
        finally: get_engine().dispose()
    print("E12 adapter profile parity PASS",flush=True); return 0
if __name__=="__main__": raise SystemExit(main())
