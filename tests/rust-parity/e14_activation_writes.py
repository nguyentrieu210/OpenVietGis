#!/usr/bin/env python3
"""E14 extension activation lifecycle differential parity on isolated PostGIS."""

from __future__ import annotations

import argparse
import json
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

def manifest(owner,f,key,version,*,partner=False,enabled=True,capabilities=None):
    mid=uuid4(); now=datetime.now(UTC)
    if partner:
        permissions=[];commands=[];schemas={};caps=capabilities or ["host_context","map_view"]
        ui=[{"id":"partner_panel","slot":"project_panel","label":"Partner",
             "route_suffix":"/partner","runtime":"sandboxed_iframe",
             "url":"https://partner.example.com/app","sdk_version":"1.0"}]
    else:
        permissions=[f"ext.{key}.read"];commands=[];caps=capabilities or []
        schemas={"thing":{"fields":{"name":{"type":"text"}}}};ui=[]
    with psycopg.connect(owner) as c:
        c.execute("""INSERT INTO extensions.manifests
          (id,organization_id,extension_key,version,api_compatibility,display_name,
           description,permissions,commands,capabilities,entity_schemas,enabled,
           registered_by_actor_id,created_at,ui_contributions)
          VALUES(%s,%s,%s,%s,'1.x',%s,'',%s::jsonb,%s::jsonb,%s::jsonb,%s::jsonb,%s,%s,%s,%s::jsonb)""",
          (mid,f["org_a_id"],key,version,key,json.dumps(permissions),json.dumps(commands),
           json.dumps(caps),json.dumps(schemas),enabled,f["user_a_id"],now,json.dumps(ui)))
    return str(mid)

def norm_act(body,*,revision=False):
    out=json.loads(json.dumps(body))
    for k in ("id","manifest_id","extension_key","activated_at","disabled_at","updated_at"):
        if k in out: out[k]=f"<{k}>"
    if revision and "revision" in out: out["revision"]="<revision>"
    return out

@parity.case
def compare(label,e,a,norm=lambda x:x):
    req(e.status_code==a.status_code,f"{label}: {e.status_code}!={a.status_code}; {e.text}/{a.text}")
    eb=norm(e.json());ab=norm(a.json())
    req(parity.difference(eb,ab) is None,f"{label}: {eb} != {ab}")
    print("PASS",label,flush=True)
    return a.json()

def counts(owner,activation_id):
    with psycopg.connect(owner) as c:
        events=c.execute("SELECT count(*) FROM extensions.activation_events WHERE activation_id=%s",(activation_id,)).fetchone()[0]
        audits=c.execute("""SELECT count(*) FROM audit.audit_events
          WHERE resource_type='extension_activation' AND resource_id=%s""",(activation_id,)).fetchone()[0]
    return events,audits

