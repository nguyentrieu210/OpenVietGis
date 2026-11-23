"""Scoped, stale-safe cluster correction with immutable survey evidence."""
import json
from uuid import uuid4
from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client
from modules.house_numbering.migrate import upgrade
from modules.house_numbering.cluster_correction import cluster_key
from modules.house_numbering.tests.integration.test_house_numbering_runtime import _seed_source
from modules.house_numbering.tests.integration.test_house_numbering_edge_acceptance import _activate_house,_create_route

def test_cluster_correction_protects_distance_source_and_stale_preview(migrated_database):
    upgrade()
    world=seed_access_world()
    def seed(fid,wkt,family,attrs):
        sid=_seed_source(organization_id=world.organization_id,project_id=world.project_id,
            reference_key=f"correction_{uuid4().hex[:8]}",family=family,feature_id=fid,wkt=wkt,
            extent={"min_x":0,"min_y":0,"max_x":100,"max_y":150,"crs_code":3857})
        with get_admin_engine().begin() as conn:
            conn.execute(text("UPDATE spatial.reference_features SET attributes=CAST(:attrs AS jsonb) WHERE source_id=:sid"),{"sid":sid,"attrs":json.dumps(attrs)})
        return sid
    with _client(world.owner_user_id) as client:
        _,_,module,_=_activate_house(client,world)
        old=_create_route(client,module,name="Đường Sai",source_id=seed("old","LINESTRING(0 100,100 100)","LineString",{}),feature_id="old")
        new=_create_route(client,module,name="Đường Đúng",source_id=seed("new","LINESTRING(0 0,100 0)","LineString",{}),feature_id="new")
        attrs={"ten_duong":"Đường Đúng","hem":"","tuyen_id":old["id"]}
        ids=[]; sources=[]
        for fid,y,eligibility in [("near1",3,"eligible"),("near2",8,"eligible"),("far",100,"eligible"),("ineligible",4,"ineligible")]:
            sid=seed(fid,f"POINT(20 {y})","Point",attrs);sources.append(sid)
            r=client.post(module+"/buildings",json={"source_id":str(sid),"feature_id":fid,"eligibility":eligibility,**({"ineligibility_reason":"fixture"} if eligibility=="ineligible" else {})})
            assert r.status_code==201,r.text
            ids.append(r.json()["id"])
        group=cluster_key("Đường Đúng","",old["id"])
        audit=client.post(module+"/building-assignment/audit",json={"group":"address_conflict","cluster":group,"limit":100})
        assert audit.status_code==200,audit.text
        assert audit.json()["active_cluster"]["count"]==4
        body={"cluster":group,"building_ids":ids,"max_distance_m":35}
        endpoint=module+"/building-assignment/correction"
        preview=client.post(endpoint+"/preview",json=body)
        assert preview.status_code==200,preview.text
        plan=preview.json()
        assert plan["recommended_route_id"]==new["id"]
        assert plan["candidates"][0]["within_limit"]==2
        assert plan["original_routes"][0]["route_id"]==old["id"]
        assert client.post(endpoint+"/preview",json={**body,"cluster":"0"*24}).status_code==409
        assert client.post(endpoint+"/preview",json={**body,"max_distance_m":80}).status_code==422
        payload={**body,"route_id":new["id"],"selected_ids":ids[:2],"reason":"Đã kiểm tra đúng đường thực tế","preview_token":plan["preview_token"]}
        assert client.post(endpoint+"/apply",json={**payload,"route_id":old["id"]}).status_code==422
        assert client.post(endpoint+"/apply",json={**payload,"selected_ids":[ids[2]]}).status_code==422
        assert client.post(endpoint+"/apply",json={**payload,"selected_ids":[ids[3]]}).status_code==422
        assert client.post(endpoint+"/apply",json={**payload,"preview_token":"0"*64}).status_code==409
        result=client.post(endpoint+"/apply",json=payload)
        assert result.status_code==200,result.text
        assert result.json()["assigned"]==2
        assert client.post(endpoint+"/apply",json=payload).status_code==409
        remaining=client.post(module+"/building-assignment/audit",json={"group":"address_conflict","cluster":group}).json()
        assert remaining["active_cluster"]["count"]==2
        with get_admin_engine().connect() as conn:
            for sid in sources:
                assert conn.execute(text("SELECT attributes->>'tuyen_id' FROM spatial.reference_features WHERE source_id=:sid"),{"sid":sid}).scalar_one()==old["id"]
            count=conn.execute(text("SELECT count(*) FROM audit.audit_events WHERE organization_id=:org AND action='house_numbering.building.correct_route_code'"),{"org":world.organization_id}).scalar_one()
            assert count==2
