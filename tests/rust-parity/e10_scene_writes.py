#!/usr/bin/env python3
"""E10 saved-scene write/open differential parity."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tempfile
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
def norm_scene(body):
    out=json.loads(json.dumps(body))
    if "id" in out: out["id"]="<id>"
    if "created_at" in out: out["created_at"]="<created_at>"
    return out
def norm_open(body):
    out=json.loads(json.dumps(body))
    if "scene" in out:
        out["scene"]=norm_scene(out["scene"])
    return out
def compare(label,e,a,norm=lambda x:x,headers=False):
    req(e.status_code==a.status_code,f"{label}: {e.status_code}!={a.status_code}; {e.text}/{a.text}")
    eb=norm(e.json());ab=norm(a.json())
    req(parity.difference(eb,ab) is None,f"{label}: {eb} != {ab}")
    if headers and e.status_code==200:
        for k in ("cache-control","x-content-type-options"):
            req(e.headers.get(k)==a.headers.get(k),f"{label} {k}")
    print("PASS",label,flush=True)
    return a.json()

def seed_model(owner,f):
    mid=uuid4(); payload=b"scene-model"; sha=hashlib.sha256(payload).hexdigest()
    profile={"source_id":str(uuid4()),"feature_id":"f1","scale":1.0,
             "rotation_deg":0.0,"offset_m":0.0,"dem_id":None,
             "vertical_reference":"local","reference_label":""}
    with psycopg.connect(owner) as c:
        c.execute("""INSERT INTO spatial.model_assets
          (organization_id,project_id,id,filename,format,sha256,byte_size,payload,actor_id)
          VALUES(%s,%s,%s,'scene.glb','glb',%s,%s,%s,%s)""",
          (f["org_a_id"],f["project_a_id"],mid,sha,len(payload),payload,f["user_a_id"]))
        c.execute("""INSERT INTO spatial.model_placements
          (organization_id,project_id,model_id,version,profile,actor_id)
          VALUES(%s,%s,%s,1,%s::jsonb,%s)""",
          (f["org_a_id"],f["project_a_id"],mid,json.dumps(profile),f["user_a_id"]))
    return str(mid)
def audit_count(owner,f):
    with psycopg.connect(owner) as c:
        return c.execute("""SELECT count(*) FROM audit.audit_events
          WHERE organization_id=%s AND action='3d.scene.save'""",(f["org_a_id"],)).fetchone()[0]

def main():
    ap=argparse.ArgumentParser();ap.add_argument("--candidate",type=Path,required=True);a=ap.parse_args()
    binary=a.candidate.resolve();req(binary.is_file(),"binary missing")
    admin=os.environ.get("OVG_PARITY_PG_ADMIN_URL",baseline.DEFAULT_ADMIN_PG_URL)
    with tempfile.TemporaryDirectory(prefix="ovg-e10-scene-write-") as d:
      work=Path(d)
      with parity.database(admin) as (owner,runtime):
        art=work/"artifacts";art.mkdir();parity.configure_python(owner,runtime,art)
        baseline.run_alembic_upgrade(owner,runtime)
        f=baseline.seed_benchmark_database(owner,seed=20261126,scales=["1k"],csrf_secret=baseline.DEFAULT_CSRF_SECRET)
        model_id=seed_model(owner,f)
        from fastapi.testclient import TestClient
        from openvietgis_api.adapters.persistence.db import get_engine
        from openvietgis_api.main import app
        root=f"{API}/organizations/{f['org_a_id']}/projects/{f['project_a_id']}/3d/scenes"
        h=parity.session_headers(f["session_a_token"])
        csrf=parity.session_headers(f["session_a_token"],**{"x-openvietgis-csrf":f["csrf_a_token"]})
        foreign=parity.session_headers(f["session_b_token"],**{"x-openvietgis-csrf":f["csrf_b_token"]})
        payload={"name":"  Scene parity  ","models":[{"id":model_id,"version":1,"visible":True}],
                 "camera":{"crs_code":4326,"longitude":106.0,"latitude":10.0,"zoom":12.0,
                           "bearing":5.0,"pitch":30.0},"dem":None,"footprint":None}
        try:
          with TestClient(app,raise_server_exceptions=False) as oracle, parity.rust_client(binary,runtime,work) as rust:
            before=audit_count(owner,f)
            e=oracle.post(root,json=payload,headers=csrf);r=rust.post(root,json=payload,headers=csrf)
            rb=compare("E10 scene create",e,r,norm_scene)
            req(e.status_code==201,e.text); oid=e.json()["id"];rid=rb["id"]
            req(rb["name"]=="Scene parity",rb)
            req(audit_count(owner,f)==before+2,"create audit")
            print("PASS E10 scene create audit",flush=True)

            revise=dict(payload); revise["name"]="Scene v2"; revise["expected_version"]=1
            e=oracle.post(f"{root}/{oid}/versions",json=revise,headers=csrf)
            r=rust.post(f"{root}/{rid}/versions",json=revise,headers=csrf)
            rb=compare("E10 scene revise",e,r,norm_scene)
            req(rb["version"]==2,rb)
            req(audit_count(owner,f)==before+4,"revise audit")
            print("PASS E10 scene revise audit",flush=True)

            stale=dict(revise); stale["name"]="Stale"
            compare("E10 scene stale revise",
                    oracle.post(f"{root}/{oid}/versions",json=stale,headers=csrf),
                    rust.post(f"{root}/{rid}/versions",json=stale,headers=csrf))

            compare("E10 scene history after writes",
                    oracle.get(f"{root}/{oid}/versions",headers=h),
                    rust.get(f"{root}/{rid}/versions",headers=h),
                    lambda b:{"items":[norm_scene(x) for x in b["items"]]},headers=True)

            e=oracle.get(f"{root}/{oid}/versions/2",headers=h);r=rust.get(f"{root}/{rid}/versions/2",headers=h)
            compare("E10 open pinned scene",e,r,norm_open,headers=True)

            bad=dict(payload); bad["name"]="Missing model"; bad["models"]=[{"id":str(uuid4()),"version":1}]
            compare("E10 scene missing pinned model",
                    oracle.post(root,json=bad,headers=csrf),rust.post(root,json=bad,headers=csrf))

            missing=str(uuid4())
            revise_missing=dict(revise);revise_missing["expected_version"]=1
            compare("E10 scene revise missing",
                    oracle.post(f"{root}/{missing}/versions",json=revise_missing,headers=csrf),
                    rust.post(f"{root}/{missing}/versions",json=revise_missing,headers=csrf))

            no_csrf=rust.post(root,json=payload,headers=h)
            req(no_csrf.status_code==403,no_csrf.text);print("PASS E10 scene CSRF fail-closed",flush=True)
            compare("E10 scene foreign denied",
                    oracle.post(root,json=payload,headers=foreign),
                    rust.post(root,json=payload,headers=foreign))
        finally:get_engine().dispose()
    print("E10 scene write/open parity PASS",flush=True);return 0
if __name__=="__main__":raise SystemExit(main())
