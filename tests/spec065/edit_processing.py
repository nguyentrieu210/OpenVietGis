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
    assert {r["id"] for r in get("/processing/algorithms")["items"]}=={"buffer","clip","dissolve","difference","intersection","union","reproject","spatial_join"}
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
    _, difference_metrics = result(job("difference",parcels,{},clip),3,20000)
    intersection, intersection_metrics = result(job("intersection",parcels,{},clip),2,10000)
    with psycopg.connect(OWNER) as conn:
        overlay_ids = conn.execute(
            "SELECT DISTINCT attributes->>'overlay_feature_id' "
            "FROM spatial.reference_features WHERE source_id=%s",
            (intersection["output_source_id"],),
        ).fetchall()
        assert overlay_ids == [("clip",)]
    print("PASS Difference/Intersection area and feature provenance",difference_metrics,intersection_metrics,flush=True)
    union, union_metrics = result(job("union",parcels,{},clip),5,30000)
    joined, join_metrics = result(job("spatial_join",parcels,{"predicate":"contains","join_type":"left","match_mode":"all"},point),3,30000)
    result(job("spatial_join",parcels,{"predicate":"contains","join_type":"inner","match_mode":"first"},point),1,10000)
    result(job("spatial_join",point,{"predicate":"within","join_type":"inner","match_mode":"all"},parcels),1,0)
    reprojected, reproject_metrics = result(job("reproject",point,{"target_srid":4326}),1,0)
    assert reprojected["output_crs"]==4326
    with psycopg.connect(OWNER) as conn:
        union_rows=conn.execute(
            "SELECT attributes FROM spatial.reference_features WHERE source_id=%s",
            (union["output_source_id"],)).fetchall()
        assert sum(a["overlay_feature_id"] is not None for (a,) in union_rows)==2
        assert all(a["source_feature_id"] is not None for (a,) in union_rows)
        joined_rows=conn.execute(
            "SELECT attributes FROM spatial.reference_features WHERE source_id=%s",
            (joined["output_source_id"],)).fetchall()
        assert sum(a["overlay_feature_id"]=="pt" for (a,) in joined_rows)==1
        assert sum(a["overlay_feature_id"] is None for (a,) in joined_rows)==2
        for sid in (union["output_source_id"],joined["output_source_id"]):
            assert conn.execute("SELECT count(*) FROM spatial.source_fields WHERE source_id=%s",(sid,)).fetchone()[0]==10
        srid,lon,lat,attrs=conn.execute(
            "SELECT ST_SRID(geometry),ST_X(geometry),ST_Y(geometry),attributes FROM spatial.reference_features WHERE source_id=%s",
            (reprojected["output_source_id"],)).fetchone()
        ref_lon,ref_lat,ref_attrs=conn.execute(
            "SELECT ST_X(ST_Transform(geometry,4326)),ST_Y(ST_Transform(geometry,4326)),attributes FROM spatial.reference_features WHERE source_id=%s",
            (point["source_id"],)).fetchone()
        assert srid==4326 and attrs==ref_attrs
        assert math.isclose(lon,ref_lon,abs_tol=1e-10) and math.isclose(lat,ref_lat,abs_tol=1e-10)
    def pinned_layer(layer_id, source_id):
        with psycopg.connect(OWNER) as conn:
            content,schema,count=conn.execute(
                "SELECT content_revision,schema_revision,feature_count FROM spatial.data_sources WHERE id=%s",
                (source_id,)).fetchone()
        return {"layer_id":layer_id,"source_id":source_id,"content_revision":content,
                "schema_revision":schema,"feature_count":count}

    registered=post("/spatial/sources",{"reference_key":"reference_points_4756_hvs","name":"SPEC065 HVS VN2000"})
    source_id=registered["resource_id"]
    registered_layer=post("/layers",{"source_id":source_id,"name":"SPEC065 HVS VN2000 layer"})
    regional=pinned_layer(registered_layer["resource_id"],source_id)
    projected,_=result(job("reproject",regional,{"target_srid":5899}),regional["feature_count"],0)
    assert projected["output_crs"]==5899
    derived=pinned_layer(projected["output_layer_id"],projected["output_source_id"])
    geographic,_=result(job("reproject",derived,{"target_srid":4326}),regional["feature_count"],0)
    assert geographic["output_crs"]==4326
    for region in ("da_nang", "hai_phong", "dong_nai"):
        for src,dst in ((4756,4326),(4326,4756)):
            registered=post("/spatial/sources",{"reference_key":f"reference_points_{src}_{region}","name":f"SPEC065 {region} {src}"})
            sid=registered["resource_id"]
            layer=post("/layers",{"source_id":sid,"name":f"SPEC065 {region} {src} layer"})
            regional_pin=pinned_layer(layer["resource_id"],sid)
            assert regional_pin["feature_count"]==3
            transformed,_=result(job("reproject",regional_pin,{"target_srid":dst}),3,0)
            assert transformed["output_crs"]==dst
    # Whole-layer same-datum projection with attributes and feature IDs unchanged.
    extended,_=result(job("reproject",regional,{"target_srid":3405}),regional["feature_count"],0)
    assert extended["output_crs"]==3405
    extended_pin=pinned_layer(extended["output_layer_id"],extended["output_source_id"])
    wire=get("/layers/"+extended_pin["layer_id"])
    assert wire["source"]["stored_crs"]["code"]==3405
    assert next(x for x in wire["capabilities"] if x["name"]=="render")["available"]
    assert not next(x for x in wire["capabilities"] if x["name"]=="edit")["available"]
    query=("/layers/"+extended_pin["layer_id"]+"/features?content_revision="+extended_pin["content_revision"]
           +"&schema_revision="+extended_pin["schema_revision"]+"&response_crs=4756")
    page=get(query)
    assert page["context"]["response_crs"]["code"]==4756
    assert page["context"]["transformation"]["operation_id"]=="native-vn2000-projection"
    assert len(page["items"])==regional["feature_count"]
    wgs_page=get(query.replace("response_crs=4756","response_crs=4326"))
    assert wgs_page["context"]["transformation"]["operation_id"]=="verified-native-vn2000-to-wgs84"
    assert len(wgs_page["items"])==regional["feature_count"]
    recovered,_=result(job("reproject",extended_pin,{"target_srid":4756}),regional["feature_count"],0)
    with psycopg.connect(OWNER) as conn:
        diff=conn.execute("""SELECT max(ST_Distance(a.geometry,b.geometry)),
            bool_and(a.attributes=b.attributes) FROM spatial.reference_features a
            JOIN spatial.reference_features b ON b.feature_id=a.feature_id
            WHERE a.source_id=%s AND b.source_id=%s""",(regional["source_id"],recovered["output_source_id"])).fetchone()
        assert diff[0]<1e-8 and diff[1]
    post("/processing/jobs",job("reproject",regional,{"target_srid":3406}),expected=422)
    extended_wgs,_=result(job("reproject",extended_pin,{"target_srid":4326}),regional["feature_count"],0)
    assert extended_wgs["output_crs"]==4326
    recovered_pin=pinned_layer(recovered["output_layer_id"],recovered["output_source_id"])
    chained_wgs,_=result(job("reproject",recovered_pin,{"target_srid":4326}),regional["feature_count"],0)
    assert chained_wgs["output_crs"]==4326
    with psycopg.connect(OWNER) as conn:
        distance=conn.execute("""SELECT max(ST_Distance(a.geometry::geography,b.geometry::geography))
            FROM spatial.reference_features a JOIN spatial.reference_features b ON a.feature_id=b.feature_id
            WHERE a.source_id=%s AND b.source_id=%s""",(extended_wgs["output_source_id"],chained_wgs["output_source_id"])).fetchone()[0]
        assert distance<0.001
        conn.execute("UPDATE spatial.reference_features SET geometry=ST_Translate(geometry,1,0) WHERE source_id=%s",(extended_pin["source_id"],))
    # Pin hashes alone cannot hide altered coordinates: the actual replay must agree.
    post("/processing/jobs",job("reproject",extended_pin,{"target_srid":4326}),expected=422)
    with psycopg.connect(OWNER) as conn:
        conn.execute("UPDATE spatial.reference_features SET geometry=ST_Translate(geometry,-1,0) WHERE source_id=%s",(extended_pin["source_id"],))
    post("/processing/jobs",job("buffer",extended_pin,{"distance_meters":10,"quad_segs":8}),expected=422)
    print("PASS same-datum whole-layer UTM, native feature reads, round trip and out-of-zone rejection",flush=True)
    # A successful ordinary reproject does not establish regional provenance.
    unverified=pinned_layer(reprojected["output_layer_id"],reprojected["output_source_id"])
    post("/processing/jobs",job("reproject",unverified,{"target_srid":4756}),expected=422)
    result(job("reproject",derived,{"target_srid":4756}),regional["feature_count"],0)
    # Extended native edit requires explicit EPSG, then removes the source's
    # eligibility for datum replay. The native edit itself remains revision-pinned.
    with psycopg.connect(OWNER) as conn:
        fid,x,y=conn.execute("SELECT feature_id,ST_X(geometry),ST_Y(geometry) FROM spatial.reference_features WHERE source_id=%s ORDER BY feature_id LIMIT 1",(extended_pin["source_id"],)).fetchone()
    edit_path="/sources/"+extended_pin["source_id"]+"/changesets"
    edit_body={"operation_id":str(uuid4()),"base_content_revision":extended_pin["content_revision"],"base_schema_revision":extended_pin["schema_revision"],"summary":"Native CRS edit","operations":[{"op_type":"update","feature_id":fid,"geometry":{"type":"Point","coordinates":[x+1,y]}}]}
    post(edit_path,edit_body,expected=422)
    post(edit_path,edit_body|{"acknowledge_native_crs":3406},expected=422)
    post(edit_path,edit_body|{"acknowledge_native_crs":3405,"operations":[{"op_type":"update","feature_id":fid,"geometry":{"type":"Point","coordinates":[106,10]}}]},expected=422)
    receipt=post(edit_path,edit_body|{"acknowledge_native_crs":3405})
    assert receipt["status"]=="committed"
    changed=pinned_layer(extended_pin["layer_id"],extended_pin["source_id"])
    post("/processing/jobs",job("reproject",changed,{"target_srid":4326}),expected=422)
    print("PASS verified VN2000 projection, bounded multi-hop replay and tampered/unverified-source rejection",flush=True)
    post("/processing/jobs",job("reproject",point,{"target_srid":3405}),expected=422)
    post("/processing/jobs",job("spatial_join",point,{"predicate":"arbitrary_sql"},parcels),expected=422)
    post("/processing/jobs",job("union",point,{},clip),expected=422)
    print("PASS Union coverage, Spatial Join cardinality and attribute schema, approved CRS transform",flush=True)
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
    # Persistent relation rules are tested after processing to keep fixture pins stable.
    _, target, child=fixtures
    relation="/editing/relations"
    definition={"source_id":child["source_id"],"local_field":"ward",
        "target_source_id":target["source_id"],"target_field":"ward","cardinality":"one",
        "source_schema_revision":child["schema_revision"],"target_schema_revision":target["schema_revision"]}
    rule=post(relation,definition,expected=200)
    assert post(relation,definition,expected=200)["id"]==rule["id"]
    assert rule["id"] in {r["id"] for r in get(relation)["items"]}
    post(relation,definition,foreign,404)
    get(relation,foreign,404)
    child_endpoint="/sources/"+child["source_id"]+"/changesets"
    target_endpoint="/sources/"+target["source_id"]+"/changesets"
    with psycopg.connect(OWNER) as conn:
        child_attrs=conn.execute("SELECT attributes FROM spatial.reference_features WHERE source_id=%s AND feature_id='pt'",(child["source_id"],)).fetchone()[0]
    bad={"operation_id":str(uuid4()),"base_content_revision":child["content_revision"],
        "base_schema_revision":child["schema_revision"],"summary":"invalid relation",
        "operations":[{"op_type":"update","feature_id":"pt","properties":child_attrs|{"ward":"MISSING"}}]}
    post(child_endpoint,bad,expected=409)
    remove={"operation_id":str(uuid4()),"base_content_revision":target["content_revision"],
        "base_schema_revision":target["schema_revision"],"summary":"delete referenced target",
        "operations":[{"op_type":"delete","feature_id":"clip"}]}
    post(target_endpoint,remove,expected=409)
    # Even a separate write path bypassing the changeset service must be rejected.
    try:
        with psycopg.connect(OWNER) as conn:
            conn.execute("UPDATE spatial.reference_features SET attributes=jsonb_set(attributes,'{ward}',to_jsonb('MISSING'::text)) WHERE source_id=%s AND feature_id='pt'",(child["source_id"],))
            conn.execute("SET CONSTRAINTS ALL IMMEDIATE")
        raise AssertionError("database accepted invalid relation key")
    except psycopg.Error as exc:
        assert exc.sqlstate=="OV409",(exc.sqlstate,str(exc))
    try:
        with psycopg.connect(OWNER) as conn:
            conn.execute("DELETE FROM spatial.reference_features WHERE source_id=%s AND feature_id='clip'",(target["source_id"],))
            conn.execute("SET CONSTRAINTS ALL IMMEDIATE")
        raise AssertionError("database accepted orphaned relation")
    except psycopg.Error as exc:
        assert exc.sqlstate=="OV409",(exc.sqlstate,str(exc))
    with psycopg.connect(OWNER) as conn:
        assert conn.execute("SELECT attributes FROM spatial.reference_features WHERE source_id=%s AND feature_id='pt'",(child["source_id"],)).fetchone()[0]==child_attrs
        assert conn.execute("SELECT count(*) FROM spatial.reference_features WHERE source_id=%s AND feature_id='clip'",(target["source_id"],)).fetchone()[0]==1
        assert conn.execute("SELECT content_revision FROM spatial.data_sources WHERE id=%s",(child["source_id"],)).fetchone()[0]==child["content_revision"]
    # The parcels have duplicate ward A: creation must fail even if no child references another duplicate.
    parcels=fixtures[0]
    duplicate=definition|{"target_source_id":parcels["source_id"],"target_schema_revision":parcels["schema_revision"]}
    post(relation,duplicate,expected=409)
    # Definition writes outside the HTTP API are guarded too.
    for target_source_id,target_field,kind in [
        (parcels["source_id"],"ward","text"),
        (target["source_id"],"metric_val","text"),
        (target["source_id"],"code","text"),
    ]:
        try:
            with psycopg.connect(OWNER) as conn:
                conn.execute("""
                    INSERT INTO editing.relations(id,organization_id,project_id,source_id,local_field,
                        target_source_id,target_field,cardinality,field_type)
                    SELECT %s,organization_id,project_id,id,'ward',%s,%s,'one',%s
                    FROM spatial.data_sources WHERE id=%s
                """,(uuid4(),target_source_id,target_field,kind,child["source_id"]))
            raise AssertionError("database accepted invalid relation definition")
        except psycopg.Error as exc:
            assert exc.sqlstate=="OV409",(exc.sqlstate,str(exc))
    try:
        with psycopg.connect(OWNER) as conn:
            conn.execute("UPDATE editing.relations SET cardinality='many' WHERE id=%s",(rule["id"],))
        raise AssertionError("database accepted in-place relation mutation")
    except psycopg.Error as exc:
        assert exc.sqlstate=="OV409",(exc.sqlstate,str(exc))
    assert len(get(relation)["items"])==1
    assert post(relation+"/"+rule["id"]+"/delete",{},expected=200)["deleted"]
    assert rule["id"] not in {r["id"] for r in get(relation)["items"]}
    # Once explicitly removed, the same previously rejected edit may be resubmitted.
    changed=post(child_endpoint,bad)
    assert changed["status"]=="committed"
    post("/changesets/"+changed["id"]+"/revert",{"operation_id":str(uuid4())})
    print("PASS persistent relation create/replay, cross-tenant denial, duplicate targets, orphan update/delete rollback, removal",flush=True)
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
