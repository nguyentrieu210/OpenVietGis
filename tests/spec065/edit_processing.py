"""Actual Rust/PostGIS edit, vector processing and raster acceptance on disposable fixtures."""
import argparse
import base64
import importlib.util
import json
import math
import os
import shutil
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from tempfile import TemporaryDirectory
from uuid import UUID, uuid4

import psycopg
from attribute_query import (
    OWNER,
    ROOT,
    RUNTIME,
    baseline,
    configure_python,
    project_path,
    rust_client,
    session_headers,
)
from run import scoped_member, verify_database_role


def fixture_layers(f):
    with psycopg.connect(OWNER) as conn:
        def layer(key, family, rows):
            data = [(fid, json.dumps({"code": fid, "ward": ward, "metric_val": i, "active": True}), "SRID=3857;" + geom)
                    for i, (fid, ward, geom) in enumerate(rows)]
            return baseline._seed_layer(conn.cursor(), seed=65002, org_id=UUID(f["org_a_id"]),
                proj_id=UUID(f["project_a_id"]), ref_key=key, name=key, kind="manual", crs=3857,
                family=family, count=len(data), extent={"min_x":0,"min_y":0,"max_x":300,"max_y":100,"crs_code":3857},
                validation_state="ready", validation_reason=None, rows_iter=data)
        parcels=layer("spec065_parcels","Polygon",[
            ("p1","A","POLYGON((0 0,100 0,100 100,0 100,0 0))"),
            ("p2","A","POLYGON((100 0,200 0,200 100,100 100,100 0))"),
            ("p3","B","POLYGON((200 0,300 0,300 100,200 100,200 0))")])
        clip=layer("spec065_clip","Polygon",[("clip","A","POLYGON((50 0,150 0,150 100,50 100,50 0))")])
        point=layer("spec065_point","Point",[("pt","A","POINT(50 50)")])
    return parcels,clip,point


def check_processing(post,get,foreign,fixtures):
    parcels,clip,point=fixtures
    assert {r["id"] for r in get("/processing/algorithms")["items"]}=={"buffer","clip","dissolve"}
    def pin(layer):
        return {k: layer[k] for k in ("source_id", "content_revision", "schema_revision")}
    def job(algorithm,layer,parameters,overlay=None,execute=True,op=None):
        return {"expected_input":pin(layer), "expected_overlay":pin(overlay) if overlay else None, "algorithm_id":algorithm,"input_layer_id":layer["layer_id"],"overlay_layer_id":overlay["layer_id"] if overlay else None,
                "parameters":parameters,"output_name":"spec065 "+algorithm,"operation_id":op or str(uuid4()),"execute_now":execute}
    def result(body,count,area):
        r=post("/processing/jobs",body)
        assert r["status"]=="succeeded",(r.get("error_code"),r.get("error_message"),r)
        assert r["feature_count"]==count
        assert post("/processing/jobs",body)["id"]==r["id"]
        post("/processing/jobs",body|{"output_name":"changed"},expected=409)
        post("/processing/jobs",body,foreign,404)
        with psycopg.connect(OWNER) as conn:
            n,a=conn.execute("SELECT count(*),sum(ST_Area(r.geometry)) FROM spatial.reference_features r JOIN spatial.layers l ON l.source_id=r.source_id AND l.organization_id=r.organization_id AND l.project_id=r.project_id WHERE l.id=%s",(r["output_layer_id"],)).fetchone()
            assert n==count and math.isclose(a,area,abs_tol=1e-6),(n,a,area)
        return r, {"features": n, "area_m2": a}
    stale = job("buffer",point,{"distance_meters":10,"quad_segs":8})
    stale["expected_input"]["content_revision"] = "0"*64
    post("/processing/jobs",stale,expected=409)
    _, buffer_metrics = result(job("buffer",point,{"distance_meters":10,"quad_segs":8}),1,1600*math.sin(math.pi/16))
    _, clip_metrics = result(job("clip",parcels,{},clip),2,10000)
    dissolved, dissolve_metrics = result(job("dissolve",parcels,{"group_by_field":"ward"}),2,30000)
    with psycopg.connect(OWNER) as conn:
        areas = dict(conn.execute(
            "SELECT r.attributes->>'group_value',sum(ST_Area(r.geometry)) "
            "FROM spatial.reference_features r JOIN spatial.layers l ON l.source_id=r.source_id "
            "AND l.organization_id=r.organization_id AND l.project_id=r.project_id "
            "WHERE l.id=%s GROUP BY 1", (dissolved["output_layer_id"],)
        ).fetchall())
    assert set(areas)=={"A","B"} and math.isclose(areas["A"],20000,abs_tol=1e-6)
    assert math.isclose(areas["B"],10000,abs_tol=1e-6)
    print("PASS actual Rust jobs Buffer/Clip/Dissolve, golden counts/areas, idempotent replay, tenant isolation",flush=True)
    queued=post("/processing/jobs",job("buffer",point,{"distance_meters":10,"quad_segs":8},execute=False))
    assert queued["status"]=="queued"
    cancelled=post("/processing/jobs/"+queued["id"]+"/cancel",{},expected=200)
    assert cancelled["status"]=="cancelled"
    retried=post("/processing/jobs/"+queued["id"]+"/retry",{},expected=200)
    assert retried["status"]=="succeeded" and retried["id"]==queued["id"]
    post("/processing/jobs/"+queued["id"]+"/cancel",{},expected=409)
    print("PASS queued/cancel/retry/terminal job lifecycle",flush=True)
    return {"buffer":buffer_metrics,"clip":clip_metrics,
            "dissolve_zone":{"features":dissolve_metrics["features"],"areas_m2":areas}}

