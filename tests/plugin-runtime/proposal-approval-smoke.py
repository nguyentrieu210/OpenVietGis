#!/usr/bin/env python3
"""Independent reviewer, immutable submission pin, stale geometry and quality gate."""
import argparse
import os
import sys
import tempfile
from pathlib import Path
from uuid import uuid4
import psycopg
import re

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
    p=argparse.ArgumentParser();p.add_argument('--candidate',type=Path,required=True);p.add_argument('--sql-only',action='store_true',help='Validate exact new SQL on an existing compatible candidate, not the new transition API');args=p.parse_args()
    with tempfile.TemporaryDirectory(prefix='ovg-approval-') as d, parity.database(os.environ.get('OVG_PARITY_PG_ADMIN_URL',baseline.DEFAULT_ADMIN_PG_URL)) as (owner,runtime):
        work=Path(d);artifacts=work/'artifacts';artifacts.mkdir()
        parity.configure_python(owner,runtime,artifacts);baseline.run_alembic_upgrade(owner,runtime)
        from modules.house_numbering.migrate import upgrade
        upgrade()
        fx=baseline.seed_benchmark_database(owner,seed=20261128,scales=['1k'],csrf_secret=baseline.DEFAULT_CSRF_SECRET)
        org,project,actor=(fx[k] for k in ('org_a_id','project_a_id','user_a_id'))
        base=f'/api/v1/organizations/{org}';proj=f'{base}/projects/{project}';root=proj+'/modules/house-numbering'
        h=parity.session_headers(fx['session_a_token'],**{'x-openvietgis-csrf':fx['csrf_a_token']})
        reviewer=make_member(owner,fx,'manager','Independent reviewer')
        editor=make_member(owner,fx,'editor','Editor')
        points=seed_source(owner,fx,'Point',{'p1':'POINT(10 10)','p2':'POINT(20 10)'})
        lines=seed_source(owner,fx,'LineString',{'r1':'LINESTRING(0 0,100 0)'})
        polygon=seed_source(owner,fx,'Polygon',{'parcel1':'POLYGON((0 0,40 0,40 40,0 40,0 0))','parcel2':'POLYGON((40 0,80 0,80 40,40 40,40 0))'})
        with parity.rust_client(args.candidate.resolve(),runtime,work) as api:
            manifest=expect(api.post(base+'/extensions/official/house_numbering/register',headers=h),201)
            expect(api.post(proj+'/extensions/activations',json={'manifest_id':manifest['id']},headers=h),201)
            rule=expect(api.post(root+'/rules',json={'version':'fixture-authority','status':'authoritative','authority_ref':'TEST-ONLY'},headers=h),201)
            route=expect(api.post(root+'/routes',json={'name':'Fixture road','source_id':lines,'feature_id':'r1','status':'confirmed'},headers=h),201)
            building=expect(api.post(root+'/buildings',json={'source_id':points,'feature_id':'p1','route_id':route['id'],'eligibility':'eligible'},headers=h),201)
            number=str(uuid4())
            with psycopg.connect(owner) as db:
                db.execute("""INSERT INTO house_numbering.address_numbers(id,organization_id,project_id,building_id,route_id,number_text,kind,side,measure_m,rule_id,created_by_actor_id,updated_by_actor_id,created_at,updated_at)
                VALUES(%s,%s,%s,%s,%s,'1','proposed','left',10,%s,%s,%s,now(),now())""",(number,org,project,building['id'],route['id'],rule['id'],actor,actor))
            proposal=expect(api.post(root+'/proposals',json={'scope_kind':'route','scope_id':route['id'],'rule_id':rule['id'],'building_ids':[building['id']]},headers=h),201)['proposal']
            url=root+'/proposals/'+proposal['id']+'/transition'
            def move(action,rev,headers=h,status=200,comment=None):
                return expect(api.post(url,json={'action':action,'expected_revision':rev,'comment':comment},headers=headers),status)
            if args.sql_only:
                source=(ROOT/'modules/house_numbering/storage/src/persistence/proposals.rs').read_text()
                queries=[re.search(pattern,source,re.S).group(1) for pattern in [
                    r'"(SELECT DISTINCT link FROM \(.*?refs WHERE link->>.*?IS NOT NULL)"',
                    r'"(WITH items AS \(.*?END FROM items)"',
                    r'"(SELECT EXISTS\(SELECT 1 FROM house_numbering.proposal_items i JOIN house_numbering.reconcile_cases c.*?matched\x27\))"']]
                def run(db,query,offset=0):
                    args=[org,project,proposal['id'],offset]
                    order=[int(x)-1 for x in re.findall(r'\$(\d+)',query)]
                    return db.execute(re.sub(r'\$\d+','%s',query),[args[i] for i in order]).fetchall()
                def scoped(db):
                    db.execute("SELECT set_config('openvietgis.organization_id',%s,true),set_config('openvietgis.project_id',%s,true),set_config('openvietgis.actor_user_id',%s,true),set_config('app.current_organization_id',%s,true),set_config('app.current_user_id',%s,true)",(org,project,actor,org,actor))
                with psycopg.connect(runtime) as db:
                    scoped(db)
                    assert len(run(db,queries[0]))==2
                    pin=run(db,queries[1])[0][0];assert len(pin)==64
                    assert run(db,queries[2])[0][0] is False
                    db.execute("INSERT INTO house_numbering.proposal_submission_pins(organization_id,project_id,proposal_id,submitted_revision,submitted_by,input_digest) VALUES(%s,%s,%s,2,%s,%s)",(org,project,proposal['id'],actor,pin))
                    assert not db.execute("SELECT has_table_privilege(current_user,'house_numbering.proposal_submission_pins','UPDATE')").fetchone()[0]
                    assert not db.execute("SELECT has_table_privilege(current_user,'house_numbering.proposal_submission_pins','DELETE')").fetchone()[0]
                with psycopg.connect(owner) as db:
                    db.execute("UPDATE spatial.reference_features SET geometry=ST_GeomFromText('POINT(11 10)',3857) WHERE source_id=%s AND feature_id='p1'",(points,))
                with psycopg.connect(runtime) as db:
                    scoped(db)
                    assert run(db,queries[1])[0][0]!=pin
                    assert db.execute("SELECT count(*) FROM house_numbering.proposal_submission_pins").fetchone()[0]==1
                    db.execute("SELECT set_config('openvietgis.project_id',%s,true)",(fx['project_b_id'],))
                    assert db.execute("SELECT count(*) FROM house_numbering.proposal_submission_pins").fetchone()[0]==0
                with psycopg.connect(owner) as db:
                    db.execute("""INSERT INTO house_numbering.reconcile_cases(id,organization_id,project_id,building_id,evidence_ids,case_class,status,created_by_actor_id,updated_by_actor_id,created_at,updated_at)
                    VALUES(%s,%s,%s,%s,'[]','number_mismatch','open',%s,%s,now(),now())""",(uuid4(),org,project,building['id'],actor,actor))
                with psycopg.connect(runtime) as db:
                    scoped(db)
                    assert run(db,queries[2])[0][0] is True
                duplicate_sql=re.search(r'"(SELECT translate\(btrim\(n.number_text\).*?HAVING count\(\*\) > 1\s+LIMIT 1)"',source,re.S).group(1)
                occupied_sql=re.search(r'"(SELECT n.number_text\s+FROM house_numbering.proposal_items i.*?JOIN house_numbering.address_numbers existing.*?LIMIT 1)"',source,re.S).group(1)
                with psycopg.connect(runtime) as db:
                    scoped(db)
                    assert not run(db,duplicate_sql)
                    assert not run(db,occupied_sql)
                # Disposable test transaction, intentionally rolled back.
                with psycopg.connect(owner) as db:
                    with db.transaction(force_rollback=True):
                        db.execute("""INSERT INTO house_numbering.address_numbers(id,organization_id,project_id,building_id,route_id,number_text,kind,created_by_actor_id,updated_by_actor_id,created_at,updated_at)
                        VALUES(%s,%s,%s,NULL,%s,' 1 ','reserved',%s,%s,now(),now())""",(uuid4(),org,project,route['id'],actor,actor))
                        assert run(db,occupied_sql)==[('1',)]
                print('PASS exact canonical occupancy SQL: historical raw value preserved and whitespace alias detected')
                # Entrance observation/review SQL against the migrated fixture.
                entrance_source=(ROOT/'modules/house_numbering/storage/src/persistence/entrances.rs').read_text()
                blocker=re.search(r'"(SELECT DISTINCT b.id FROM house_numbering.buildings b.*?ORDER BY b.id)"',entrance_source,re.S).group(1)
                def blocked(db):
                    params=[org,project,[uuid4()]]
                    params[2]=[building['id']]
                    order=[int(x)-1 for x in re.findall(r'\$(\d+)',blocker)]
                    return db.execute(re.sub(r'\$\d+','%s',blocker).replace('ANY(%s)','ANY(%s::uuid[])'),[params[i] for i in order]).fetchall()
                alternate_source=seed_source(owner,fx,'LineString',{'r2':'LINESTRING(0 0,0 100)'})
                alternate=expect(api.post(root+'/routes',json={'name':'Other access road','status':'confirmed','source_id':alternate_source,'feature_id':'r2'},headers=h),201)
                with psycopg.connect(runtime) as db:
                    scoped(db)
                    with db.transaction(force_rollback=True):
                        before=run(db,queries[1])[0][0]; eid=uuid4()
                        assert not blocked(db)
                        db.execute("""INSERT INTO house_numbering.building_entrances(id,organization_id,project_id,building_id,route_id,source_id,feature_id,evidence_ref,observed_number,created_by)
                        VALUES(%s,%s,%s,%s,%s,%s,'p2','fixture observation','1A',%s)""",(eid,org,project,building['id'],route['id'],points,actor))
                        def review(rev,state,who):
                            db.execute("INSERT INTO house_numbering.entrance_reviews(organization_id,project_id,entrance_id,revision,state,reason,actor_id) VALUES(%s,%s,%s,%s,%s,'fixture review',%s)",(org,project,eid,rev,state,who))
                        review(1,'observed',actor)
                        assert blocked(db)
                        assert run(db,queries[1])[0][0]!=before
                        try:
                            with db.transaction():review(2,'confirmed',actor)
                        except psycopg.Error as e:assert e.sqlstate=='OV409'
                        else:raise AssertionError('self confirmation accepted')
                        review(2,'confirmed',reviewer['id'])
                        assert not blocked(db)
                        assert len(run(db,queries[0]))==3
                        # A confirmed secondary access on another route requires
                        # explicit primary selection, not deletion of the observation.
                        with db.transaction(force_rollback=True):
                            other=uuid4()
                            db.execute("""INSERT INTO house_numbering.building_entrances(id,organization_id,project_id,building_id,route_id,source_id,feature_id,evidence_ref,created_by)
                               VALUES(%s,%s,%s,%s,%s,%s,'p1','secondary access',%s)""",(other,org,project,building['id'],alternate['id'],points,actor))
                            for rev,state,who in [(1,'observed',actor),(2,'confirmed',reviewer['id'])]:
                                db.execute("INSERT INTO house_numbering.entrance_reviews(organization_id,project_id,entrance_id,revision,state,reason,actor_id) VALUES(%s,%s,%s,%s,%s,'secondary evidence',%s)",(org,project,other,rev,state,who))
                            assert blocked(db)
                            choose_sql=re.search(r'"(INSERT INTO house_numbering.primary_entrance_choices.*?d.choice_revision=\$4-1)"',entrance_source,re.S).group(1)
                            def choose(rev,selected,who):
                                params=[org,project,building['id'],rev,selected,'fixture primary decision',who]
                                order=[int(x)-1 for x in re.findall(r'\$(\d+)',choose_sql)]
                                return db.execute(re.sub(r'\$\d+','%s',choose_sql),[params[i] for i in order]).rowcount
                            for selected,who in [(eid,actor),(other,reviewer['id'])]:
                                try:
                                    with db.transaction():choose(1,selected,who)
                                except psycopg.Error as e:assert e.sqlstate=='OV409'
                                else:raise AssertionError('invalid primary choice accepted')
                            digest_before=run(db,queries[1])[0][0]
                            assert choose(1,eid,reviewer['id'])==1
                            assert not blocked(db)
                            assert run(db,queries[1])[0][0]!=digest_before
                            assert choose(1,eid,reviewer['id'])==0
                            numbering=(ROOT/'modules/house_numbering/storage/src/persistence/numbering.rs').read_text()
                            point_sql=re.search(r'"(SELECT b.id,n.id AS number_id.*?ST_MultiPolygon\x27\))"',numbering,re.S).group(1)
                            def selected_points():
                                params=[org,project,route['id'],3857]
                                order=[int(x)-1 for x in re.findall(r'\$(\d+)',point_sql)]
                                return db.execute(re.sub(r'\$\d+','%s',point_sql),[params[i] for i in order]).fetchall()
                            assert len(selected_points())==1 and selected_points()[0][-2:]==(20.0,10.0)
                            # Retiring the secondary evidence invalidates the pinned choice.
                            db.execute("INSERT INTO house_numbering.entrance_reviews(organization_id,project_id,entrance_id,revision,state,reason,actor_id) VALUES(%s,%s,%s,3,'retired','changed evidence',%s)",(org,project,other,reviewer['id']))
                            assert blocked(db) and not selected_points()
                            assert choose(2,eid,reviewer['id'])==1 and not blocked(db)
                            db.execute("UPDATE house_numbering.address_numbers SET kind='submitted' WHERE id=%s",(number,))
                            try:
                                with db.transaction():choose(3,eid,reviewer['id'])
                            except psycopg.Error as e:assert e.sqlstate=='OV409'
                            else:raise AssertionError('submitted number allowed primary change')
                            assert not db.execute("SELECT has_table_privilege(current_user,'house_numbering.primary_entrance_choices','UPDATE')").fetchone()[0]
                            db.execute("SELECT set_config('openvietgis.project_id',%s,true)",(fx['project_b_id'],))
                            assert db.execute("SELECT count(*) FROM house_numbering.primary_entrance_choices").fetchone()[0]==0
                        print('PASS primary entrance SQL: cross-route choice, independent reviewer, selected geometry, stale choice, submitted number gate, RLS')
                        review(3,'retired',reviewer['id'])
                        assert not blocked(db)
                        assert not db.execute("SELECT has_table_privilege(current_user,'house_numbering.entrance_reviews','UPDATE')").fetchone()[0]
                        db.execute("SELECT set_config('openvietgis.project_id',%s,true)",(fx['project_b_id'],))
                        assert db.execute("SELECT count(*) FROM house_numbering.building_entrances").fetchone()[0]==0
                print('PASS exact entrance SQL: observed blocks, independent confirmation, digest change, feature lock references, retirement and RLS')
                # Staff cohort: duplicate observations, different recorders,
                # drafts and out-of-window rows. No production records touched.
                from datetime import date
                staff_source=(ROOT/'modules/house_numbering/storage/src/persistence/issuance.rs').read_text()
                staff_sql='WITH cohort AS ('+staff_source.split('WITH cohort AS (',1)[1].split('"',1)[0]
                def staff(db,start,end):
                    params=[org,project,start,end]
                    order=[int(x)-1 for x in re.findall(r'\$(\d+)',staff_sql)]
                    return db.execute(re.sub(r'\$\d+','%s',staff_sql),[params[i] for i in order]).fetchone()[0]
                with psycopg.connect(runtime) as db:
                    scoped(db)
                    with db.transaction(force_rollback=True):
                        for who,state,day in [(actor,'accepted','2026-11-05'),(actor,'rejected','2026-11-06'),(reviewer['id'],'submitted','2026-11-07'),(actor,'draft','2026-11-08'),(actor,'submitted','2026-12-01')]:
                            db.execute("""INSERT INTO house_numbering.surveys(id,organization_id,project_id,client_uuid,building_id,condition,state,recorded_by_actor_id,submitted_by_actor_id,revision,created_at,updated_at)
                            VALUES(%s,%s,%s,%s,%s,'house',%s,%s,%s,1,%s::date::timestamp AT TIME ZONE 'UTC',now())""",
                            (uuid4(),org,project,uuid4(),building['id'],state,who,actor,day))
                        report=staff(db,date(2026,11,1),date(2026,12,1))
                        assert report['distinct_observed_buildings']==report['distinct_accepted_buildings']==1
                        assert len(report['items'])==2
                        rows={r['actor_id']:r for r in report['items']}
                        assert rows[actor]['submitted_forms']==2 and rows[actor]['observed_buildings']==1
                        assert rows[actor]['accepted_forms']==rows[actor]['rejected_forms']==1
                        assert rows[reviewer['id']]['pending_forms']==1
                        assert rows[reviewer['id']]['display_name']=='Independent reviewer'
                        assert staff(db,date(2026,10,1),date(2026,11,1))['items']==[]
                        db.execute("SELECT set_config('openvietgis.project_id',%s,true)",(fx['project_b_id'],))
                        assert staff(db,date(2026,11,1),date(2026,12,1))['items']==[]
                print('PASS staff KPI SQL: distinct buildings, current review states, draft exclusion, UTC period, identity label and cross-project RLS')
                # Parcel identity and many-to-many associations with immutable review.
                parcel_src=(ROOT/'modules/house_numbering/storage/src/persistence/parcels.rs').read_text()
                create_sql=re.search(r'"(WITH inserted AS \(INSERT INTO house_numbering.parcels.*?FROM inserted)"',parcel_src,re.S).group(1)
                link_sql=re.search(r'"(INSERT INTO house_numbering.parcel_building_links.*?END IS NOT NULL)"',parcel_src,re.S).group(1)
                def execute(db,sql,args):
                    order=[int(x)-1 for x in re.findall(r'\$(\d+)',sql)]
                    return db.execute(re.sub(r'\$\d+','%s',sql),[args[i] for i in order])
                with psycopg.connect(runtime) as db:
                    scoped(db)
                    with db.transaction(force_rollback=True):
                        pid=str(uuid4());pid2=str(uuid4())
                        def register(id,feature):
                            return execute(db,create_sql,[id,org,project,polygon,feature,'fixture parcel','test evidence',actor]).fetchone()
                        assert register(pid,'parcel1')[0]['id']==pid
                        assert register(str(uuid4()),'parcel1') is None
                        assert register(pid2,'parcel2')[0]['id']==pid2
                        def link(parcel,rev,state,who):
                            return execute(db,link_sql,[org,project,parcel,building['id'],rev,state,'fixture link',who]).rowcount
                        assert link(pid,1,'observed',actor)==1
                        try:
                            with db.transaction():link(pid,2,'confirmed',actor)
                        except psycopg.Error as e:assert e.sqlstate=='OV409'
                        else:raise AssertionError('self confirmation accepted')
                        assert link(pid,2,'confirmed',reviewer['id'])==1
                        assert link(pid,2,'detached',reviewer['id'])==0
                        assert link(pid2,1,'observed',actor)==1
                        assert link(pid2,2,'confirmed',reviewer['id'])==1
                        assert link(pid,3,'detached',reviewer['id'])==1
                        assert link(pid,4,'observed',actor)==1
                        assert not db.execute("SELECT has_table_privilege(current_user,'house_numbering.parcel_building_links','UPDATE')").fetchone()[0]
                        db.execute("SELECT set_config('openvietgis.project_id',%s,true)",(fx['project_b_id'],))
                        assert db.execute("SELECT count(*) FROM house_numbering.parcels").fetchone()[0]==0
                print('PASS parcel SQL: distinct identity, duplicate geometry link, independent review, multiple parcels, revision CAS, detach/reobserve and RLS')
                # Old candidate supplies a real approved fixture via its ordinary
                # transition API. New issuance SQL is checked separately.
                with psycopg.connect(owner) as db:
                    db.execute("UPDATE house_numbering.reconcile_cases SET status='resolved',revision=revision+1 WHERE building_id=%s",(building['id'],))
                move('submit',1)
                with psycopg.connect(runtime) as db:
                    scoped(db)
                    submitted_digest=run(db,queries[1])[0][0]
                move('approve',2,headers=reviewer['csrf'])
                with psycopg.connect(runtime) as db:
                    scoped(db)
                    assert run(db,queries[1],1)[0][0]==submitted_digest
                    assert run(db,queries[1],0)[0][0]!=submitted_digest
                from issuance_sql_fixture import check_issuance_sql
                check_issuance_sql(ROOT,runtime,scoped,org,project,actor,proposal['id'],number,route['id'],fx['project_b_id'])
                print('PASS exact proposal SQL, geometry digest invalidation, quality gate, append-only grants and cross-project RLS; transition API not tested')
                return
            move('submit',1)
            move('approve',2,status=403)
            move('approve',2,headers=editor['csrf'],status=403)
            with psycopg.connect(owner) as db:
                db.execute("UPDATE spatial.reference_features SET geometry=ST_GeomFromText('POINT(11 10)',3857) WHERE source_id=%s AND feature_id='p1'",(points,))
            move('approve',2,headers=reviewer['csrf'],status=409)
            move('withdraw',2,comment='changed geometry')
            move('submit',3)
            with psycopg.connect(owner) as db:
                db.execute("""INSERT INTO house_numbering.reconcile_cases(id,organization_id,project_id,building_id,evidence_ids,case_class,status,created_by_actor_id,updated_by_actor_id,created_at,updated_at)
                VALUES(%s,%s,%s,%s,'[]','number_mismatch','open',%s,%s,now(),now())""",(uuid4(),org,project,building['id'],actor,actor))
            move('approve',4,headers=reviewer['csrf'],status=409)
            with psycopg.connect(owner) as db:
                db.execute("UPDATE house_numbering.reconcile_cases SET status='resolved',revision=revision+1,resolution='fixture verified' WHERE building_id=%s",(building['id'],))
            move('approve',4,headers=reviewer['csrf'],status=409)
            move('withdraw',4,comment='reviewed evidence')
            move('submit',5)
            result=move('approve',6,headers=reviewer['csrf'])
            assert result['status']=='approved'
            move('approve',6,headers=reviewer['csrf'],status=409)
            with psycopg.connect(owner) as db:
                assert db.execute("SELECT kind FROM house_numbering.address_numbers WHERE id=%s",(number,)).fetchone()[0]=='approved'
                pins=db.execute("SELECT submitted_revision,submitted_by::text,input_digest FROM house_numbering.proposal_submission_pins WHERE proposal_id=%s ORDER BY submitted_revision",(proposal['id'],)).fetchall()
                assert [p[0] for p in pins]==[2,4,6] and all(p[1]==actor and len(p[2])==64 for p in pins)
                assert len({p[2] for p in pins})==3
            body={'proposal_id':proposal['id'],'number_id':number,'authority_name':'Fixture authority','decision_ref':'TEST-DECISION','certificate_ref':'TEST-CERT','effective_date':'2026-11-01','template_revision':'test-v1'}
            expect(api.post(root+'/issuances',json=body,headers=editor['csrf']),403)
            registered=expect(api.post(root+'/issuances',json=body,headers=reviewer['csrf']),201)
            iid=registered['record']['id'];issue=root+'/issuances/'+iid;lookup=root+'/address-lookup/'+iid
            assert registered['history'][0]['state']=='recorded'
            expect(api.post(root+'/issuances',json=body,headers=reviewer['csrf']),409)
            expect(api.get(lookup),401)
            assert expect(api.get(lookup,headers=h),200)['plate_state']=='recorded'
            def stage(revision,state,status=200):
                return expect(api.post(issue+'/transition',json={'expected_revision':revision,'state':state,'evidence_ref':'Fixture evidence'},headers=reviewer['csrf']),status)
            stage(1,'installed',409)
            for rev,state in [(1,'printed'),(2,'delivered'),(3,'installed')]:stage(rev,state)
            stage(1,'void',409)
            assert expect(api.get(lookup,headers=h),200)['plate_state']=='installed'
            replacement={k:v for k,v in body.items() if k not in ('proposal_id','number_id')}
            replacement.update(expected_revision=4,reason='Fixture replacement')
            expect(api.post(issue+'/replace',json=replacement,headers=reviewer['csrf']),409)
            assert expect(api.get(lookup,headers=h),200)['plate_state']=='installed'
            replacement['certificate_ref']='TEST-CERT-2'
            replaced=expect(api.post(issue+'/replace',json=replacement,headers=reviewer['csrf']),201)
            new_id=replaced['record']['id']
            assert replaced['record']['replaces_issuance_id']==iid
            expect(api.get(lookup,headers=h),404)
            assert expect(api.get(root+'/address-lookup/'+new_id,headers=h),200)['plate_state']=='recorded'
            replacement.update(expected_revision=5,certificate_ref='TEST-CERT-3')
            expect(api.post(issue+'/replace',json=replacement,headers=reviewer['csrf']),409)
            kpi=expect(api.get(root+'/lifecycle-kpi',headers=h),200)
            assert kpi['registered_buildings']==1 and kpi['installed_buildings']==0
            breakdown=expect(api.get(root+'/lifecycle-breakdown',headers=h),200)
            for dim in ('areas','routes'):
                assert sum(r['active_buildings'] for r in breakdown[dim])==breakdown['total']['active_buildings']
                assert sum(r['registered_buildings'] for r in breakdown[dim])==1
            staff=root+'/survey-staff-kpi?start_date=2026-11-01&end_date_exclusive=2026-12-01'
            expect(api.get(staff,headers=editor['csrf']),403)
            assert expect(api.get(staff,headers=reviewer['csrf']),200)['items']==[]
            print('PASS replacement HTTP: failed replacement rollback, linked successor, old lookup denied and deduplicated KPI')
            assert len(expect(api.get(issue,headers=reviewer['csrf']),200)['history'])==5
            print('PASS issuance HTTP: external certificate registry, ordered plate states, permission, immutable history and revocation')
            print('PASS proposal: independent reviewer, stale geometry, unresolved case, changed evidence, resubmission, approved number, retained pins')
if __name__=='__main__':main()
