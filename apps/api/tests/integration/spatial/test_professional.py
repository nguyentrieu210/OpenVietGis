from uuid import uuid4
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_layers import _path
from tests.integration.spatial.test_sources import _client
from openvietgis_api.domains.cartography.domain import PaperSize, Orientation, paper_dimensions_px

def test_pro_tools(migrated_database):
    world=seed_access_world()
    path=_path(world)
    root=path.rsplit("/",1)[0]
    pro=root+"/pro"
    with _client(world.owner_user_id) as c:
        def check(r,status=200):
            assert r.status_code==status,r.text
            return r.json()
        def make(family, geoms):
            rec=check(c.post(path+"/manual",json={"operation_id":str(uuid4()),"name":family,"geometry_family":family,"fields":[{"field_id":"nm","name":"Tên","type":"text","nullable":True}]}),201)
            lid,sid=rec["layer_id"],rec["source_id"]
            s=check(c.get(path+"/"+lid))["source"]
            ids=["feat_"+uuid4().hex for _ in geoms]
            r=c.post(root+"/sources/"+sid+"/changesets",json={"operation_id":str(uuid4()),"base_content_revision":s["content_revision"],"base_schema_revision":s["schema_revision"],"summary":"test","operations":[{"op_type":"create","feature_id":fid,"geometry":{"type":family,"coordinates":g},"properties":{"nm":"A"}} for fid,g in zip(ids,geoms)]})
            assert r.status_code in (200,201),r.text
            s=check(c.get(path+"/"+lid))["source"]
            return {"layer_id":lid,"content_revision":s["content_revision"],"schema_revision":s["schema_revision"],"feature_ids":ids}
        a=make("LineString",[[[106.5,10.7],[106.51,10.7],[106.51,10.71]]])
        b=make("LineString",[[[106.505,10.69],[106.505,10.71]]])
        assert check(c.post(pro+"/geometry",json={"operation":"trim","primary":a,"secondary":b}))["items"]
        for op,extra in [("copy",{"dx":10,"dy":4}),("rotate",{"angle":30}),("scale",{"factor":1.2}),("stretch",{"dx":5,"vertex":1}),("extend",{"distance":10}),("offset",{"distance":5}),("fillet",{"distance":5,"vertex":1}),("simplify",{"distance":10}),("centroid",{}),("hull",{})]:
            assert check(c.post(pro+"/geometry",json={"operation":op,"primary":a,**extra}))["items"],op
        poly=make("Polygon",[[[[106.49,10.69],[106.52,10.69],[106.52,10.72],[106.49,10.72],[106.49,10.69]]]])
        q=make("Point",[[106.505,10.705],[106.505,10.705],[107.0,10.7]])
        assert len(check(c.post(pro+"/topology",json={"check":"duplicate","primary":{**q,"feature_ids":[]}}))["items"])==1
        moved=check(c.post(pro+"/geometry",json={"operation":"move_batch","primary":q,"dx":10,"dy":20}))
        assert {x["feature_id"] for x in moved["items"]}==set(q["feature_ids"])
        assert len(moved["items"])==3 and all(x["geometry"]["type"]=="Point" for x in moved["items"])
        assert all(x["geometry"]["coordinates"] not in [[106.505,10.705],[107.0,10.7]] for x in moved["items"])
        assert c.post(pro+"/geometry",json={"operation":"move_batch","primary":q,"dx":0,"dy":0}).status_code==422
        assert c.post(pro+"/geometry",json={"operation":"move_batch","primary":{**q,"content_revision":"0"*64},"dx":10}).status_code==409
        # Preview must not write into the original layer.
        assert len(check(c.post(pro+"/topology",json={"check":"duplicate","primary":{**q,"feature_ids":[]}}))["items"])==1
        assert len(check(c.post(pro+"/topology",json={"check":"outside","primary":{**q,"feature_ids":[]},"secondary":poly}))["items"])==1
        assert check(c.post(pro+"/topology",json={"check":"dangling","primary":a}))["items"]
        assert check(c.post(pro+"/geometry",json={"operation":"nearest","primary":q,"secondary":poly}))["items"]
        assert check(c.post(pro+"/geometry",json={"operation":"spatial_join","primary":q,"secondary":poly}))["items"]
        assert check(c.post(pro+"/geometry",json={"operation":"intersection","primary":a,"secondary":poly}))["items"]
        poly2=make("Polygon",[[[[106.50,10.70],[106.51,10.70],[106.51,10.71],[106.50,10.71],[106.50,10.70]]]])
        assert check(c.post(pro+"/geometry",json={"operation":"difference","primary":poly,"secondary":poly2}))["items"]
        assert check(c.post(pro+"/geometry",json={"operation":"union","primary":poly,"secondary":poly2}))["items"]
        assert check(c.post(pro+"/topology",json={"check":"gap","primary":poly2,"secondary":poly}))["items"]
        overlapping=make("Polygon",[[[[106.49,10.69],[106.52,10.69],[106.52,10.72],[106.49,10.72],[106.49,10.69]]],[[[106.50,10.70],[106.51,10.70],[106.51,10.71],[106.50,10.71],[106.50,10.70]]]])
        assert len(check(c.post(pro+"/topology",json={"check":"overlap","primary":overlapping}))["items"])==1
        r=check(c.post(pro+"/schema",json={"primary":q,"action":"add","field_id":"new_field","name":"Mới","type":"integer"}))
        q.update(content_revision=r["content_revision"],schema_revision=r["schema_revision"])
        feature=check(c.get(path+"/"+q["layer_id"]+"/features",params={"content_revision":q["content_revision"],"schema_revision":q["schema_revision"],"include_geometry":"true","response_crs":4326}))["items"][0]
        assert "new_field" in feature["attributes"] and feature["attributes"]["new_field"] is None
        r=check(c.post(pro+"/schema",json={"primary":q,"action":"rename","field_id":"new_field","name":"Đổi tên"}))
        q.update(content_revision=r["content_revision"],schema_revision=r["schema_revision"])
        result=c.get(pro+"/tiles/"+q["layer_id"]+"/0/0/0.mvt",params={"content_revision":q["content_revision"],"schema_revision":q["schema_revision"]})
        assert result.status_code==200,result.text
        assert result.content and b"features" in result.content
        stale=c.get(pro+"/tiles/"+q["layer_id"]+"/0/0/0.mvt",params={"content_revision":"0"*64,"schema_revision":q["schema_revision"]})
        assert stale.status_code==409
        pdf=check(c.post(root+"/cartography/layouts",json={"name":"Test A0","title":"A0","paper_size":"A0","orientation":"landscape","layer_ids":[q["layer_id"]]}),201)
        assert pdf["paper_size"]=="A0"
        p=check(c.post(root+"/cartography/layouts/"+pdf["id"]+"/exports/preview",json={"output_format":"pdf","dpi":150}))
        assert p["revision_fingerprint"]
        exported=check(c.post(root+"/cartography/layouts/"+pdf["id"]+"/exports",json={"output_format":"pdf","dpi":150,"expected_revision_fingerprint":p["revision_fingerprint"]}),201)
        download=c.get(root+"/cartography/exports/"+exported["id"]+"/download")
        assert download.status_code==200 and download.content.startswith(b"%PDF")
        with _client(world.wrong_user_id) as other:
            assert other.post(pro+"/topology",json={"check":"duplicate","primary":q}).status_code in (403,404)
    w,h=paper_dimensions_px(PaperSize.A0,Orientation.LANDSCAPE,150)
    assert w>7000 and h>4900