def verify_processing_fixture(fixtures,directory):
    import hashlib

    spec=importlib.util.spec_from_file_location("spec065_reference_fixture",ROOT/"fixtures/qgis-parity/generate.py")
    gen=importlib.util.module_from_spec(spec);spec.loader.exec_module(gen)
    root=directory/"processing-reference";gen.generate(root)
    with psycopg.connect(OWNER) as conn:
        for name, layer in zip(("parcels","clip","point"),fixtures,strict=True):
            expected=json.loads((root/(name+".geojson")).read_text())["features"]
            n=conn.execute(
                "SELECT count(*) FROM spatial.reference_features WHERE source_id=%s",
                (layer["source_id"],),
            ).fetchone()[0]
            assert n==len(expected),(name,n)
            for feature in expected:
                fid="pt" if name=="point" else feature["id"]
                matched=conn.execute(
                    "SELECT ST_Equals(geometry,ST_SetSRID(ST_GeomFromGeoJSON(%s),3857)),attributes->>'ward' "
                    "FROM spatial.reference_features WHERE source_id=%s AND feature_id=%s AND ST_SRID(geometry)=3857",
                    (json.dumps(feature["geometry"]),layer["source_id"],fid),
                ).fetchone()
                assert matched and matched[0],(name,fid,"geometry mismatch")
                if name=="parcels":
                    assert matched[1]==feature["properties"]["zone"],(name,fid,"group mismatch")
    return hashlib.sha256((root/"SHA256SUMS.txt").read_bytes()).hexdigest()


def processing_only(c,f,fixtures):
    auth=session_headers(f["session_a_token"], **{"x-openvietgis-csrf":f["csrf_a_token"]})
    foreign=session_headers(f["session_b_token"], **{"x-openvietgis-csrf":f["csrf_b_token"]})
    path=project_path(f["org_a_id"],f["project_a_id"])

    def post(endpoint,body,headers=auth,expected=201):
        r=c.post(path+endpoint,json=body,headers=headers,timeout=60)
        assert r.status_code==expected,(endpoint,r.status_code,r.text[:600])
        return r.json()

    def get(endpoint):
        r=c.get(path+endpoint,headers=auth)
        assert r.status_code==200,(endpoint,r.status_code,r.text[:600])
        return r.json()

    return check_processing(post,get,foreign,fixtures)


