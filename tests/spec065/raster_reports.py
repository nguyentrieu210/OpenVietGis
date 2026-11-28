"""Independent authenticated GDAL report fixture; no production data."""
import importlib.util
import json
import math

from attribute_query import ROOT, project_path, session_headers


def check_raster_reports(c, f, directory):
    path=project_path(f["org_a_id"],f["project_a_id"])+"/raster"
    auth=session_headers(f["session_a_token"], **{"x-openvietgis-csrf":f["csrf_a_token"]})
    foreign=session_headers(f["session_b_token"], **{"x-openvietgis-csrf":f["csrf_b_token"]})
    spec=importlib.util.spec_from_file_location("spec065_raster",ROOT/"fixtures/qgis-parity/generate.py")
    gen=importlib.util.module_from_spec(spec);spec.loader.exec_module(gen)
    scratch=directory/"reports";gen.generate(scratch)
    def store(name):
        r=c.post(path+"/layers",files={"file":(name+".tif",(scratch/(name+".tif")).read_bytes(),"image/tiff")},data={"method":"band","name":name,"band_a":"1"},headers=auth,timeout=70)
        assert r.status_code==201,(r.status_code,r.text[:500])
        return r.json()
    nir,dem=store("nir"),store("dem")
    def point(x,y):
        return [math.degrees(x/6378137),math.degrees(math.atan(math.sinh(y/6378137)))]
    def zone(name,x,y,X,Y):
        return {"label":name,"geometry":{"type":"Polygon","coordinates":[[point(x,y),point(X,y),point(X,Y),point(x,Y),point(x,y)]]}}
    data={"method":"zonal","source_id":nir["id"],"band":"1","zones_json":json.dumps([zone("All",-1,-1,31,31),zone("Outside",100,100,130,130)])}
    def report(data,headers=auth,expected=200):
        r=c.post(path+"/reports",files={k:(None,v) for k,v in data.items()},headers=headers,timeout=70)
        assert r.status_code==expected,(r.status_code,r.text[:600])
        return r.json()
    result=report(data)
    assert result["method"]=="zonal"
    a,b=result["zones"]
    assert a["label"]=="All" and a["valid"]==8 and a["nodata"]==1 and a["sum"]==24 and a["mean"]==3
    assert b["label"]=="Outside" and b["valid"]==b["nodata"]==0 and b["mean"] is None
    report(data,foreign,404)
    contour={"method":"contour","source_id":dem["id"],"band":"1","interval":"5"}
    result=report(contour)
    assert result["method"]=="contour" and result["geojson"]["type"]=="FeatureCollection"
    feats=result["geojson"]["features"]
    assert feats and all(r["geometry"]["type"]=="LineString" and r["properties"]["elevation"]%5==0 for r in feats)
    assert {5,10,15} <= {r["properties"]["elevation"] for r in feats}
    report(contour|{"interval":"0"},expected=422)
    report(contour,foreign,404)
    print("PASS actual GDAL zonal 8 valid/1 NoData, sum 24, outside zone null, contour intervals 5/10/15, foreign denial",flush=True)
