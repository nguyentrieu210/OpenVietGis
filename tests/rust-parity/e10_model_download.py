#!/usr/bin/env python3
"""E10 3D model binary download parity."""

from __future__ import annotations

import argparse
import hashlib
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

def main():
    ap=argparse.ArgumentParser(); ap.add_argument("--candidate",type=Path,required=True); a=ap.parse_args()
    binary=a.candidate.resolve(); req(binary.is_file(),"binary missing")
    admin=os.environ.get("OVG_PARITY_PG_ADMIN_URL",baseline.DEFAULT_ADMIN_PG_URL)
    with tempfile.TemporaryDirectory(prefix="ovg-e10-model-download-") as d:
      work=Path(d)
      with parity.database(admin) as (owner,runtime):
        art=work/"artifacts"; art.mkdir(); parity.configure_python(owner,runtime,art)
        baseline.run_alembic_upgrade(owner,runtime)
        f=baseline.seed_benchmark_database(owner,seed=20261126,scales=["1k"],csrf_secret=baseline.DEFAULT_CSRF_SECRET)
        model_id=uuid4(); payload=b"glTF-model-binary-parity"; sha=hashlib.sha256(payload).hexdigest()
        with psycopg.connect(owner) as c:
          c.execute("""INSERT INTO spatial.model_assets
            (organization_id,project_id,id,filename,format,sha256,byte_size,payload,actor_id)
            VALUES(%s,%s,%s,%s,'glb',%s,%s,%s,%s)""",
            (f["org_a_id"],f["project_a_id"],model_id,"Mô hình 01.glb",sha,len(payload),payload,f["user_a_id"]))
        from fastapi.testclient import TestClient
        from openvietgis_api.adapters.persistence.db import get_engine
        from openvietgis_api.main import app
        root=f"{API}/organizations/{f['org_a_id']}/projects/{f['project_a_id']}/3d/models"
        h=parity.session_headers(f["session_a_token"])
        try:
          with TestClient(app,raise_server_exceptions=False) as oracle, parity.rust_client(binary,runtime,work) as rust:
            e=oracle.get(f"{root}/{model_id}/download",headers=h); r=rust.get(f"{root}/{model_id}/download",headers=h)
            req(e.status_code==r.status_code==200,(e.status_code,e.text,r.status_code,r.text))
            req(e.content==r.content==payload,"binary mismatch")
            for k in ("content-type","content-disposition","cache-control","x-content-type-options"):
              req(e.headers.get(k)==r.headers.get(k),f"{k}: {e.headers.get(k)} != {r.headers.get(k)}")
            print("PASS E10 model binary body+headers",flush=True)
            missing=uuid4()
            e=oracle.get(f"{root}/{missing}/download",headers=h); r=rust.get(f"{root}/{missing}/download",headers=h)
            req(e.status_code==r.status_code,"missing status")
            req(parity.difference(e.json(),r.json()) is None,"missing body")
            print("PASS E10 model binary missing",flush=True)
            with psycopg.connect(owner) as c:
              c.execute("UPDATE spatial.model_assets SET sha256=%s WHERE organization_id=%s AND project_id=%s AND id=%s",("0"*64,f["org_a_id"],f["project_a_id"],model_id))
            e=oracle.get(f"{root}/{model_id}/download",headers=h); r=rust.get(f"{root}/{model_id}/download",headers=h)
            req(e.status_code==r.status_code==409,(e.status_code,e.text,r.status_code,r.text))
            req(parity.difference(e.json(),r.json()) is None,f"integrity body: {e.text} {r.text}")
            print("PASS E10 model integrity conflict",flush=True)
        finally: get_engine().dispose()
    print("E10 model download parity PASS",flush=True); return 0
if __name__=="__main__": raise SystemExit(main())