def check(c,f,second,fixtures,directory):
    from openvietgis_api.security.csrf import make_csrf_token
    _uid,_,token=second
    auth=session_headers(f["session_a_token"], **{"x-openvietgis-csrf":f["csrf_a_token"]})
    other=session_headers(token, **{"x-openvietgis-csrf":make_csrf_token(token,baseline.DEFAULT_CSRF_SECRET)})
    foreign=session_headers(f["session_b_token"], **{"x-openvietgis-csrf":f["csrf_b_token"]})
    path=project_path(f["org_a_id"],f["project_a_id"])
    def post(endpoint,body,headers=auth,expected=201):
        r=c.post(path+endpoint,json=body,headers=headers,timeout=60)
        assert r.status_code==expected,(endpoint,r.status_code,r.text[:600])
        return r.json()
    def get(endpoint,headers=auth,expected=200):
        r=c.get(path+endpoint,headers=headers)
        assert r.status_code==expected,(endpoint,r.status_code,r.text[:600])
        return r.json()
    edit=f["layers"]["edit_4326"]
    source=edit["source_id"]; endpoint="/sources/"+source+"/changesets"
    with psycopg.connect(OWNER) as conn:
        fid,attrs=conn.execute("SELECT feature_id,attributes FROM spatial.reference_features WHERE source_id=%s ORDER BY feature_id LIMIT 1",(source,)).fetchone()
    def update(op,rank,rev=edit["content_revision"]):
        return {"operation_id":op,"base_content_revision":rev,"base_schema_revision":edit["schema_revision"],
            "summary":"spec065 two editor conflict","operations":[{"op_type":"update","feature_id":fid,"properties":attrs|{"metric_val":rank}}]}
    a,b=update("spec065-edit-a",101),update("spec065-edit-b",202)
    def send(body,headers):return c.post(path+endpoint,json=body,headers=headers,timeout=30)
    with ThreadPoolExecutor(max_workers=2) as pool:
        fa=pool.submit(send,a,auth);fb=pool.submit(send,b,other);ra,rb=fa.result(),fb.result()
    assert sorted([ra.status_code,rb.status_code])==[201,409],(ra.status_code,ra.text[:300],rb.status_code,rb.text[:300])
    winner,body,who,loser=(ra.json(),a,auth,other) if ra.status_code==201 else (rb.json(),b,other,auth)
    assert winner["status"]=="committed"
    assert post(endpoint,body,who)["id"]==winner["id"]
    post(endpoint,body|{"summary":"different"},who,409)
    post(endpoint,body,foreign,404)
    post(endpoint,body,{},401)
    rev=winner["resulting_content_revision"]
    with psycopg.connect(OWNER) as conn:
        assert conn.execute("SELECT attributes->>'metric_val' FROM spatial.reference_features WHERE source_id=%s AND feature_id=%s",(source,fid)).fetchone()[0]==str(body["operations"][0]["properties"]["metric_val"])
    # Atomic rollback: first operation valid, second nonexistent.
    bad=update("spec065-atomic",999,rev)
    bad["operations"].append({"op_type":"delete","feature_id":"not-existing"})
    r=send(bad,loser);assert r.status_code in (404,409,422),(r.status_code,r.text[:300])
    with psycopg.connect(OWNER) as conn:
        assert conn.execute("SELECT content_revision FROM spatial.data_sources WHERE id=%s",(source,)).fetchone()[0]==rev
        assert conn.execute("SELECT count(*) FROM editing.changesets WHERE source_id=%s AND operation_id='spec065-atomic'",(source,)).fetchone()[0]==0
    revert=post("/changesets/"+winner["id"]+"/revert",{"operation_id":"spec065-revert"},who)
    assert revert["status"]=="reverted" and revert["reverted_from_changeset_id"]==winner["id"]
    assert post("/changesets/"+winner["id"]+"/revert",{"operation_id":"spec065-revert"},who)["id"]==revert["id"]
    with psycopg.connect(OWNER) as conn:
        assert conn.execute("SELECT attributes FROM spatial.reference_features WHERE source_id=%s AND feature_id=%s",(source,fid)).fetchone()[0]==attrs
    print("PASS two distinct authorized editor sessions, one commit/one 409, exact replay, atomic rollback, persisted revert, foreign denial",flush=True)
    check_processing(post,get,foreign,fixtures)
    # Real GDAL worker, candidate runs under unprivileged uid.
    spec=importlib.util.spec_from_file_location("spec065_fixtures",ROOT/"fixtures/qgis-parity/generate.py")
    gen=importlib.util.module_from_spec(spec);spec.loader.exec_module(gen)
    raster=directory/"raster";gen.generate(raster)
    files={"file":("nir.tif",(raster/"nir.tif").read_bytes(),"image/tiff"),
           "second":("red.tif",(raster/"red.tif").read_bytes(),"image/tiff")}
    endpoint=path+"/raster/analyze"
    r=c.post(endpoint,data={"method":"ndvi","band_a":"1","band_b":"1"},files=files,headers=auth,timeout=60)
    assert r.status_code==200,(r.status_code,r.text[:600])
    d=r.json();assert d["width"]==d["height"]==3 and d["epsg"]==3857 and d["persisted"] is False
    assert d["stats"]["valid"]==8 and d["stats"]["nodata"]==1 and math.isclose(d["stats"]["mean"],.5,abs_tol=1e-6)
    assert base64.b64decode(d["tiff"])[:4] in (b"II*\x00",b"MM\x00*")
    r=c.post(endpoint,data={"method":"ndvi","band_a":"1","band_b":"1"},files=files,headers=foreign,timeout=30)
    assert r.status_code==404,(r.status_code,r.text[:300])
    print("PASS authenticated raster NDVI, 8 valid / 1 NoData, mean .5, real GeoTIFF and tenant denial",flush=True)
    def store(data,files=None):
        parts={k:(None,v) for k,v in data.items()} | (files or {})
        r=c.post(path+"/raster/layers",files=parts,headers=auth,timeout=70)
        assert r.status_code==201,(r.status_code,r.text[:600])
        return r.json()
    # Persist source bands, then derive an index from their IDs, preserving lineage.
    nir=store({"method":"band","name":"QA NIR","band_a":"1"},{"file":files["file"]})
    red=store({"method":"band","name":"QA Red","band_a":"1"},{"file":files["second"]})
    index=store({"method":"ndvi","name":"QA derived NDVI","source_id":nir["id"],"second_id":red["id"],"band_a":"1","band_b":"1"})
    assert index["inputs"]["a"]==nir["id"] and index["inputs"]["b"]==red["id"]
    assert index["stats"]["valid"]==8 and index["stats"]["nodata"]==1 and math.isclose(index["stats"]["mean"],.5,abs_tol=1e-6)
    rasters=get("/raster/layers")
    items=rasters["items"] if isinstance(rasters,dict) else rasters
    assert {nir["id"],red["id"],index["id"]}<={r["id"] for r in items}
    download=c.get(path+"/raster/layers/"+index["id"]+"/file",headers=auth)
    assert download.status_code==200 and download.content[:4] in (b"II*\x00",b"MM\x00*")
    assert c.get(path+"/raster/layers/"+index["id"]+"/file",headers=foreign).status_code==404
    dem=store({"method":"band","name":"QA DEM","band_a":"1"},{"file":("dem.tif",(raster/"dem.tif").read_bytes(),"image/tiff")})
    slope=store({"method":"slope","name":"QA slope","source_id":dem["id"],"band_a":"1"})
    aspect=store({"method":"aspect","name":"QA aspect","source_id":dem["id"],"band_a":"1"})
    assert math.isclose(slope["stats"]["mean"],45,abs_tol=1e-5) and math.isclose(aspect["stats"]["mean"],270,abs_tol=1e-5)
    print("PASS persisted raster inputs/NDVI lineage and download, slope 45°, aspect 270°, foreign denial",flush=True)
    # Real layout persistence and optimistic concurrency across the same two users.
    layout="/cartography/designer";m1,m2=str(uuid4()),str(uuid4())
    document={"version":1,"paper":"A4","landscape":True,"dpi":300,"items":[
        {"id":m1,"kind":"map","x":10,"y":10,"w":130,"h":130,"camera":{"lng":106.6,"lat":10.7,"zoom":12,"bearing":0,"reference_width":900}},
        {"id":m2,"kind":"map","x":150,"y":10,"w":130,"h":130,"camera":{"lng":106.7,"lat":10.8,"zoom":16,"bearing":20,"reference_width":900}},
        {"id":str(uuid4()),"kind":"scale","x":10,"y":145,"w":50,"h":12,"map_id":m1},
        {"id":str(uuid4()),"kind":"text","x":10,"y":165,"w":200,"h":15,"text":"Bản đồ kiểm thử"}]}
    saved=post(layout,{"name":"QA layout","document":document})
    assert saved["revision"]==1
    uri=path+layout+"/"+saved["id"]
    updated=c.put(uri,json={"name":"QA layout revision 2","document":document,"revision":1},headers=other)
    assert updated.status_code==200 and updated.json()["revision"]==2,(updated.status_code,updated.text[:500])
    stale=c.put(uri,json={"name":"Stale overwrite","document":document,"revision":1},headers=auth)
    assert stale.status_code==409
    bad_document=document|{"items":document["items"]+[{"id":str(uuid4()),"kind":"north","x":1,"y":1,"w":8,"h":8,"map_id":str(uuid4())}]}
    post(layout,{"name":"Invalid link","document":bad_document},expected=422)
    assert get(layout)["items"][0]["document"]["items"][1]["camera"]["zoom"]==16
    get(layout,foreign,404)
    print("PASS authenticated two-frame layout persistence, invalid map link and stale revision conflict",flush=True)
    # Reuse the existing transfer oracle on the Rust binary only.
    from e07_transfers import fixtures as transfer_fixtures
    from e07_transfers import roundtrip

    fmt,filename,content=next(transfer_fixtures())
    rt=roundtrip(c,f,"Spec065",fmt,filename,content)
    assert rt["stored_crs"]==4326
    props=[v["properties"] for v in rt["geojson"]["features"]]
    assert {"code":"Điểm A","rank":0,"active":False} in props and {"code":"","rank":None,"active":None} in props,props
    print("PASS actual GeoJSON import/inspect/confirm/export/reimport, Unicode, null, false, zero, digest and replay",flush=True)
    # Integration Hub uses a separate bounded codec path for GPKG and DXF.
    import hashlib
    import io
    import sqlite3

    import ezdxf
    from e12_adapter_transfer import _mutate_gpkg_for_sync, _seed_layer
    lid,_sid,src=_seed_layer(c,path,auth,"Spec065 QGIS")
    root="/integration-hub/adapters/qgis/layers/"+lid
    gpkg=c.get(path+root+"/export?format=geopackage",headers=auth,timeout=60)
    assert gpkg.status_code==200,(gpkg.status_code,gpkg.text[:500])
    assert gpkg.content.startswith(b"SQLite format 3\x00")
    assert gpkg.headers["x-content-sha256"]==hashlib.sha256(gpkg.content).hexdigest()
    assert gpkg.headers["x-openvietgis-content-revision"]==src["content_revision"]
    assert gpkg.headers["x-openvietgis-schema-revision"]==src["schema_revision"]
    assert gpkg.headers["x-openvietgis-crs"]=="EPSG:4326"
    assert gpkg.headers["x-openvietgis-feature-count"]=="2"
    g=directory/"export.gpkg";g.write_bytes(gpkg.content)
    with sqlite3.connect(g) as conn:
        table,srs=conn.execute("SELECT table_name,srs_id FROM gpkg_contents WHERE data_type='features'").fetchone()
        assert srs==4326
        quoted='"'+table.replace('"','""')+'"'
        assert conn.execute(f"SELECT ovg_feature_id,fld_name FROM {quoted} ORDER BY ovg_feature_id").fetchall()==[("pt_1","Alpha"),("pt_2","Beta")]
    dxf=c.get(path+"/integration-hub/adapters/autocad/layers/"+lid+"/export?format=dxf",headers=auth,timeout=60)
    assert dxf.status_code==200,(dxf.status_code,dxf.text[:500])
    assert dxf.headers["x-openvietgis-crs"]=="EPSG:32648" and dxf.headers["x-openvietgis-attribute-loss"]=="true"
    assert dxf.headers["x-content-sha256"]==hashlib.sha256(dxf.content).hexdigest()
    assert len([e for e in ezdxf.read(io.StringIO(dxf.content.decode("utf-8"))).modelspace() if e.dxftype()=="POINT"])==2
    sql=c.get(path+"/integration-hub/adapters/postgis/layers/"+lid+"/export?format=postgis_sql",headers=auth,timeout=60)
    assert sql.status_code==200 and b"CREATE TABLE" in sql.content and b"ST_GeomFromText" in sql.content
    assert c.get(path+root+"/export?format=geopackage",headers=foreign).status_code==404
    print("PASS actual Integration Hub GPKG SQLite feature IDs/schema, DXF UTM entities and loss disclosure, PostGIS SQL, digest and tenant denial",flush=True)
    edited=_mutate_gpkg_for_sync(gpkg.content)
    parts={"file":("sync.gpkg",edited,"application/geopackage+sqlite3")}
    pins={"base_content_revision":src["content_revision"],"base_schema_revision":src["schema_revision"]}
    def sync(action,data,headers=auth,expected=200):
        r=c.post(path+root+"/sync-"+action,headers=headers,files=parts,data=data,timeout=60)
        assert r.status_code==expected,(r.status_code,r.text[:500])
        return r.json()
    preview=sync("preview",pins)
    assert preview["can_commit"] and not preview["stale_revision"] and preview["deletions_blocked"]
    assert preview["create_count"]==preview["update_count"]==1 and preview["omitted_server_feature_ids"]==["pt_2"]
    sync("preview",pins,foreign,404)
    push=pins|{"operation_id":"spec065-gpkg-push","expected_diff_sha256":preview["diff_sha256"]}
    sync("push",push|{"expected_diff_sha256":"0"*64},expected=409)
    committed=sync("push",push,expected=201)
    assert committed["operation_count"]==2 and committed["diff_sha256"]==preview["diff_sha256"]
    sync("push",push,expected=409)
    after=c.get(path+root+"/export?format=geojson",headers=auth,timeout=60)
    assert after.status_code==200 and after.headers["x-openvietgis-feature-count"]=="3"
    assert {r["properties"]["fld_name"] for r in after.json()["features"]}=={"AlphaEdited","Beta","Gamma"}
    assert sync("preview",pins)["stale_revision"]
    print("PASS GPKG file sync preview/push, missing FID protected, stale/replayed upload blocked, 3 resulting features",flush=True)



