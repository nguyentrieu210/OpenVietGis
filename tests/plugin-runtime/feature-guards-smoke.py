#!/usr/bin/env python3
"""Exercise real migrations, upgrade/downgrade and generic guard on isolated PostGIS.
Requires disposable PostgreSQL connection supplied explicitly by OVG_GUARD_TEST_DSN.
"""
import importlib.util
import os
import threading
import time
from pathlib import Path
import psycopg
from alembic import op

ROOT = Path(__file__).resolve().parents[2]

def run_migration(conn, path, method):
    spec = importlib.util.spec_from_file_location('test_migration', ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    original = op.execute
    try:
        op.execute = lambda sql: conn.execute(sql)
        getattr(module, method)()
    finally:
        op.execute = original

CORE='apps/api/migrations/versions/0047_generic_feature_guards.py'
OLD='modules/house_numbering/migrations/versions/0011_survey_signoffs.py'
NEW='modules/house_numbering/migrations/versions/0012_generic_edit_guards.py'
PERMISSIONS='apps/api/migrations/versions/0048_module_permission_policies.py'
ORG='00000000-0000-4000-8000-000000000001'
PROJECT='00000000-0000-4000-8000-000000000002'
SOURCE='00000000-0000-4000-8000-000000000003'
ACTOR='00000000-0000-4000-8000-000000000004'
SIGNOFF='00000000-0000-4000-8000-000000000005'
OTHER='00000000-0000-4000-8000-000000000006'

def blocked(conn, sql, params=()):
    try:
        with conn.transaction():
            conn.execute(sql, params)
    except psycopg.Error as e:
        assert e.sqlstate == 'OV409', str(e)
    else:
        raise AssertionError('expected guard conflict')

with psycopg.connect(os.environ['OVG_GUARD_TEST_DSN'], autocommit=True) as conn:
    # Require a fresh dedicated fixture DB. Never run on a real application DB.
    assert conn.execute("SELECT current_database() = 'ovg_guard_fixture' AND to_regnamespace('spatial') IS NULL").fetchone()[0]
    conn.execute('''CREATE EXTENSION IF NOT EXISTS postgis; DO $$ BEGIN IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname='openvietgis_runtime') THEN CREATE ROLE openvietgis_runtime NOLOGIN; END IF; END $$;
      CREATE SCHEMA spatial; CREATE SCHEMA extensions; CREATE SCHEMA house_numbering; CREATE SCHEMA workspace;
      CREATE TABLE workspace.projects(organization_id uuid,id uuid,PRIMARY KEY(organization_id,id));
      GRANT USAGE ON SCHEMA extensions TO openvietgis_runtime;
      CREATE TABLE extensions.project_activations(organization_id uuid, project_id uuid, extension_key text,status text);
      CREATE TABLE spatial.reference_features(organization_id uuid,project_id uuid,source_id uuid,feature_id text,
        attributes jsonb NOT NULL,geometry geometry,geometry_state text DEFAULT 'present',content_revision text DEFAULT 'a');''')
    run_migration(conn, OLD, 'upgrade')
    conn.execute('INSERT INTO extensions.project_activations VALUES (%s,%s,%s,%s)',(ORG,PROJECT,'house_numbering','active'))
    for feature,ap,road in [('locked','A ','Road'),('other','B','Road')]:
        conn.execute("INSERT INTO spatial.reference_features VALUES (%s,%s,%s,%s,jsonb_build_object('ap',%s::text,'ten_duong',%s::text,'xu_ly','giu'),ST_Point(1,1),'present','a')",(ORG,PROJECT,SOURCE,feature,ap,road))
    conn.execute('INSERT INTO house_numbering.survey_signoffs(id,organization_id,project_id,source_id,ap,ten_duong,point_count,signed_by_actor_id) VALUES (%s,%s,%s,%s,%s,%s,1,%s)',(SIGNOFF,ORG,PROJECT,SOURCE,'A','Road',ACTOR))
    with conn.transaction():
        run_migration(conn, CORE, 'upgrade')
        run_migration(conn, NEW, 'upgrade')
    assert conn.execute('SELECT count(*) FROM extensions.feature_guard_locks').fetchone()[0]==1
    assert conn.execute("SELECT count(*) FROM pg_trigger WHERE tgrelid='spatial.reference_features'::regclass AND tgname LIKE 'trg_hn_%'").fetchone()[0]==0
    blocked(conn,"UPDATE spatial.reference_features SET attributes=attributes||'{\"so_nha\":\"12\"}' WHERE feature_id='locked'")
    blocked(conn,"UPDATE spatial.reference_features SET geometry=ST_Point(2,2) WHERE feature_id='locked'")
    blocked(conn,"DELETE FROM spatial.reference_features WHERE feature_id='locked'")
    blocked(conn,"UPDATE spatial.reference_features SET attributes=attributes||'{\"ap\":\"A\"}' WHERE feature_id='other'")
    blocked(conn,"INSERT INTO spatial.reference_features SELECT organization_id,project_id,source_id,'new',attributes,geometry,geometry_state,content_revision FROM spatial.reference_features WHERE feature_id='locked'")
    conn.execute("UPDATE spatial.reference_features SET attributes=attributes||'{\"new_field\":null}'")
    conn.execute("UPDATE spatial.reference_features SET content_revision='b'")
    conn.execute("UPDATE spatial.reference_features SET attributes=attributes||'{\"so_nha\":\"10\"}' WHERE feature_id='other'")
    # No permission to insert arbitrary guard policies through runtime SQL.
    assert not conn.execute("SELECT has_table_privilege('openvietgis_runtime','extensions.feature_guard_locks','INSERT')").fetchone()[0]
    # A second independent module can lock an unrelated feature using different field names.
    conn.execute("INSERT INTO extensions.feature_guard_providers VALUES ('fixture_module','fixture:',ARRAY['zone'])")
    conn.execute("INSERT INTO extensions.project_activations VALUES (%s,%s,'fixture_module','active')",(ORG,PROJECT))
    conn.execute("INSERT INTO spatial.reference_features VALUES (%s,%s,%s,'generic','{\"zone\":\"Z\"}',ST_Point(1,1),'present','a')",(ORG,PROJECT,SOURCE))
    conn.execute("INSERT INTO extensions.feature_guard_locks VALUES ('fixture_module',%s,%s,%s,%s,'{\"zone\":\"Z\"}','generic lock')",(OTHER,ORG,PROJECT,SOURCE))
    blocked(conn,"DELETE FROM spatial.reference_features WHERE feature_id='generic'")
    conn.execute("DELETE FROM extensions.feature_guard_locks WHERE extension_key='fixture_module'")
    conn.execute("DELETE FROM extensions.feature_guard_providers WHERE extension_key='fixture_module'")
    conn.execute("UPDATE extensions.project_activations SET status='disabled' WHERE extension_key='house_numbering'")
    conn.execute("UPDATE spatial.reference_features SET geometry=ST_Point(3,3) WHERE feature_id='locked'")
    conn.execute("UPDATE extensions.project_activations SET status='active' WHERE extension_key='house_numbering'")
    blocked(conn,"DELETE FROM spatial.reference_features WHERE feature_id='locked'")
    conn.execute("UPDATE house_numbering.survey_signoffs SET revoked_at=now(),revoked_by_actor_id=%s,revoke_reason='test' WHERE id=%s",(ACTOR,SIGNOFF))
    assert conn.execute('SELECT count(*) FROM extensions.feature_guard_locks').fetchone()[0]==0
    conn.execute("UPDATE spatial.reference_features SET geometry=ST_Point(4,4) WHERE feature_id='locked'")
    # New signoff is mirrored and downgrade restores original trigger without losing signoff history.
    conn.execute('INSERT INTO house_numbering.survey_signoffs(id,organization_id,project_id,source_id,ap,ten_duong,point_count,signed_by_actor_id) VALUES (%s,%s,%s,%s,%s,%s,1,%s)',(OTHER,ORG,PROJECT,SOURCE,'A','Road',ACTOR))
    blocked(conn,"DELETE FROM spatial.reference_features WHERE feature_id='locked'")
    # A first signoff for a formerly unsigned scope races a writer. The writer
    # must wait for the exclusive source lock and then observe the committed guard.
    race_id='00000000-0000-4000-8000-000000000007'
    outcome=[]; started=threading.Event()
    def attempt():
        with psycopg.connect(os.environ['OVG_GUARD_TEST_DSN'],autocommit=True) as writer:
            started.set()
            try:
                writer.execute("UPDATE spatial.reference_features SET geometry=ST_Point(9,9) WHERE feature_id='other'")
                outcome.append('write succeeded')
            except psycopg.Error as error:
                outcome.append(error.sqlstate)
    with conn.transaction():
        conn.execute("SELECT pg_advisory_xact_lock(hashtextextended('hn_signoff:'||%s,0))",(SOURCE,))
        thread=threading.Thread(target=attempt,daemon=True);thread.start();assert started.wait(3)
        time.sleep(.3);assert thread.is_alive(),'writer did not wait on source lock'
        conn.execute('INSERT INTO house_numbering.survey_signoffs(id,organization_id,project_id,source_id,ap,ten_duong,point_count,signed_by_actor_id) VALUES (%s,%s,%s,%s,%s,%s,1,%s)',(race_id,ORG,PROJECT,SOURCE,'B','Road',ACTOR))
    thread.join(5);assert outcome==['OV409'],outcome
    conn.execute('UPDATE house_numbering.survey_signoffs SET revoked_at=now(),revoked_by_actor_id=%s,revoke_reason=%s WHERE id=%s',(ACTOR,'test',race_id))
    # Scope and downgrade guard for explicit module permissions.
    conn.execute('INSERT INTO workspace.projects VALUES (%s,%s),(%s,%s)',(ORG,PROJECT,ORG,OTHER))
    run_migration(conn,PERMISSIONS,'upgrade')
    conn.execute('INSERT INTO extensions.module_permission_policies(organization_id,project_id,extension_key,actor_type,actor_id,permissions,updated_by_actor_id) VALUES (%s,%s,%s,%s,%s,%s,%s)',(ORG,PROJECT,'house_numbering','human',ACTOR,['ext.house_numbering.read'],ACTOR))
    conn.execute('SET ROLE openvietgis_runtime')
    conn.execute("SELECT set_config('openvietgis.organization_id',%s,false),set_config('openvietgis.project_id',%s,false)",(ORG,OTHER))
    assert conn.execute('SELECT count(*) FROM extensions.module_permission_policies').fetchone()[0]==0
    conn.execute("SELECT set_config('openvietgis.project_id',%s,false)",(PROJECT,))
    assert conn.execute('SELECT permissions FROM extensions.module_permission_policies').fetchone()[0]==['ext.house_numbering.read']
    conn.execute('RESET ROLE')
    try:
        with conn.transaction():run_migration(conn,PERMISSIONS,'downgrade')
    except psycopg.Error as error:assert 'refusing security downgrade' in str(error)
    else:raise AssertionError('permission downgrade widened access')
    conn.execute('UPDATE extensions.module_permission_policies SET permissions=NULL')
    run_migration(conn,PERMISSIONS,'downgrade')
    with conn.transaction():
        run_migration(conn, NEW, 'downgrade')
        run_migration(conn, CORE, 'downgrade')
    blocked(conn,"DELETE FROM spatial.reference_features WHERE feature_id='locked'")
    assert conn.execute('SELECT count(*) FROM house_numbering.survey_signoffs').fetchone()[0]==3
    print('PASS real PostGIS: legacy backfill, old/new scope, writes, additive null, runtime privilege, second provider, disable/revoke, concurrent signoff, permission RLS and downgrade, signoff retention')
