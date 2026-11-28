"""Authenticated Rust vector codec -> atomic publish fixture."""
import hashlib
import io
import math
import zipfile
from uuid import uuid4

from attribute_query import project_path, session_headers
from e07_transfers import fixtures
from openvietgis_api.adapters.formats.geopackage import write_geopackage
from openvietgis_api.domains.spatial.domain import FieldType, NativeGeometry
from openvietgis_api.domains.transfers.domain import ExportFieldMapping


def check_native_import(c,f):
    path=project_path(f["org_a_id"],f["project_a_id"])
    foreign_path=project_path(f["org_b_id"],f["project_b_id"])
    auth=session_headers(f["session_a_token"], **{"x-openvietgis-csrf":f["csrf_a_token"]})
    foreign=session_headers(f["session_b_token"], **{"x-openvietgis-csrf":f["csrf_b_token"]})
    def post(endpoint,expected=200,headers=auth,base=path,**kw):
        r=c.post(base+endpoint,headers=headers,timeout=65,**kw)
        assert r.status_code==expected,(endpoint,r.status_code,r.text[:1000])
        return r.json()
    def upload(name,raw,expected=201,headers=auth):
        return post("/imports/upload",expected,headers= headers,files={"file":(name,raw,"application/octet-stream")})
    def publish(fmt,name,raw,target=4326):
        a=upload(name,raw); asset,i=a["asset"],a["inspection"]
        assert asset["sha256"]==hashlib.sha256(raw).hexdigest()
        assert i["detected_format"]==fmt and i["confirmable"] and i["feature_count"]==2,i
        opts={"source_asset_id":asset["id"],"selected_member":i["selected_member"],"declared_source_crs":i["resolved_source_crs"],"declared_encoding":i["resolved_encoding"],"target_crs":target}
        post("/imports/inspect",404,headers=foreign,base=foreign_path,json=opts)
        bad=post("/imports/inspect",json=opts|{"declared_source_crs":4756})["inspection"]
        assert not bad["confirmable"]
        i=post("/imports/inspect",json=opts)["inspection"]
        assert i["confirmable"],i
        body=opts|{"operation_id":str(uuid4()),"source_name":name,"layer_name":name,"inspection_fingerprint":i["inspection_fingerprint"],"approved_conversions":[c["code"] for c in i["conversions"] if c["requires_approval"]]}
        post("/imports/confirm",409,json=body|{"inspection_fingerprint":"0"*64})
        job=post("/imports/confirm",json=body)
        assert job["status"]=="succeeded",job
        assert post("/imports/confirm",json=body)["id"]==job["id"]
        post("/imports/confirm",409,json=body|{"layer_name":"different"})
        report=job["result_report"]
        assert report["format"]==fmt and report["source_crs"]==opts["declared_source_crs"] and report["stored_crs"]==target
        lid=job["target_layer_id"]
        ex=c.get(path+"/integration-hub/adapters/qgis/layers/"+lid+"/export?format=geojson",headers=auth,timeout=60)
        assert ex.status_code==200,(ex.status_code,ex.text[:400])
        rows=ex.json()["features"]
        assert len(rows)==2
        if fmt!="geojson":
            import subprocess
            import tempfile
            from pathlib import Path
            export={"layer_id":lid,"content_revision":report["content_revision"],"schema_revision":report["schema_revision"],"output_format":fmt,"output_crs":target}
            pre=post("/exports/preview",json=export)
            assert pre["feature_count"]==2 and pre["confirmable"]
            required=[d["code"] for d in pre["disclosures"] if d["requires_approval"]]
            body=export|{"operation_id":str(uuid4()),"preview_fingerprint":pre["preview_fingerprint"],"approved_disclosures":required}
            post("/exports/confirm",409,json=body|{"preview_fingerprint":"0"*64})
            if required:post("/exports/confirm",422,json=body|{"approved_disclosures":[]})
            out=post("/exports/confirm",json=body)
            assert out["status"]=="succeeded",out
            assert post("/exports/confirm",json=body)["id"]==out["id"]
            a=out["artifact"]
            assert a["comparison_report"]["exported_feature_count"]==2
            dl=c.get(path+"/artifacts/"+a["id"]+"/download",headers=auth,timeout=60)
            assert dl.status_code==200 and dl.headers["x-content-sha256"]==a["sha256"]==hashlib.sha256(dl.content).hexdigest()
            assert c.get(path+"/artifacts/"+a["id"]+"/download",headers=foreign).status_code==404
            roundtrip=upload(a["filename"],dl.content)["inspection"]
            assert roundtrip["confirmable"] and roundtrip["feature_count"]==2 and roundtrip["resolved_source_crs"]==target
            with tempfile.TemporaryDirectory() as d:
                file=Path(d)/a["filename"];file.write_bytes(dl.content)
                reader_path=str(file)
                if fmt=="shapefile_zip":
                    with zipfile.ZipFile(file) as archive:
                        names=[n for n in archive.namelist() if n.lower().endswith(".shp")]
                        assert len(names)==1
                    reader_path="/vsizip/"+str(file)+"/"+names[0]
                external=subprocess.run(["ogrinfo","-al","-so",reader_path],capture_output=True,text=True,timeout=20,check=False)
                assert external.returncode==0 and "Feature Count: 2" in external.stdout,(external.stdout,external.stderr)
            print("PASS durable "+fmt+" export, explicit approvals, digest, tenant, replay and independent GDAL reader/re-import",flush=True)
        return rows,report
    for fmt,name,raw in fixtures():
        rows,_report=publish(fmt,name,raw)
        p1=next(r for r in rows if r["properties"]["rank"]==0)
        p2=next(r for r in rows if r["properties"]["rank"] is None)
        assert p1["properties"]["code"]=="Điểm A" and p1["properties"]["active"] is False
        assert p2["properties"]["active"] is None
        if fmt!="shapefile_zip":
            assert p2["properties"]["code"]==""
        assert p1["geometry"]["coordinates"]==[106.6,10.7]
        print("PASS "+fmt+" import, digest, CRS conflict, stale/replay, Unicode/false/zero/null, tenant boundary, GeoJSON export",flush=True)
        if fmt=="shapefile_zip":
            buf=io.BytesIO()
            with zipfile.ZipFile(io.BytesIO(raw)) as z,zipfile.ZipFile(buf,"w") as out:
                for n in z.namelist():
                    if not n.lower().endswith(".prj"):out.writestr(n,z.read(n))
            unknown=upload("unknown.zip",buf.getvalue())["inspection"]
            assert not unknown["confirmable"] and unknown["requires_crs_resolution"]
            buf=io.BytesIO()
            with zipfile.ZipFile(io.BytesIO(raw)) as z,zipfile.ZipFile(buf,"w") as out:
                for n in z.namelist():
                    out.writestr("first/"+n,z.read(n))
                    out.writestr("second/"+n,z.read(n))
            multi=upload("multiple.zip",buf.getvalue())
            i=multi["inspection"]
            assert i["requires_member_selection"] and not i["confirmable"] and len(i["members"])==2
            selected=post("/imports/inspect",json={"source_asset_id":multi["asset"]["id"],"selected_member":i["members"][1]["name"]})["inspection"]
            assert selected["confirmable"] and selected["feature_count"]==2
            buf=io.BytesIO()
            with zipfile.ZipFile(io.BytesIO(raw)) as z,zipfile.ZipFile(buf,"w") as out:
                for n in z.namelist():
                    if not n.lower().endswith(".cpg"):out.writestr(n,z.read(n))
            enc=upload("encoding.zip",buf.getvalue())
            assert enc["inspection"]["requires_encoding_resolution"] and not enc["inspection"]["confirmable"]
            confirmed=post("/imports/inspect",json={"source_asset_id":enc["asset"]["id"],"selected_member":enc["inspection"]["selected_member"],"declared_encoding":"utf-8"})["inspection"]
            assert confirmed["confirmable"]
            print("PASS multi-member selection and explicit DBF encoding",flush=True)
            for entries in [[("../escape.shp",b"x")],[("A.shp",b"x"),("a.SHP",b"y")]]:
                buf=io.BytesIO()
                with zipfile.ZipFile(buf,"w") as z:
                    for n,v in entries:z.writestr(n,v)
                upload("unsafe.zip",buf.getvalue(),422)
    # Native metric CRS -> WGS84 through approved Rust/PostGIS transform.
    coords=[(106.6,10.7),(106.7,10.8)]
    def xy(p):return (6378137*math.radians(p[0]),6378137*math.log(math.tan(math.pi/4+math.radians(p[1])/2)))
    raw=write_geopackage(layer_name="metric",crs_code=3857,geometry_family="Point",field_mappings=[ExportFieldMapping("code","code","code",FieldType.TEXT)],features=[(f"p{i}",{"code":str(i)},NativeGeometry("Point",xy(p),3857)) for i,p in enumerate(coords)])[0]
    rows,r=publish("geopackage","metric.gpkg",raw)
    assert r["source_crs"]==3857 and r["stored_crs"]==4326 and r["transformation_operation_id"]
    for feature in rows:
        p=coords[int(feature["properties"]["code"])]
        assert all(math.isclose(a,b,abs_tol=1e-8) for a,b in zip(feature["geometry"]["coordinates"],p))
    print("PASS EPSG:3857 GeoPackage -> EPSG:4326 via approved transformation, native CRS provenance retained; malformed archive and unknown CRS rejected",flush=True)

    # Verify actual precision/name disclosure, and native polygon with interior hole.
    rings=(((106.6,10.7),(106.62,10.7),(106.62,10.72),(106.6,10.72),(106.6,10.7)),((106.605,10.705),(106.605,10.71),(106.61,10.71),(106.61,10.705),(106.605,10.705)))
    raw=write_geopackage(layer_name="QA polygon",crs_code=4326,geometry_family="Polygon",field_mappings=[ExportFieldMapping("very_long_field","very_long_field","very_long_field",FieldType.TEXT),ExportFieldMapping("amount","amount","amount",FieldType.DECIMAL)],features=[("p",{"very_long_field":"Đất trồng","amount":"12.123456789"},NativeGeometry("Polygon",rings,4326))])[0]
    upload_result=upload("polygon.gpkg",raw);i=upload_result["inspection"];assert i["confirmable"],i
    opts={"source_asset_id":upload_result["asset"]["id"],"selected_member":i["selected_member"],"declared_source_crs":4326,"declared_encoding":"utf-8","target_crs":4326,"operation_id":str(uuid4()),"source_name":"Polygon QA","layer_name":"Polygon QA","inspection_fingerprint":i["inspection_fingerprint"],"approved_conversions":[]}
    j=post("/imports/confirm",json=opts);assert j["status"]=="succeeded",j
    r=j["result_report"]
    for fmt,codes in [("geopackage",{"GPKG_DECIMAL_REAL"}),("shapefile_zip",{"DBF_FIELD_NAME_TRUNCATED","DBF_DECIMAL_FIXED_PRECISION","DBF_TEXT_NULL_EMPTY"})]:
        params={"layer_id":j["target_layer_id"],"content_revision":r["content_revision"],"schema_revision":r["schema_revision"],"output_format":fmt,"output_crs":4326}
        preview=post("/exports/preview",json=params);required={d["code"] for d in preview["disclosures"] if d["requires_approval"]};assert codes<=required,required
        body=params|{"operation_id":str(uuid4()),"preview_fingerprint":preview["preview_fingerprint"],"approved_disclosures":sorted(required)}
        post("/exports/confirm",422,json=body|{"approved_disclosures":[]})
        out=post("/exports/confirm",json=body);assert out["status"]=="succeeded",out
        a=out["artifact"];dl=c.get(path+"/artifacts/"+a["id"]+"/download",headers=auth,timeout=60);assert dl.status_code==200
        rt=upload(a["filename"],dl.content)["inspection"];assert rt["confirmable"] and rt["feature_count"]==1 and rt["geometry_family"]=="Polygon",rt
    print("PASS polygon with hole, GPKG decimal and DBF name/precision disclosures require approval",flush=True)
