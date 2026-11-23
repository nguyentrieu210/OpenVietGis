"""Review detail is read-only and never waives import guards."""
import json
from uuid import uuid4
from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client
from modules.house_numbering.migrate import upgrade
from modules.house_numbering.tests.integration.test_house_numbering_runtime import _seed_source
from modules.house_numbering.tests.integration.test_house_numbering_edge_acceptance import _activate_house,_create_route

def test_survey_conflict_breakdown_and_guard(migrated_database):
    upgrade()
    world=seed_access_world()
    def seed(fid,wkt,family,attrs):
        sid=_seed_source(organization_id=world.organization_id,project_id=world.project_id,
            reference_key=f"review_{uuid4().hex[:8]}",family=family,feature_id=fid,wkt=wkt,
            extent={"min_x":0,"min_y":0,"max_x":100,"max_y":150,"crs_code":3857})
        with get_admin_engine().begin() as conn:
            conn.execute(text("UPDATE spatial.reference_features SET attributes=CAST(:attrs AS jsonb) WHERE source_id=:sid"),{"sid":sid,"attrs":json.dumps(attrs)})
        return sid
    with _client(world.owner_user_id) as c:
        _,_,module,_=_activate_house(c,world)
        root=_create_route(c,module,name="Đường A",source_id=seed("root","LINESTRING(0 0,100 0)","LineString",{"ten":"Đường A"}),feature_id="root")
        route=seed("r","LINESTRING(20 0,20 50)","LineString",{"ten":"Đường A hẻm 91","prefix":"91","route_type":"alley","route_code":"legacy","parent_code":root["id"],"review_reason":"parent_required"})
        refs={}
        for fid,road,alley in [("exact","Đường A","91"),("desc","Đường A","91/14"),("branch","Đường A","15"),("road","Đường B","91"),("missing","Đường A",None),("nameless",None,"91")]:
            sid=seed(fid,"POINT(20 5)","Point",{"ten_duong":road,"hem":alley,"tuyen_id":"legacy"})
            refs[fid]=str(sid)
            r=c.post(module+"/buildings",json={"source_id":str(sid),"feature_id":fid,"eligibility":"eligible"})
            assert r.status_code==201,r.text
        key=f"{route}:r"
        body={"layers":[{"source_id":str(route),"mapping":{"name":"ten","code":"route_code","parent_code":"parent_code","prefix":"prefix","route_type":"route_type"}}],"overrides":{key:{"parent_code":root["id"],"prefix":"91","reviewed":True}}}
        p=c.post(module+"/route-import/preview",json=body);assert p.status_code==200,p.text
        plan=p.json();row=plan["items"][0]
        assert row["survey_evidence"]=={"referenced":6,"unassigned":6,"address_conflicts":3}
        assert row["survey_review"]["counts"]=={"exact":1,"descendant":1,"different_branch":1,"different_road":1,"missing_prefix":1,"missing_road":1}
        groups={g["kind"]:g for g in row["survey_review"]["groups"]}
        assert groups["descendant"]["examples"]==[{"source_id":refs["desc"],"feature_id":"desc"}]
        assert "review_survey_address_conflict" in row["errors"]
        a=c.post(module+"/route-import/apply",json={**body,"preview_token":plan["preview_token"],"selected_keys":[key]})
        assert a.status_code==422,a.text
        assert row["geometry_stage_eligible"] is True
        # Geometry review is not a waiver of the address review checkbox.
        stage_body={**body,"overrides":{key:{"parent_code":root["id"],"prefix":"91","reviewed":False}}}
        draft=c.post(module+"/route-import/preview",json=stage_body).json()
        assert draft["items"][0]["geometry_stage_eligible"] is True
        endpoint=module+"/route-import/stage-geometry"
        payload={**stage_body,"preview_token":draft["preview_token"],"selected_keys":[key],"reason":"Đã xác minh nối đúng tuyến cha, còn lỗi hồ sơ nhà"}
        assert c.post(endpoint,json={**payload,"preview_token":"0"*64}).status_code==409
        assert c.post(endpoint,json={**payload,"selected_keys":["unknown"]}).status_code==422
        staged=c.post(endpoint,json=payload);assert staged.status_code==201,staged.text
        result=staged.json()
        assert result["staged"]==1 and result["routes_created"]==0 and result["buildings_assigned"]==0
        repeat=c.post(endpoint,json=payload);assert repeat.status_code==201,repeat.text
        assert repeat.json()==result
        listing=c.get(module+"/geometry-reviews").json()
        assert len(listing["items"])==1 and listing["items"][0]["status"]=="pending"
        assert listing["items"][0]["snapshot"]["survey_evidence"]["address_conflicts"]==3
        with get_admin_engine().connect() as conn:
            assert conn.execute(text("SELECT count(*) FROM house_numbering.routes WHERE organization_id=:org"),{"org":world.organization_id}).scalar_one()==1
            assert conn.execute(text("SELECT count(*) FROM house_numbering.buildings WHERE organization_id=:org AND route_id IS NOT NULL"),{"org":world.organization_id}).scalar_one()==0
            assert conn.execute(text("SELECT count(*) FROM audit.audit_events WHERE organization_id=:org AND action='house_numbering.route.stage_geometry'"),{"org":world.organization_id}).scalar_one()==1
        # Only a fresh standard import can promote a saved review.
        with get_admin_engine().begin() as conn:
            for sid in refs.values():
                conn.execute(text("UPDATE spatial.reference_features SET attributes=CAST(:a AS jsonb) WHERE source_id=:sid"),
                    {"a":json.dumps({"ten_duong":"Đường A","hem":"91","tuyen_id":"legacy"}),"sid":sid})
        fixed=c.post(module+"/route-import/preview",json=body).json()
        assert fixed["items"][0]["state"]=="ready" and not fixed["items"][0]["geometry_stage_eligible"]
        applied=c.post(module+"/route-import/apply",json={**body,"preview_token":fixed["preview_token"],"selected_keys":[key]})
        assert applied.status_code==201,applied.text
        listing=c.get(module+"/geometry-reviews").json()
        assert listing["items"][0]["status"]=="promoted" and listing["items"][0]["promoted_route_id"]
        assert c.post(endpoint,json=payload).status_code==409
