from uuid import uuid4
import pytest
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_layers import _path
from tests.integration.spatial.test_sources import _client

@pytest.mark.parametrize("family,geometry,blade", [
    ("LineString", {"type":"LineString","coordinates":[[106.5,10.7],[106.6,10.7]]}, [[106.55,10.65],[106.55,10.75]]),
    ("Polygon", {"type":"Polygon","coordinates":[[[106.5,10.7],[106.6,10.7],[106.6,10.8],[106.5,10.8],[106.5,10.7]]]}, [[106.55,10.65],[106.55,10.85]]),
])
def test_manual_editor(migrated_database, family, geometry, blade):
    world=seed_access_world()
    path=_path(world)
    with _client(world.owner_user_id) as c:
        payload={"operation_id":str(uuid4()),"name":"Manual test","geometry_family":family,"fields":[{"field_id":"fld_name","name":"Tên","type":"text","nullable":False}]}
        r=c.post(path+"/manual",json=payload)
        assert r.status_code==201,r.text
        receipt=r.json()
        assert c.post(path+"/manual",json=payload).json()==receipt
        assert c.post(path+"/manual",json={**payload,"name":"different"}).status_code==409
        with _client(world.wrong_user_id) as other:
            assert other.post(path+"/manual",json=payload).status_code in (403,404)
        lid,sid=receipt["layer_id"],receipt["source_id"]
        layer=c.get(path+"/"+lid).json()
        assert layer["source"]["kind"]=="manual"
        assert layer["source"]["feature_count"]==0
        assert any(x["name"]=="edit" and x["available"] for x in layer["capabilities"])
        route=path.rsplit("/",1)[0]+"/sources/"+sid+"/changesets"
        def commit(ops):
            s=c.get(path+"/"+lid).json()["source"]
            r=c.post(route,json={"operation_id":str(uuid4()),"base_content_revision":s["content_revision"],"base_schema_revision":s["schema_revision"],"summary":"Editor test","operations":ops})
            assert r.status_code in (200,201),r.text
        fid="feat_"+uuid4().hex
        commit([{"op_type":"create","feature_id":fid,"geometry":geometry,"properties":{"fld_name":"gốc"}}])
        s=c.get(path+"/"+lid).json()["source"]
        def preview(action,ids,cutter):
            r=c.post(path+"/"+lid+"/geometry-preview",json={"action":action,"feature_ids":ids,"blade":cutter,"content_revision":s["content_revision"],"schema_revision":s["schema_revision"]})
            assert r.status_code==200,r.text
            return r.json()["geometries"]
        r=c.post(path+"/"+lid+"/geometry-preview",json={"action":"split","feature_ids":[fid],"blade":blade,"content_revision":"0"*64,"schema_revision":s["schema_revision"]})
        assert r.status_code==409
        parts=preview("split",[fid],blade)
        assert len(parts)==2
        other="feat_"+uuid4().hex
        commit([{"op_type":"update","feature_id":fid,"geometry":parts[0]},{"op_type":"create","feature_id":other,"geometry":parts[1],"properties":{"fld_name":"gốc"}}])
        s=c.get(path+"/"+lid).json()["source"]
        assert s["feature_count"]==2
        records=c.get(path+"/"+lid+"/features",params={"content_revision":s["content_revision"],"schema_revision":s["schema_revision"],"include_geometry":"true","response_crs":4326})
        assert records.status_code==200,records.text
        assert len(records.json()["items"])==2
        merged=preview("merge",[fid,other],[])
        assert len(merged)==1
        commit([{"op_type":"update","feature_id":fid,"geometry":merged[0],"properties":{"fld_name":"đã sửa"}},{"op_type":"delete","feature_id":other}])
        assert c.get(path+"/"+lid).json()["source"]["feature_count"]==1
