"""Authenticated topology and split/merge golden fixture for Spec 065."""
from uuid import UUID
import json
import math
import psycopg
from attribute_query import OWNER, baseline, project_path, session_headers


def check_topology(c, f):
    with psycopg.connect(OWNER) as conn:
        def layer(key,family,shapes):
            data=[(fid,json.dumps({"code":fid,"ward":"QA","metric_val":i,"active":True}),"SRID=4326;"+geom) for i,(fid,geom) in enumerate(shapes)]
            return baseline._seed_layer(conn.cursor(),seed=65021,org_id=UUID(f["org_a_id"]),proj_id=UUID(f["project_a_id"]),ref_key=key,name=key,kind="manual",crs=4326,family=family,count=len(data),extent={"min_x":0,"min_y":0,"max_x":.05,"max_y":.05,"crs_code":4326},validation_state="ready",validation_reason=None,rows_iter=data)
        overlap=layer("qa_overlap","Polygon",[
            ("a","POLYGON((0 0,0.02 0,0.02 0.02,0 0.02,0 0))"),
            ("b","POLYGON((0.01 0,0.03 0,0.03 0.02,0.01 0.02,0.01 0))"),
            ("c","POLYGON((0 0,0.02 0,0.02 0.02,0 0.02,0 0))")])
        boundary=layer("qa_boundary","Polygon",[("mask","POLYGON((0 0,0.04 0,0.04 0.02,0 0.02,0 0))")])
        lines=layer("qa_lines","LineString",[
            ("a","LINESTRING(0 0,0.01 0)"),("b","LINESTRING(0.01 0,0.02 0)"),
            ("closed","LINESTRING(0.03 0,0.04 0,0.04 0.01,0.03 0)")])
        parcels=layer("qa_parcels","Polygon",[
            ("a","POLYGON((0 0,0.01 0,0.01 0.01,0 0.01,0 0))"),
            ("b","POLYGON((0.01 0,0.02 0,0.02 0.01,0.01 0.01,0.01 0))"),
            ("far","POLYGON((0.03 0,0.04 0,0.04 0.01,0.03 0.01,0.03 0))")])
        missing=layer("qa_missing","Polygon",[
            ("a","POLYGON((0 0,0.01 0,0.01 0.01,0 0.01,0 0))"),
            ("b","POLYGON((0.01 0,0.02 0,0.02 0.01,0.01 0.01,0.01 0))")])
        conn.execute("UPDATE spatial.reference_features SET geometry=NULL,geometry_state='absent' WHERE source_id=%s AND feature_id='b'",(missing["source_id"],))
    path=project_path(f["org_a_id"],f["project_a_id"])
    auth=session_headers(f["session_a_token"], **{"x-openvietgis-csrf":f["csrf_a_token"]})
    foreign=session_headers(f["session_b_token"], **{"x-openvietgis-csrf":f["csrf_b_token"]})
    def pin(l,ids=()):
        return {"layer_id":l["layer_id"],"content_revision":l["content_revision"],"schema_revision":l["schema_revision"],"feature_ids":list(ids)}
    def post(endpoint,body,expected=200,headers=auth):
        r=c.post(path+endpoint,json=body,headers=headers,timeout=40)
        assert r.status_code==expected,(endpoint,r.status_code,r.text[:600])
        return r.json()
    def topo(rule,l,secondary=None,ids=(),expected=200,headers=auth):
        return post("/pro/topology",{"check":rule,"primary":pin(l,ids),**({"secondary":secondary} if secondary else {})},expected,headers)
    d=topo("duplicate",overlap)
    assert d["complete"] and d["scanned_count"]==3 and d["skipped_count"]==0
    assert [(i["feature_id"],i["other"]) for i in d["items"]]==[("c","a")]
    d=topo("overlap",overlap)
    assert len(d["items"])==3
    d=topo("overlap",overlap,ids=["b"])
    assert {i["other"] for i in d["items"]}=={"a","c"},d
    d=topo("gap",overlap,pin(boundary))
    assert len(d["items"])==1 and d["secondary_pin"]["source_id"]==boundary["source_id"]
    with psycopg.connect(OWNER) as conn:
        area=conn.execute("SELECT ST_Area(ST_GeomFromGeoJSON(%s))",(json.dumps(d["items"][0]["geometry"]),)).fetchone()[0]
    assert math.isclose(area,.0002,abs_tol=1e-10)
    assert topo("outside",overlap,pin(boundary))["items"]==[]
    d=topo("dangling",lines)
    assert {i["feature_id"] for i in d["items"]}=={"a","b"} and len(d["items"])==2
    d=topo("invalid",missing)
    assert d["items"]==[] and not d["complete"] and d["skipped_count"]==1 and d["scanned_count"]==1
    topo("gap",missing,pin(boundary),expected=422)
    topo("gap",overlap,pin(boundary,["unknown"]),expected=409)
    topo("gap",overlap,pin(overlap),expected=422)
    topo("duplicate",overlap,expected=404,headers=foreign)
    print("PASS actual topology duplicate/overlap selected FID order, gap area, outside, closed line, skipped geometry, reference pins and foreign denial",flush=True)
    endpoint="/layers/"+parcels["layer_id"]+"/geometry-preview"
    def preview(action,ids,blade=(),expected=200):
        return post(endpoint,{"action":action,"content_revision":parcels["content_revision"],"schema_revision":parcels["schema_revision"],"feature_ids":ids,"blade":list(blade)},expected)
    split=preview("split",["a"],[[.005,-.005],[.005,.015]])
    assert len(split["geometries"])==2
    merge=preview("merge",["a","b"])
    assert len(merge["geometries"])==1
    preview("merge",["a","far"],expected=422)
    preview("split",["a"],[[.025,0],[.025,.02]],expected=422)
    with psycopg.connect(OWNER) as conn:
        area=lambda g:conn.execute("SELECT ST_Area(ST_GeomFromGeoJSON(%s))",(json.dumps(g),)).fetchone()[0]
        assert math.isclose(sum(map(area,split["geometries"])),.0001,abs_tol=1e-10)
        assert math.isclose(area(merge["geometries"][0]),.0002,abs_tol=1e-10)
    print("PASS actual split/merge preview golden area, disconnected merge and noncrossing split rejection",flush=True)
    sid=parcels["source_id"]
    with psycopg.connect(OWNER) as conn:
        attrs=conn.execute("SELECT attributes FROM spatial.reference_features WHERE source_id=%s AND feature_id='a'",(sid,)).fetchone()[0]
    def save(op,revision,operations):
        return post("/sources/"+sid+"/changesets",{"operation_id":op,"base_content_revision":revision,"base_schema_revision":parcels["schema_revision"],"summary":"Spec065 topology acceptance","operations":operations},201)
    cut_ops=[{"op_type":"update","feature_id":"a","geometry":split["geometries"][0]},
             {"op_type":"create","feature_id":"a_split","geometry":split["geometries"][1],"properties":attrs}]
    cut=save("spec065-cut",parcels["content_revision"],cut_ops)
    assert cut["status"]=="committed" and cut["operation_count"]==2
    assert save("spec065-cut",parcels["content_revision"],cut_ops)["id"]==cut["id"]
    new=post(endpoint,{"action":"merge","content_revision":cut["resulting_content_revision"],"schema_revision":parcels["schema_revision"],"feature_ids":["a","a_split","b"]})
    assert len(new["geometries"])==1
    union_ops=[{"op_type":"update","feature_id":"a","geometry":new["geometries"][0]},
               {"op_type":"delete","feature_id":"a_split"},{"op_type":"delete","feature_id":"b"}]
    union=save("spec065-merge",cut["resulting_content_revision"],union_ops)
    assert union["status"]=="committed" and union["operation_count"]==3
    with psycopg.connect(OWNER) as conn:
        rows=conn.execute("SELECT feature_id,attributes,ST_Area(geometry) FROM spatial.reference_features WHERE source_id=%s ORDER BY feature_id",(sid,)).fetchall()
        assert [r[0] for r in rows]==["a","far"] and rows[0][1]==attrs
        assert math.isclose(rows[0][2],.0002,abs_tol=1e-10)
    print("PASS actual split/merge committed changesets, keeper FID/attributes preserved and replay",flush=True)
