"""Disposable fixture only: exercise the exact new issuance SQL and DB state machine."""
from datetime import date
from pathlib import Path
from uuid import uuid4
import re
import psycopg

def check_issuance_sql(root, runtime, scoped, org, project, actor, proposal, number, route, other_project):
    src=(Path(root)/'modules/house_numbering/storage/src/persistence/issuance.rs').read_text()
    create=re.search(r'"(WITH inserted AS \(.*?SELECT to_jsonb\(inserted\) FROM inserted)"',src,re.S).group(1)
    event=re.search(r'"(INSERT INTO house_numbering.issuance_events.*?)"',src,re.S).group(1)
    lookup=re.search(r'"(SELECT jsonb_build_object\(\x27id\x27,r.id.*?route.status=\x27confirmed\x27)"',src,re.S).group(1)
    stable=re.search(r'"(SELECT jsonb_build_object\(\x27address_id\x27,r.number_id.*?route.status=\x27confirmed\x27)"',src,re.S).group(1)
    iid=str(uuid4())
    def query(db,sql,params):
        order=[int(x)-1 for x in re.findall(r'\$(\d+)',sql)]
        return db.execute(re.sub(r'\$\d+','%s',sql),[params[i] for i in order])
    def append(db,rev,state):
        return query(db,event,[org,project,iid,rev,state,'fixture evidence',actor])
    with psycopg.connect(runtime) as db:
        scoped(db)
        params=[org,project,iid,proposal,number,'Fixture authority','TEST-DECISION','TEST-CERT',date(2026,11,1),'test-v1',actor]
        assert query(db,create,params).fetchone()[0]['route_id']==route
        append(db,1,'recorded')
        assert query(db,create,[*params[:2],str(uuid4()),*params[3:]]).fetchone() is None
        def state():
            row=query(db,lookup,[org,project,iid]).fetchone()
            return row[0]['plate_state'] if row else None
        kpi=src.split('WITH buildings AS (',1)[1].split('"',1)[0]
        kpi='WITH buildings AS ('+kpi
        def counts():return query(db,kpi,[org,project]).fetchone()[0]["total"]
        assert counts()['registered_buildings']==1 and counts()['installed_buildings']==0
        assert query(db,stable,[org,project,number]).fetchone()[0]['issuance_id']==iid
        assert state()=='recorded'
        try:
            with db.transaction():append(db,2,'installed')
        except psycopg.Error as e:assert e.sqlstate=='OV409'
        else:raise AssertionError('skipped required plate stages')
        for rev,status in [(2,'printed'),(3,'delivered'),(4,'installed')]:append(db,rev,status)
        assert state()=='installed'
        assert counts()['installed_buildings']==1
        replace=re.search(r'"(INSERT INTO house_numbering.issuance_records\(id,organization_id,project_id,proposal_id,building_id,number_id,route_id,\s+number_text,route_name,authority_name,decision_ref,certificate_ref,effective_date,template_revision,recorded_by,replaces_issuance_id.*?RETURNING id)"',src,re.S).group(1)
        replacement_id=str(uuid4())
        new_params=[org,project,iid,replacement_id,'Fixture authority','TEST-REPLACE','TEST-CERT-2',date(2026,11,2),'test-v1',actor,'lost certificate']
        try:
            with db.transaction():query(db,replace,new_params)
        except psycopg.Error as e:assert e.sqlstate=='OV409'
        else:raise AssertionError('nonvoid predecessor accepted')
        # A failed replacement must leave the original valid after rollback.
        try:
            with db.transaction():
                append(db,5,'void')
                bad=list(new_params);bad[6]='TEST-CERT'
                assert query(db,replace,bad).fetchone() is None
                raise ValueError('rollback simulated conflict')
        except ValueError:pass
        assert state()=='installed'
        append(db,5,'void')
        assert state() is None
        assert query(db,stable,[org,project,number]).fetchone() is None
        try:
            with db.transaction():append(db,6,'printed')
        except psycopg.Error as e:assert e.sqlstate=='OV409'
        else:raise AssertionError('void record reactivated')
        assert str(query(db,replace,new_params).fetchone()[0])==replacement_id
        query(db,event,[org,project,replacement_id,1,'recorded','fixture replacement',actor])
        assert query(db,lookup,[org,project,replacement_id]).fetchone()[0]['plate_state']=='recorded'
        assert query(db,stable,[org,project,number]).fetchone()[0]['issuance_id']==replacement_id
        assert state() is None
        assert counts()['registered_buildings']==1 and counts()['installed_buildings']==0
        grouped=query(db,kpi,[org,project]).fetchone()[0]
        for dim in ('areas','routes'):
            assert sum(r['active_buildings'] for r in grouped[dim])==grouped['total']['active_buildings']
            assert sum(r['registered_buildings'] for r in grouped[dim])==1
        assert grouped['areas'][0]['id'] is None and grouped['routes'][0]['id']==route
        fork=list(new_params);fork[3]=str(uuid4());fork[6]='TEST-CERT-3'
        assert query(db,replace,fork).fetchone() is None
        assert db.execute("SELECT count(*) FROM house_numbering.issuance_records WHERE number_id=%s",(number,)).fetchone()[0]==2
        assert not db.execute("SELECT has_table_privilege(current_user,'house_numbering.issuance_events','UPDATE')").fetchone()[0]
        assert not db.execute("SELECT has_table_privilege(current_user,'house_numbering.issuance_records','DELETE')").fetchone()[0]
        db.execute("SELECT set_config('openvietgis.project_id',%s,true)",(other_project,))
        assert db.execute("SELECT count(*) FROM house_numbering.issuance_records").fetchone()[0]==0
        assert query(db,stable,[org,project,number]).fetchone() is None
    print('PASS exact issuance SQL: duplicate protection, ordered stages, void lookup, atomic replacement rollback, single successor, immutable grants, RLS')