def main():
    p=argparse.ArgumentParser();p.add_argument("--candidate",type=Path,required=True);p.add_argument("--suite",choices=["integrated","collaboration","raster","topology","native-import","processing"],default="integrated");p.add_argument("--evidence",type=Path);args=p.parse_args()
    if args.evidence and args.suite!="processing":
        p.error("--evidence requires --suite processing")
    if args.evidence and args.evidence.exists():
        p.error("evidence output already exists")
    baseline.assert_safe_bench_db(OWNER,isolated_port=54332)
    with psycopg.connect(OWNER) as conn:
        assert conn.execute("SELECT to_regclass('spatial.reference_features')").fetchone()[0] is None,"refusing nonfresh DB"
    with TemporaryDirectory(prefix="ovg065-integrated-") as work:
        directory=Path(work)
        configure_python(OWNER,RUNTIME,directory)
        baseline.run_alembic_upgrade(OWNER,RUNTIME)
        f=baseline.seed_benchmark_database(OWNER,seed=65002,scales=["1k"],csrf_secret=baseline.DEFAULT_CSRF_SECRET,isolated_port=54332)
        second=scoped_member(OWNER,f)
        with psycopg.connect(OWNER) as conn:
            conn.execute("UPDATE org.organization_memberships SET role='admin' WHERE id=%s",(second[1],))
        verify_database_role(RUNTIME)
        fixtures=fixture_layers(f)
        # Only the disposable copied binary and scratch directory are accessible to nobody.
        binary=directory/"candidate";shutil.copyfile(args.candidate,binary);binary.chmod(0o755)
        wrapper=directory/"candidate-unprivileged"
        codec_env=""
        if args.suite=="native-import":
            import shapefile
            # Fixture-only module copy: system Python stays unmodified.
            codec=directory/"codec";codec.mkdir();codec.chmod(0o755)
            shutil.copyfile(shapefile.__file__,codec/"shapefile.py");(codec/"shapefile.py").chmod(0o644)
            launcher=codec/"python"
            launcher.write_text("#!/bin/sh\nexec /usr/bin/python3 -c 'import sys,runpy,os;sys.path.insert(0,"+repr(str(codec)).replace("'","\"")+");sys.argv=sys.argv[4:];sys.path.insert(0,os.path.dirname(sys.argv[0]));runpy.run_path(sys.argv[0],run_name=\"__main__\")' \"$@\"\n")
            launcher.chmod(0o755)
            codec_env="export OPENVIETGIS_VECTOR_CODEC_PYTHON="+str(launcher)+"\n"
        wrapper.write_text("#!/bin/sh\n"+codec_env+"exec /usr/bin/setpriv --reuid=65534 --regid=65534 --clear-groups "+str(binary)+"\n")
        wrapper.chmod(0o755);os.chown(directory,65534,65534);directory.chmod(0o700)
        with rust_client(wrapper,RUNTIME,directory) as c:
            try:
                if args.suite=="integrated":
                    check(c,f,second,fixtures,directory)
                elif args.suite=="processing":
                    fixture_sha256=verify_processing_fixture(fixtures,directory)
                    actual=processing_only(c,f,fixtures)
                    if args.evidence:
                        with args.evidence.open("x",encoding="utf-8") as output:
                            json.dump({"crs":"EPSG:3857","fixture_sha256":fixture_sha256,"actual":actual},output,ensure_ascii=False,indent=2)
                elif args.suite=="native-import":
                    from native_import import check_native_import
                    check_native_import(c,f)
                elif args.suite=="topology":
                    from topology import check_topology
                    check_topology(c,f)
                elif args.suite=="raster":
                    from raster_reports import check_raster_reports
                    check_raster_reports(c,f,directory)
                else:
                    from realtime_collaboration import check_realtime_collaboration
                    check_realtime_collaboration(c,f,second)
            except Exception:
                print((directory/"rust.log").read_text()[-8000:],flush=True)
                raise


if __name__=="__main__":
    main()
