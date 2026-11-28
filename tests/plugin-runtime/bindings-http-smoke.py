#!/usr/bin/env python3
"""Full Rust HTTP + PostGIS smoke, using the isolated benchmark database harness."""
from pathlib import Path
import argparse
import os
import sys
import tempfile
from uuid import uuid4
import psycopg
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tests/rust-parity'))
sys.path.insert(0,str(ROOT/'tools/benchmark/rust-platform'))
import run as parity
import benchmark_runner as baseline

def expect(response,status):
    assert response.status_code==status,(response.status_code,response.text)
    return response.json() if response.content else None

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate',type=Path,required=True)
    parser.add_argument('--pure-commands',action='store_true',help='Also test Wave 04 runtime command gateway')
    parser.add_argument('--module-events',action='store_true',help='Also test Wave 06 durable module change stream')
    args=parser.parse_args();binary=args.candidate.resolve();assert binary.is_file()
    with tempfile.TemporaryDirectory(prefix='ovg-067-binding-http-') as d:
      work=Path(d)
      with parity.database(os.environ.get('OVG_PARITY_PG_ADMIN_URL',baseline.DEFAULT_ADMIN_PG_URL)) as (owner,runtime):
        artifacts=work/'artifacts';artifacts.mkdir()
        parity.configure_python(owner,runtime,artifacts);baseline.run_alembic_upgrade(owner,runtime)
        from modules.house_numbering.migrate import upgrade
        upgrade();fixture=baseline.seed_benchmark_database(owner,seed=20261128,scales=['1k'],csrf_secret=baseline.DEFAULT_CSRF_SECRET)
        org=f"/api/v1/organizations/{fixture['org_a_id']}";project=f"{org}/projects/{fixture['project_a_id']}"
        root=project+'/modules/house-numbering';ext=project+'/extensions/house_numbering';bindings=ext+'/layer-bindings';policy=ext+'/permissions/human/'+fixture['user_a_id']
        h=parity.session_headers(fixture['session_a_token'],**{'x-openvietgis-csrf':fixture['csrf_a_token']})
        outsider=parity.session_headers(fixture['session_b_token'],**{'x-openvietgis-csrf':fixture['csrf_b_token']})
        with parity.rust_client(binary,runtime,work) as api:
          event_path=project+'/extensions/change-events'
          if args.module_events:
            snapshot=expect(api.get(event_path,headers=h),200)
            assert snapshot['mode']=='snapshot_required' and not snapshot['items']
            cursor=snapshot['cursor']
            response=api.get(event_path,headers=outsider);assert response.status_code in (403,404)
            expect(api.get(event_path,params={'cursor':'bad'},headers=h),422)
          manifest=expect(api.post(org+'/extensions/official/house_numbering/register',headers=h),201)
          expect(api.post(project+'/extensions/activations',json={'manifest_id':manifest['id']},headers=h),201)
          cmd=ext+'/commands/preview_numbering';payload={'payload':{'street_name':'Đường A','side':'left_odd','start_number':1,'step':2,'count':3}}
          if args.pure_commands:
            result=expect(api.post(cmd,json=payload,headers=h),200)
            assert [v['house_number'] for v in result['output']['proposals']]==['1','3','5']
            expect(api.post(cmd,json={'payload':{**payload['payload'],'step':1}},headers=h),422)
            expect(api.post(ext+'/commands/apply_numbering',json=payload,headers=h),404)
            denied=api.post(cmd,json=payload,headers=outsider);assert denied.status_code in (403,404)
            expect(api.post(cmd,json=payload,headers=parity.session_headers(fixture['session_a_token'])),403)
          # Exercise write + audit through the relocated HTTP and storage crates.
          area=expect(api.post(root+'/areas',json={'name':'Fixture hamlet','kind':'current'},headers=h),201)
          rule=expect(api.post(root+'/rules',json={'version':'fixture-v1','status':'draft'},headers=h),201)
          assert any(x['id']==area['id'] for x in expect(api.get(root+'/areas',headers=h),200)['items'])
          assert any(x['id']==rule['id'] for x in expect(api.get(root+'/rules',headers=h),200)['items'])
          with psycopg.connect(owner) as db:
            rows=db.execute("SELECT actor_id::text,actor_type,resource_id::text FROM audit.audit_events WHERE resource_id IN (%s,%s)",(area['id'],rule['id'])).fetchall()
            assert set(rows)=={(fixture['user_a_id'],'human',area['id']),(fixture['user_a_id'],'human',rule['id'])},rows
          expect(api.post(root+'/areas',json={'name':'CSRF denied'},headers=parity.session_headers(fixture['session_a_token'])),403)
          # Linked building and assignment audit exercise the versioned GIS read view.
          point_source=fixture['layers']['epsg4326_1k']['source_id']
          with psycopg.connect(owner) as db:
            feature_id=db.execute("SELECT feature_id FROM spatial.reference_features WHERE source_id=%s ORDER BY feature_id LIMIT 1",(point_source,)).fetchone()[0]
          building=expect(api.post(root+'/buildings',json={'source_id':point_source,'feature_id':feature_id,'area_id':area['id']},headers=h),201)
          assert building['linked_source_id']==point_source and building['linked_feature_id']==feature_id
          report=expect(api.post(root+'/building-assignment/audit',json={'source_id':point_source},headers=h),200)
          assert isinstance(report,dict)
          assert expect(api.get(bindings,headers=h),200)=={'configured':False,'revision':0,'items':[]}
          layer=expect(api.post(project+'/layers/manual',json={'operation_id':str(uuid4()),'name':'Original survey','geometry_family':'Point','fields':[{'field_id':f,'name':f,'type':'text','nullable':True} for f in ['ap','ten_duong','xu_ly']]},headers=h),201)
          source=layer['source_id'];layer_id=layer['layer_id'];schema=expect(api.get(project+'/layers/'+layer_id,headers=h),200)['source']['schema_revision']
          item={'source_id':source,'role':'survey_points','schema_revision':schema}
          response=expect(api.put(bindings,json={'expected_revision':0,'items':[item]},headers=h),200);assert response['revision']==1 and response['items'][0]['status']=='ready'
          expect(api.put(bindings,json={'expected_revision':0,'items':[item]},headers=h),409)
          expect(api.put(bindings,json={'expected_revision':1,'items':[{**item,'role':'bogus'}]},headers=h),422)
          expect(api.put(bindings,json={'expected_revision':1,'items':[{**item,'role':'road'}]},headers=h),422)
          expect(api.put(bindings,json={'expected_revision':1,'items':[{**item,'schema_revision':'0'*64}]},headers=h),409)
          foreign=api.get(bindings,headers=outsider);assert foreign.status_code in (403,404),foreign.text
          # Change the display name without changing source identity. Test real SQL persistence.
          with psycopg.connect(owner) as db:db.execute('UPDATE spatial.layers SET name=%s WHERE id=%s',('Renamed survey',layer_id))
          after=expect(api.get(bindings,headers=h),200);assert after['items'][0]['source_id']==source and after['items'][0]['status']=='ready'
          with psycopg.connect(owner) as db:db.execute("UPDATE spatial.layers SET lifecycle='retired',retired_at=now(),updated_at=now() WHERE id=%s",(layer_id,))
          assert expect(api.get(bindings,headers=h),200)['items'][0]['status']=='unavailable'
          # Module restrictions cannot alter Core access and take effect on next HTTP request.
          p=expect(api.get(policy,headers=h),200);assert p['revision']==0 and p['permissions'] is None
          p=expect(api.put(policy,json={'expected_revision':0,'permissions':['ext.house_numbering.read']},headers=h),200);assert p['revision']==1
          expect(api.get(root+'/rules',headers=h),200)
          if args.pure_commands:expect(api.post(cmd,json=payload,headers=h),403)
          expect(api.post(root+'/rules',json={'version':'denied','status':'draft'},headers=h),403)
          expect(api.put(bindings,json={'expected_revision':1,'items':[]},headers=h),403)
          p=expect(api.put(policy,json={'expected_revision':1,'permissions':[]},headers=h),200);assert p['revision']==2
          expect(api.get(root+'/rules',headers=h),403);expect(api.get(bindings,headers=h),403);expect(api.get(project+'/layers',headers=h),200)
          p=expect(api.put(policy,json={'expected_revision':2,'permissions':None},headers=h),200);assert p['revision']==3
          expect(api.get(root+'/rules',headers=h),200)
          expect(api.put(policy,json={'expected_revision':2,'permissions':[]},headers=h),409)
          p=expect(api.put(bindings,json={'expected_revision':1,'items':[]},headers=h),200);assert p['configured'] and p['revision']==2 and not p['items']
          expect(api.post(ext+'/disable',headers=h),200);expect(api.get(bindings,headers=h),404)
          if args.pure_commands:expect(api.post(cmd,json=payload,headers=h),404)
          if args.module_events:
            # First activation creates the stream; epoch mismatch requires a fresh snapshot.
            page=expect(api.get(event_path,params={'cursor':cursor},headers=h),200)
            assert page['mode']=='snapshot_required'
            epoch=page['cursor'].split(':')[0]
            head=int(page['cursor'].split(':')[1])
            cursor=epoch+':0';events=[]
            while True:
              page=expect(api.get(event_path,params={'cursor':cursor,'limit':2},headers=h),200)
              assert page['mode']=='delta';events.extend(page['items']);cursor=page['cursor']
              if not page['has_more']:break
            assert [e['kind'] for e in events]==['activation','bindings','permissions','permissions','permissions','bindings','activation'],events
            assert [int(e['sequence']) for e in events]==list(range(1,head+1))
            assert all(set(e)=={'sequence','extension_key','kind'} for e in events)
            assert not expect(api.get(event_path,params={'cursor':cursor},headers=h),200)['items']
            assert expect(api.get(event_path,params={'cursor':epoch+':'+str(head+1)},headers=h),200)['mode']=='snapshot_required'
            print('PASS real HTTP: durable event replay, scoped access, page cursors and invalidation privacy')
          print('PASS real HTTP: bindings, role/schema validation, revision conflict, rename/retire, cross-tenant access, permission read/deny/restore, Core unchanged, disabled runtime')
if __name__=='__main__':main()