def main():
    ap=argparse.ArgumentParser();ap.add_argument("--candidate",type=Path,required=True);a=ap.parse_args()
    binary=a.candidate.resolve();req(binary.is_file(),"binary missing")
    admin=os.environ.get("OVG_PARITY_PG_ADMIN_URL",baseline.DEFAULT_ADMIN_PG_URL)
    with tempfile.TemporaryDirectory(prefix="ovg-e14-write-") as d:
      work=Path(d)
      with parity.database(admin) as (owner,runtime):
        art=work/"artifacts";art.mkdir();parity.configure_python(owner,runtime,art)
        baseline.run_alembic_upgrade(owner,runtime)
        f=baseline.seed_benchmark_database(owner,seed=20261126,scales=["1k"],csrf_secret=baseline.DEFAULT_CSRF_SECRET)
        # Paired custom contracts.
        o10=manifest(owner,f,"demo_oracle","1.0.0");o11=manifest(owner,f,"demo_oracle","1.1.0");o12=manifest(owner,f,"demo_oracle","1.2.0",capabilities=["changed"])
        r10=manifest(owner,f,"demo_rust","1.0.0");r11=manifest(owner,f,"demo_rust","1.1.0");r12=manifest(owner,f,"demo_rust","1.2.0",capabilities=["changed"])
        op=manifest(owner,f,"partner_oracle","1.0.0",partner=True)
        rp=manifest(owner,f,"partner_rust","1.0.0",partner=True)

        from fastapi.testclient import TestClient
        from openvietgis_api.adapters.persistence.db import get_engine
        from openvietgis_api.main import app
        base=f"{API}/organizations/{f['org_a_id']}/projects/{f['project_a_id']}/extensions"
        org=f"{API}/organizations/{f['org_a_id']}"
        h=parity.session_headers(f["session_a_token"])
        csrf=parity.session_headers(f["session_a_token"],**{"x-openvietgis-csrf":f["csrf_a_token"]})
        try:
          with TestClient(app,raise_server_exceptions=False) as oracle, parity.rust_client(binary,runtime,work) as rust:
            e=oracle.post(f"{base}/activations",json={"manifest_id":o10},headers=csrf)
            r=rust.post(f"{base}/activations",json={"manifest_id":r10},headers=csrf)
            rb=compare("E14 custom activate",e,r,norm_act)
            req(e.status_code==r.status_code==201,(e.text,r.text)); oid=e.json()["id"];rid=rb["id"]

            # Implicit version replacement is blocked.
            compare("E14 implicit replacement blocked",
                    oracle.post(f"{base}/activations",json={"manifest_id":o11},headers=csrf),
                    rust.post(f"{base}/activations",json={"manifest_id":r11},headers=csrf))

            # Forward same-contract transition.
            e=oracle.post(f"{base}/activations/{oid}/transition",
                          json={"expected_manifest_id":o10,"target_manifest_id":o11},headers=csrf)
            r=rust.post(f"{base}/activations/{rid}/transition",
                        json={"expected_manifest_id":r10,"target_manifest_id":r11},headers=csrf)
            compare("E14 transition 1.0 to 1.1",e,r,norm_act)

            # Stale expected version after transition.
            compare("E14 stale transition",
                    oracle.post(f"{base}/activations/{oid}/transition",
                                json={"expected_manifest_id":o10,"target_manifest_id":o11},headers=csrf),
                    rust.post(f"{base}/activations/{rid}/transition",
                              json={"expected_manifest_id":r10,"target_manifest_id":r11},headers=csrf))

            # Contract change requires migration.
            compare("E14 changed-contract transition blocked",
                    oracle.post(f"{base}/activations/{oid}/transition",
                                json={"expected_manifest_id":o11,"target_manifest_id":o12},headers=csrf),
                    rust.post(f"{base}/activations/{rid}/transition",
                              json={"expected_manifest_id":r11,"target_manifest_id":r12},headers=csrf))

            # Disable and idempotent disable.
            e=oracle.post(f"{base}/demo_oracle/disable",headers=csrf)
            r=rust.post(f"{base}/demo_rust/disable",headers=csrf)
            compare("E14 disable",e,r,norm_act)
            e2=oracle.post(f"{base}/demo_oracle/disable",headers=csrf)
            r2=rust.post(f"{base}/demo_rust/disable",headers=csrf)
            compare("E14 disable idempotent",e2,r2,norm_act)
            req(e.json()["revision"]==e2.json()["revision"]==2,(e.text,e2.text))
            req(r.json()["revision"]==r2.json()["revision"]==2,(r.text,r2.text))

            # Legacy deactivate preserves revision but appends audit/event.
            compare("E14 legacy deactivate",
                    oracle.post(f"{base}/activations/{oid}/deactivate",headers=csrf),
                    rust.post(f"{base}/activations/{rid}/deactivate",headers=csrf),
                    norm_act)
            req(counts(owner,oid)==(4,4),f"oracle counts {counts(owner,oid)}")
            req(counts(owner,rid)==(4,4),f"rust counts {counts(owner,rid)}")
            print("PASS E14 activation event+audit ledger",flush=True)

            # Partner sandbox manifests activate under their isolated contract.
            compare("E14 partner manifest activate",
                    oracle.post(f"{base}/activations",json={"manifest_id":op},headers=csrf),
                    rust.post(f"{base}/activations",json={"manifest_id":rp},headers=csrf),
                    norm_act)

            # Official shipped runtime contract: second activation is a reactivation,
            # so revision/timestamps are normalized.
            registered=oracle.post(f"{org}/extensions/official/house_numbering/register",headers=csrf)
            req(registered.status_code==201,registered.text)
            official=registered.json()["id"]
            e=oracle.post(f"{base}/activations",json={"manifest_id":official},headers=csrf)
            r=rust.post(f"{base}/activations",json={"manifest_id":official},headers=csrf)
            compare("E14 official runtime activate",e,r,lambda b:norm_act(b,revision=True))

            no_csrf=rust.post(f"{base}/activations",json={"manifest_id":r10},headers=h)
            req(no_csrf.status_code==403,no_csrf.text);print("PASS E14 activation CSRF fail-closed",flush=True)

            # An editor has project.edit/read but not project.manage.
            set_project_role(owner,f,"editor",scoped=False)
            extra_o=manifest(owner,f,"editor_oracle","1.0.0")
            extra_r=manifest(owner,f,"editor_rust","1.0.0")
            compare("E14 editor denied activation",
                    oracle.post(f"{base}/activations",json={"manifest_id":extra_o},headers=csrf),
                    rust.post(f"{base}/activations",json={"manifest_id":extra_r},headers=csrf))
        finally:get_engine().dispose()
    print("E14 activation lifecycle parity PASS",flush=True);return 0

if __name__=="__main__":raise SystemExit(main())
