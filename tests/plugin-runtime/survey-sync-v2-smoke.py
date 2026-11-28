#!/usr/bin/env python3
"""End-to-end offline receipt protocol acceptance on a fresh disposable DB."""
import argparse
import os
import sys
import tempfile
from pathlib import Path
from uuid import uuid4
import psycopg

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tests/rust-parity'))
sys.path.insert(0,str(ROOT/'tools/benchmark/rust-platform'))
import run as parity
import benchmark_runner as baseline
from e15_full_parity import make_member,seed_source

def expect(res,status):
    assert res.status_code==status,(res.status_code,res.text)
    return res.json()

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--candidate',type=Path,required=True)
    args=parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='ovg-survey-v2-') as d, parity.database(os.environ.get('OVG_PARITY_PG_ADMIN_URL',baseline.DEFAULT_ADMIN_PG_URL)) as (owner,runtime):
        work=Path(d);artifacts=work/'artifacts';artifacts.mkdir()
        parity.configure_python(owner,runtime,artifacts);baseline.run_alembic_upgrade(owner,runtime)
        from modules.house_numbering.migrate import upgrade
        upgrade()
        fx=baseline.seed_benchmark_database(owner,seed=20261128,scales=['1k'],csrf_secret=baseline.DEFAULT_CSRF_SECRET)
        org,project,actor=(fx[k] for k in ('org_a_id','project_a_id','user_a_id'))
        base=f'/api/v1/organizations/{org}';proj=f'{base}/projects/{project}';root=proj+'/modules/house-numbering'
        h=parity.session_headers(fx['session_a_token'],**{'x-openvietgis-csrf':fx['csrf_a_token']})
        reviewer=make_member(owner,fx,'manager','Independent reviewer')
        points=seed_source(owner,fx,'Point',{'p1':'POINT(10 10)','p2':'POINT(20 10)','p3':'POINT(30 10)'})
        with parity.rust_client(args.candidate.resolve(),runtime,work) as api:
            manifest=expect(api.post(base+'/extensions/official/house_numbering/register',headers=h),201)
            expect(api.post(proj+'/extensions/activations',json={'manifest_id':manifest['id']},headers=h),201)
            buildings=[expect(api.post(root+'/buildings',json={'source_id':points,'feature_id':fid,'eligibility':'unknown'},headers=h),201) for fid in ('p1','p2','p3')]
            def operation(b,**over):
                op={'client_uuid':str(uuid4()),'building_id':b['id'],'queue_actor_id':actor,'expected_building_revision':b['revision'],'condition':'house','photo_artifact_ids':[]}
                op.update(over);return op
            def sync(ops,status=200,headers=h):
                return expect(api.post(root+'/survey/sync',json={'receipt_version':2,'operations':ops},headers=headers),status)
            stale=operation(buildings[1],expected_building_revision=99)
            good=operation(buildings[0])
            result=sync([good,stale])
            receipt=result['receipts'][0];sid=receipt['survey_id']
            assert result['receipts']==[{'client_uuid':good['client_uuid'],'survey_id':sid,'status':'applied'}]
            assert result['conflicts'][0]['client_uuid']==stale['client_uuid'] and result['conflicts'][0]['code']=='STALE_BUILDING'
            assert sync([good])['receipts']==[{**receipt,'status':'replayed'}]
            assert sync([{**good,'condition':'other'}])['conflicts'][0]['code']=='CLIENT_UUID_PAYLOAD_CONFLICT'
            sync([good],403,reviewer['csrf'])
            sync([operation(buildings[2],queue_actor_id=reviewer['id'])],403)
            sync([good,good],422)
            expect(api.post(root+'/surveys/'+sid+'/review',json={'expected_revision':1,'decision':'accepted'},headers=reviewer['csrf']),200)
            assert sync([operation(buildings[0])])['conflicts'][0]['code']=='BUILDING_ALREADY_ACCEPTED'
            assert sync([good])['receipts'][0]['status']=='replayed'
            with psycopg.connect(owner) as db:
                assert db.execute("SELECT count(*) FROM house_numbering.surveys WHERE project_id=%s",(project,)).fetchone()[0]==1
                db.execute("UPDATE house_numbering.buildings SET revision=revision+1 WHERE id=%s",(buildings[0]['id'],))
            assert sync([good])['receipts'][0]['status']=='replayed'
            print('PASS receipt v2: mixed results, exact replay, changed payload, foreign actor, duplicate batch, accepted survey and stale building')
if __name__=='__main__':main()
