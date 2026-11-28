"""Generate isolated, deterministic spec 065 fixtures. No external data/dependencies."""
import argparse
import hashlib
import json
import math
import struct
from pathlib import Path

def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

def rectangle(x, y, w, h):
    return {"type": "Polygon", "coordinates": [[[x,y],[x+w,y],[x+w,y+h],[x,y+h],[x,y]]]}

def feature(fid, geometry, properties):
    return {"type":"Feature","id":fid,"geometry":geometry,"properties":properties}

def collection(features, crs="EPSG:3857"):
    return {"type":"FeatureCollection","name":"spec065 synthetic","crs":{"type":"name","properties":{"name":crs}},"features":features}

def tiff(path, values):
    # Little-endian, uncompressed Float32 single-band GeoTIFF, 3x3 cells, EPSG:3857,
    # north-up, 10m pixel size, RasterPixelIsArea, explicit -9999 GDAL nodata.
    data=struct.pack("<9f",*values)
    tags=[]
    def tag(key, typ, count, value):
        tags.append((key,typ,count,value))
    def short(x): return struct.pack("<H",x)
    def long(x): return struct.pack("<I",x)
    tag(256,4,1,long(3)); tag(257,4,1,long(3))
    tag(258,3,1,short(32)); tag(259,3,1,short(1))
    tag(262,3,1,short(1)); tag(273,4,1,b"DATA")
    tag(277,3,1,short(1)); tag(278,4,1,long(3))
    tag(279,4,1,long(len(data))); tag(284,3,1,short(1))
    tag(339,3,1,short(3))
    tag(33550,12,3,struct.pack("<3d",10,10,0))
    tag(33922,12,6,struct.pack("<6d",0,0,0,0,30,0))
    keys=[1,1,0,3,1024,0,1,1,1025,0,1,1,3072,0,1,3857]
    tag(34735,3,len(keys),struct.pack("<"+"H"*len(keys),*keys))
    tag(42113,2,6,b"-9999\0")
    tags.sort()
    offset=8+2+12*len(tags)+4
    extra=b""; entries=[]
    for key,typ,count,value in tags:
        if value==b"DATA": entries.append((key,typ,count,None));continue
        if len(value)<=4: entries.append((key,typ,count,value.ljust(4,b"\0")))
        else:
            if len(extra)%2:extra+=b"\0"
            entries.append((key,typ,count,long(offset+len(extra))))
            extra+=value
    data_offset=offset+len(extra)
    body=b"II"+short(42)+long(8)+short(len(entries))
    for key,typ,count,value in entries:
        body+=struct.pack("<HHI",key,typ,count)+(long(data_offset) if value is None else value)
    path.write_bytes(body+long(0)+extra+data)

def generate(out):
    out.mkdir(parents=True,exist_ok=True)
    write_json(out/"parcels.geojson",collection([
        feature("p1",rectangle(0,0,100,100),{"code":"p1","source_feature_id":"p1","zone":"A","height":2,"active":True,"note":"Ô thử A"}),
        feature("p2",rectangle(100,0,100,100),{"code":"p2","source_feature_id":"p2","zone":"A","height":10,"active":False,"note":None}),
        feature("p3",rectangle(200,0,100,100),{"code":"p3","source_feature_id":"p3","zone":"B","height":None,"active":True,"note":"Ô thử B"}),
    ]))
    write_json(out/"clip.geojson",collection([feature("clip",rectangle(50,0,100,100),{"code":"clip","overlay_feature_id":"clip","name":"Clip"})],"EPSG:4326"))
    write_json(out/"point.geojson",collection([feature("point",{"type":"Point","coordinates":[50,50]},{"code":"pt","source_feature_id":"pt","overlay_feature_id":"pt","name":"Buffer point"})]))
    # WGS84 counterparts are generated mathematically from the same Web Mercator
    # coordinates used by the Rust/PostGIS fixture. Their bytes are covered by the
    # immutable manifest below, so both engines consume the same source fixture.
    radius=6378137.0
    def inverse_mercator(x,y):
        return math.degrees(x/radius), math.degrees(math.atan(math.sinh(y/radius)))
    def wgs_point(x,y):
        lon,lat=inverse_mercator(x,y)
        return [lon,lat]
    write_json(out/"clip-wgs84.geojson",collection([
        feature("clip",{"type":"Polygon","coordinates":[[
            wgs_point(50,0),wgs_point(150,0),wgs_point(150,100),
            wgs_point(50,100),wgs_point(50,0)]]},
            {"code":"clip","overlay_feature_id":"clip","name":"Clip"})],"EPSG:4326"))
    write_json(out/"point-wgs84.geojson",collection([
        feature("pt",{"type":"Point","coordinates":wgs_point(50,50)},
            {"code":"pt","source_feature_id":"pt","overlay_feature_id":"pt","name":"WGS point"})],"EPSG:4326"))
    write_json(out/"invalid.geojson",collection([feature("bowtie",{"type":"Polygon","coordinates":[[[0,0],[100,100],[0,100],[100,0],[0,0]]]},{})]))
    (out/"attributes.csv").write_text("fid,zone,height,active,note\np1,A,2,true,Ô thử A\np2,A,10,false,\np3,B,,true,Ô thử B\n",encoding="utf-8")
    (out/"line.dxf").write_text("0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nQA\n10\n0\n20\n0\n30\n0\n11\n100\n21\n0\n31\n0\n0\nENDSEC\n0\nEOF\n",encoding="ascii")
    tiff(out/"nir.tif",[3,3,3,3,-9999,3,3,3,3])
    tiff(out/"red.tif",[1,1,1,1,1,1,1,1,1])
    tiff(out/"dem.tif",[0,10,20,0,10,20,0,10,20])
    write_json(out/"expected.json",{
        "oracle":"analytical expectations; not captured from QGIS or an actual backend",
        "crs":"EPSG:3857","clip":{"features":2,"area_m2":10000},
        "dissolve_zone":{"features":2,"areas_m2":{"A":20000,"B":10000}},
        "buffer":{"radius_m":10,"quad_segs":8,"area_m2":16*100*math.sin(math.pi/16),"tolerance_m2":1e-6},
        "ndvi":{"valid":8,"nodata":1,"min":0.5,"max":0.5,"mean":0.5},
        "dem":{"center_slope_degrees":45,"center_aspect_degrees":270},
        "sort_height_asc":["p1","p2","p3"],
        "two_user_scenario":{"actor_a":"project editor A","actor_b":"project editor B","same_base_revision":True,"expected":"A commit succeeds; B stale changeset rejected; no production identities"}
    })
    names=["parcels.geojson","clip.geojson","point.geojson","invalid.geojson","attributes.csv","line.dxf","nir.tif","red.tif","dem.tif","expected.json","clip-wgs84.geojson","point-wgs84.geojson"]
    (out/"SHA256SUMS.txt").write_text("".join(hashlib.sha256((out/n).read_bytes()).hexdigest()+"  "+n+"\n" for n in sorted(names)),encoding="ascii")
if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--out",type=Path,required=True)
    args=parser.parse_args()
    generate(args.out)
