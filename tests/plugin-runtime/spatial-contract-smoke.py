#!/usr/bin/env python3
"""Versioned module spatial contract: real migrated PostGIS, parity and RLS."""
import os
import sys
import tempfile
from pathlib import Path
from statistics import median
from time import perf_counter
import psycopg
import re
from psycopg.types.json import Jsonb

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tests/rust-parity'))
sys.path.insert(0,str(ROOT/'tools/benchmark/rust-platform'))
import run as parity
import benchmark_runner as baseline

def scope(db,org,project,actor):
    db.execute("SELECT set_config('openvietgis.organization_id',%s,true),set_config('openvietgis.project_id',%s,true),set_config('openvietgis.actor_user_id',%s,true),set_config('app.current_organization_id',%s,true),set_config('app.current_user_id',%s,true)",(org,project,actor,org,actor))

with tempfile.TemporaryDirectory(prefix='ovg-spatial-contract-') as directory:
    with parity.database(os.environ.get('OVG_PARITY_PG_ADMIN_URL',baseline.DEFAULT_ADMIN_PG_URL)) as (owner,runtime):
        work=Path(directory);artifacts=work/'artifacts';artifacts.mkdir()
        parity.configure_python(owner,runtime,artifacts);baseline.run_alembic_upgrade(owner,runtime)
        fixture=baseline.seed_benchmark_database(owner,seed=20261128,scales=['30k'],csrf_secret=baseline.DEFAULT_CSRF_SECRET)
        org,project,actor=(fixture[k] for k in ('org_a_id','project_a_id','user_a_id'))
        source=fixture['layers']['epsg4326_30k']['source_id']
        with psycopg.connect(runtime) as db:
            assert db.execute("SELECT current_user='openvietgis_runtime'").fetchone()[0]
            scope(db,org,project,actor)
            base="SELECT count(*),sum(ST_X(geometry)) FROM {} WHERE organization_id=%s AND project_id=%s AND source_id=%s"
            args=(org,project,source)
            old=db.execute(base.format('spatial.reference_features'),args).fetchone()
            new=db.execute(base.format('extensions.spatial_features_v1'),args).fetchone()
            assert old==new and old[0]==30000,(old,new)
            query="""SELECT f.feature_id, f.attributes, ST_AsEWKB(f.geometry), f.content_revision, s.geometry_family
              FROM {} f JOIN {} s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
              WHERE f.organization_id=%s AND f.project_id=%s AND f.source_id=%s ORDER BY f.feature_id"""
            direct=db.execute(query.format('spatial.reference_features','spatial.data_sources'),args).fetchall()
            contract=db.execute(query.format('extensions.spatial_features_v1','extensions.spatial_sources_v1'),args).fetchall()
            assert direct==contract
            timings={'base':[],'view':[]}
            for i in range(7):
                order=[('base','spatial.reference_features'),('view','extensions.spatial_features_v1')]
                if i%2:order.reverse()
                for name,table in order:
                    start=perf_counter();db.execute(base.format(table),args).fetchone();timings[name].append((perf_counter()-start)*1000)
            print('30k aggregation median ms:',{k:round(median(v),2) for k,v in timings.items()})
            for view in ('spatial_features_v1','spatial_sources_v1'):
                assert db.execute("SELECT has_table_privilege(current_user,%s,'SELECT')",('extensions.'+view,)).fetchone()[0]
                assert not db.execute("SELECT has_table_privilege(current_user,%s,'UPDATE')",('extensions.'+view,)).fetchone()[0]
            assert db.execute("SELECT count(*) FROM extensions.spatial_features_v1 WHERE organization_id=%s",(fixture['org_b_id'],)).fetchone()[0]==0
            scope(db,fixture['org_b_id'],fixture['project_b_id'],fixture['user_b_id'])
            assert db.execute("SELECT count(*) FROM extensions.spatial_features_v1 WHERE source_id=%s",(source,)).fetchone()[0]==0
            scope(db,'','','')
            assert db.execute("SELECT count(*) FROM extensions.spatial_features_v1").fetchone()[0]==0
        # Exercise the exact Core lock SQL, not a hand-written substitute.
        # The original API transaction holds the lock until rollback/commit.
        rust=(ROOT/'crates/ovg-storage/src/module_ports.rs').read_text()
        locks=re.findall(r'"(SELECT f.source_id,f.feature_id FROM jsonb_to_recordset.*?FOR SHARE OF f(?:,ds)?)"',rust,re.S)
        assert len(locks)==2
        for lock in locks:
            order=[int(x)-1 for x in re.findall(r'\$(\d+)',lock)]
            sql=re.sub(r'\$\d+','%s',lock)
            with psycopg.connect(runtime) as db:
                scope(db,org,project,actor)
                feature_id=direct[0][0]
                refs=[{'source_id':source,'feature_id':feature_id},{'source_id':fixture['layers']['tenant_b_1k']['source_id'],'feature_id':feature_id}]
                parameters=[org,project,Jsonb(refs)]
                locked=db.execute(sql,[parameters[i] for i in order]).fetchall()
                assert len(locked)==1 and str(locked[0][0])==source and locked[0][1]==feature_id,locked
                try:
                    with psycopg.connect(owner) as writer:
                        writer.execute("SET LOCAL lock_timeout='150ms'")
                        writer.execute("SELECT 1 FROM spatial.reference_features WHERE source_id=%s AND feature_id=%s FOR UPDATE",(source,feature_id))
                except psycopg.errors.LockNotAvailable:pass
                else:raise AssertionError("Core port did not hold feature lock")
        # Scoped membership without any GIS grant must not gain access through views.
        with psycopg.connect(owner) as db:
            db.execute("UPDATE org.organization_memberships SET role='member' WHERE organization_id=%s AND user_id=%s",(org,actor))
            db.execute("INSERT INTO gis_access.scoped_members(organization_id,project_id,user_id) VALUES(%s,%s,%s)",(org,project,actor))
        with psycopg.connect(runtime) as db:
            scope(db,org,project,actor)
            assert db.execute("SELECT gis_access.scoped(%s,%s)",(org,project)).fetchone()[0]
            assert db.execute("SELECT count(*) FROM extensions.spatial_features_v1").fetchone()[0]==0
            assert db.execute("SELECT count(*) FROM extensions.spatial_sources_v1").fetchone()[0]==0
        print('PASS exact Core lock SQL: foreign feature excluded, row lock held; scoped GIS member without grants sees no rows')
        print('PASS 30k fixture: identical feature/source values, invoker RLS, read-only contract, unset tenant denies access')
