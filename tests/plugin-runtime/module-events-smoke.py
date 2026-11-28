#!/usr/bin/env python3
"""Real PostgreSQL test for commit-ordered module configuration events and RLS."""
import importlib.util
import os
from pathlib import Path
import re
import threading
import time
from uuid import uuid4
import psycopg
from psycopg import ClientCursor
from alembic import op

ROOT=Path(__file__).resolve().parents[2]
DSN=os.environ["OVG_EVENTS_TEST_DSN"]
ORG,PROJECT,OTHER,ACTOR=[str(uuid4()) for _ in range(4)]
def migration(conn,name,method="upgrade"):
    path=ROOT/"apps/api/migrations/versions"/name
    spec=importlib.util.spec_from_file_location("fixture_migration",path)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    old=op.execute
    try:
        op.execute=lambda sql:conn.execute(sql)
        getattr(module,method)()
    finally:op.execute=old

with psycopg.connect(DSN,autocommit=True,cursor_factory=ClientCursor) as conn:
    assert conn.execute("SELECT current_database()='ovg_events_fixture' AND to_regnamespace('extensions') IS NULL").fetchone()[0]
    conn.execute("""DO $$ BEGIN IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='openvietgis_runtime') THEN CREATE ROLE openvietgis_runtime NOLOGIN; END IF; END $$;
      CREATE SCHEMA extensions;CREATE SCHEMA workspace;CREATE SCHEMA spatial;
      CREATE TABLE workspace.projects(organization_id uuid,id uuid,PRIMARY KEY(organization_id,id));
      CREATE TABLE spatial.data_sources(organization_id uuid,project_id uuid,id uuid,PRIMARY KEY(organization_id,project_id,id));
      CREATE TABLE extensions.project_activations(organization_id uuid,project_id uuid,extension_key text,status text,PRIMARY KEY(organization_id,project_id,extension_key));
      GRANT USAGE ON SCHEMA extensions TO openvietgis_runtime;
      GRANT SELECT,INSERT,UPDATE ON extensions.project_activations TO openvietgis_runtime;""")
    conn.execute("INSERT INTO workspace.projects VALUES (%s,%s),(%s,%s)",(ORG,PROJECT,ORG,OTHER))
    migration(conn,"0048_module_permission_policies.py")
    migration(conn,"0049_module_layer_bindings.py")
    migration(conn,"0050_module_change_stream.py")
    rust=(ROOT/"crates/ovg-storage/src/extensions/events.rs").read_text()
    sql=re.search(r'query_scalar\(\s*"(WITH stream AS .*?)"\s*\)',rust,re.S).group(1)
    order=[int(v)-1 for v in re.findall(r'\$(\d+)',sql)]
    sql=re.sub(r'\$\d+','%s',sql)
    def replay(epoch=None,seq=0,limit=100,project=PROJECT):
        params=[ORG,project,epoch,seq,limit]
        # ClientCursor renders NULL as untyped; pin UUID at comparison.
        return conn.execute(sql,[params[i] for i in order]).fetchone()[0]
    assert replay()["head"]=="0"
    def insert_activation(c,key,project=PROJECT):
        c.execute("INSERT INTO extensions.project_activations VALUES (%s,%s,%s,'active')",(ORG,project,key))
    with conn.transaction(force_rollback=True):
        insert_activation(conn,"rolled_back")
    assert replay()["head"]=="0"
    insert_activation(conn,"initial")
    first=replay();epoch=first["epoch"];assert first["head"]=="1"
    # Hold the project stream counter lock while another transaction writes.
    started=threading.Event();outcome=[]
    def writer():
        with psycopg.connect(DSN,autocommit=True) as other:
            started.set()
            insert_activation(other,"second")
            outcome.append("committed")
    with conn.transaction():
        conn.execute("UPDATE extensions.project_activations SET status='disabled' WHERE extension_key='initial'")
        t=threading.Thread(target=writer,daemon=True);t.start();assert started.wait(3)
        time.sleep(.3);assert t.is_alive()
        with psycopg.connect(DSN) as reader:
            assert reader.execute("SELECT head FROM extensions.module_change_streams WHERE project_id=%s",(PROJECT,)).fetchone()[0]==1
    t.join(5);assert outcome==["committed"]
    page=replay(epoch,1,1);assert page["head"]=="3" and page["items"]==[{"sequence":"2","extension_key":"initial","kind":"activation"}]
    assert replay(epoch,2)["items"]==[{"sequence":"3","extension_key":"second","kind":"activation"}]
    conn.execute("INSERT INTO extensions.module_permission_policies(organization_id,project_id,extension_key,actor_type,actor_id,permissions,updated_by_actor_id) VALUES(%s,%s,'initial','human',%s,ARRAY['secret.permission'],%s)",(ORG,PROJECT,ACTOR,ACTOR))
    conn.execute("INSERT INTO extensions.module_binding_sets(organization_id,project_id,extension_key,revision,updated_by_actor_id) VALUES(%s,%s,'initial',1,%s)",(ORG,PROJECT,ACTOR))
    assert [i["kind"] for i in replay(epoch,3)["items"]]==["permissions","bindings"]
    assert "secret.permission" not in str(replay(epoch,0))
    insert_activation(conn,"foreign",OTHER)
    assert replay(epoch,0)["head"]=="5"
    conn.execute("SET ROLE openvietgis_runtime")
    conn.execute("SELECT set_config('openvietgis.organization_id',%s,false),set_config('openvietgis.project_id',%s,false)",(ORG,PROJECT))
    assert conn.execute("SELECT count(*) FROM extensions.module_change_events").fetchone()[0]==5
    assert replay(epoch,0,project=OTHER)["head"]=="0"
    assert not conn.execute("SELECT has_table_privilege('openvietgis_runtime','extensions.module_change_events','INSERT')").fetchone()[0]
    assert not conn.execute("SELECT has_table_privilege('openvietgis_runtime','extensions.module_change_streams','UPDATE')").fetchone()[0]
    insert_activation(conn,"runtime_writer")
    assert replay(epoch,5)["items"]==[{"sequence":"6","extension_key":"runtime_writer","kind":"activation"}]
    conn.execute("RESET ROLE")
    try:
        with conn.transaction():migration(conn,"0050_module_change_stream.py","downgrade")
    except psycopg.Error as e:assert "Module change history exists" in str(e)
    else:raise AssertionError("downgrade erased durable events")
    print("PASS real PostgreSQL: rollback, serialized commit order, pagination, all event kinds, RLS, payload privacy, runtime trigger, guarded downgrade")
