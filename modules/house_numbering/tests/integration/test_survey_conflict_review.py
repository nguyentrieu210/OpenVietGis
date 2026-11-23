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
        for fid,road,alley in [("exact","Đường A","91"),("desc","Đường A","91/14"),("branch","Đường A","15"),("road","Đường B","91"),("missing","Đường A",None),("missing2","Đường A",None),("nameless",None,"91")]:
            sid=seed(fid,"POINT(20 5)","Point",{"ten_duong":road,"hem":alley,"tuyen_id":"legacy"})
            refs[fid]=str(sid)
            r=c.post(module+"/buildings",json={"source_id":str(sid),"feature_id":fid,"eligibility":"eligible"})
            assert r.status_code==201,r.text
        key=f"{route}:r"
        body={"layers":[{"source_id":str(route),"mapping":{"name":"ten","code":"route_code","parent_code":"parent_code","prefix":"prefix","route_type":"route_type"}}],"overrides":{key:{"parent_code":root["id"],"prefix":"91","reviewed":True}}}
        p=c.post(module+"/route-import/preview",json=body);assert p.status_code==200,p.text
        plan=p.json();row=plan["items"][0]
        assert row["survey_evidence"]=={"referenced":7,"unassigned":7,"address_conflicts":3}
        assert row["survey_review"]["counts"]=={"exact":1,"descendant":1,"different_branch":1,"different_road":1,"missing_prefix":2,"missing_road":1}
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
        refreshed=c.post(module+"/route-import/preview",json=stage_body).json()
        repeat=c.post(endpoint,json={**payload,"preview_token":refreshed["preview_token"]});assert repeat.status_code==201,repeat.text
        assert repeat.json()==result
        listing=c.get(module+"/geometry-reviews").json()
        assert len(listing["items"])==1 and listing["items"][0]["status"]=="pending"
        assert listing["items"][0]["snapshot"]["survey_evidence"]["address_conflicts"]==3
        with get_admin_engine().connect() as conn:
            assert conn.execute(text("SELECT count(*) FROM house_numbering.routes WHERE organization_id=:org"),{"org":world.organization_id}).scalar_one()==1
            assert conn.execute(text("SELECT count(*) FROM house_numbering.buildings WHERE organization_id=:org AND route_id IS NOT NULL"),{"org":world.organization_id}).scalar_one()==0
            assert conn.execute(text("SELECT count(*) FROM audit.audit_events WHERE organization_id=:org AND action='house_numbering.route.stage_geometry'"),{"org":world.organization_id}).scalar_one()==1
        # Changing the mapped route code cannot bypass the staged source.
        alternate={**body,"layers":[{"source_id":str(route),"mapping":{**body["layers"][0]["mapping"],"code":"ten"}}]}
        alternate_plan=c.post(module+"/route-import/preview",json=alternate).json()
        assert "review_stage_overlap" in alternate_plan["items"][0]["errors"]
        # A different already-catalogued branch is ranked by its full address and distance.
        alternate_route=_create_route(c,module,name="Đường A hẻm 15",
            source_id=seed("branch15","LINESTRING(25 0,25 50)","LineString",{"ten":"Đường A hẻm 15"}),
            feature_id="branch15",parent_route_id=root["id"],prefix="15")
        # Verify exceptions individually without rewriting source material.
        addresses=module+"/geometry-reviews/"+listing["items"][0]["id"]+"/addresses"
        report=c.get(addresses);assert report.status_code==200,report.text
        report=report.json();assert report["geometry_current"]
        assert report["counts"]=={"total":6,"verified":0,"pending":6,"blocked":0}
        assert sum(report["suggestion_counts"].values())==6
        by_feature={item["feature_id"]:item for item in report["items"]}
        branch=by_feature["branch"]["suggestions"]
        assert branch["priority"]=="strong" and branch["candidates"][0]["route_id"]==alternate_route["id"]
        assert branch["candidates"][0]["decision"]=="other_address"
        assert by_feature["missing"]["suggestions"]["priority"]=="partial"
        assert by_feature["missing"]["suggestions"]["candidates"][0]["origin"]=="staged"
        assert all(item["suggestions"]["candidates"] for item in report["items"])
        assert not any(item["status"]=="verified" for item in report["items"])
        groups=report["batch_groups"]
        assert len(groups)==1 and groups[0]["count"]==2 and groups[0]["kind"]=="missing_prefix"
        group=groups[0];assert group["verified_prefix"]=="91" and group["verified_road"]=="Đường A"
        def member(item):
            return {"building_id":item["building_id"],"evidence_token":item["evidence_token"],"expected_revision":item["revision"]}
        members=[member(item) for item in group["members"]]
        batch={"key":group["key"],"members":members,"evidence":"Đối chiếu biên bản thực địa cho hai nhà mẫu","confirmed":True}
        endpoint_batch=addresses+"/batch"
        assert c.post(endpoint_batch,json={**batch,"confirmed":False}).status_code==422
        assert c.post(endpoint_batch,json={**batch,"members":[members[0],members[0]]}).status_code==422
        assert c.post(endpoint_batch,json={**batch,"key":"0"*64}).status_code==422
        assert c.post(endpoint_batch,json={**batch,"members":[members[0],member(by_feature["desc"])]}).status_code==422
        assert c.post(endpoint_batch,json={**batch,"members":[members[0],{**members[1],"evidence_token":"0"*64}]}).status_code==409
        assert c.get(addresses).json()["counts"]["verified"]==0
        saved_batch=c.post(endpoint_batch,json=batch);assert saved_batch.status_code==200,saved_batch.text
        assert saved_batch.json()["verified"]==2 and saved_batch.json()["buildings_updated"]==0
        assert c.post(endpoint_batch,json=batch).status_code==409
        after=c.get(addresses).json()
        assert after["counts"]["verified"]==2 and after["batch_groups"]==[]
        assert "review_address_pending" in c.post(module+"/route-import/preview",json=body).json()["items"][0]["errors"]
        for i,item in enumerate(r for r in report["items"] if r["kind"]!="missing_prefix"):
            belongs=item["kind"] in {"missing_prefix","missing_road","different_road"}
            decision={"evidence_token":item["evidence_token"],"expected_revision":item["revision"],
                "decision":"belongs" if belongs else "other_address","verified_road":"Đường A",
                "verified_prefix":"91" if belongs else item["alley"],"evidence":"Đối chiếu hồ sơ nghiệm thu thực địa thử nghiệm"}
            url=addresses+"/"+item["building_id"]
            if i==0:
                assert c.put(url,json={**decision,"evidence_token":"0"*64}).status_code==409
                assert c.put(url,json={**decision,"decision":"belongs","verified_prefix":"99"}).status_code==422
            saved=c.put(url,json=decision);assert saved.status_code==200,saved.text
            assert saved.json()["building_updated"] is False
            if i==0:
                assert c.put(url,json=decision).status_code==409
                partial=c.post(module+"/route-import/preview",json=body).json()
                assert "review_address_pending" in partial["items"][0]["errors"]
        assert c.get(addresses).json()["counts"]["verified"]==6
        links=module+"/geometry-reviews/"+listing["items"][0]["id"]+"/verified-links"
        assert c.get(links).json()["state"]=="not_promoted"
        blocked=c.post(links,json={"preview_token":"0"*64,"building_ids":[by_feature["missing"]["building_id"]],"confirmed":True})
        assert blocked.status_code==409,blocked.text
        unchecked=c.post(module+"/route-import/preview",json=stage_body).json()
        assert "review_required" in unchecked["items"][0]["errors"]
        fixed=c.post(module+"/route-import/preview",json=body).json()
        assert fixed["items"][0]["state"]=="ready" and not fixed["items"][0]["geometry_stage_eligible"]
        applied=c.post(module+"/route-import/apply",json={**body,"preview_token":fixed["preview_token"],"selected_keys":[key]})
        assert applied.status_code==201,applied.text
        listing=c.get(module+"/geometry-reviews").json()
        assert listing["items"][0]["status"]=="promoted" and listing["items"][0]["promoted_route_id"]
        assert c.post(endpoint,json=payload).status_code==409
        assert c.put(url,json=decision).status_code==409
        plan=c.get(links);assert plan.status_code==200,plan.text
        plan=plan.json();rid=listing["items"][0]["promoted_route_id"]
        assert plan["state"]=="current" and plan["route"]["id"]==rid
        assert plan["counts"]=={"ready":4,"linked":0,"other_address":2,"blocked":0,"stale":0}
        ready=[r["building_id"] for r in plan["items"] if r["status"]=="ready"]
        other=next(r["building_id"] for r in plan["items"] if r["status"]=="other_address")
        req={"preview_token":plan["preview_token"],"building_ids":ready,"confirmed":True}
        assert c.post(links,json={**req,"confirmed":False}).status_code==422
        assert c.post(links,json={**req,"building_ids":[other]}).status_code==422
        assert c.post(links,json={**req,"building_ids":[ready[0],ready[0]]}).status_code==422
        assert c.post(links,json={**req,"preview_token":"0"*64}).status_code==409
        with get_admin_engine().begin() as conn:
            conn.execute(text("UPDATE spatial.reference_features SET attributes=jsonb_set(attributes,'{ten_duong}',to_jsonb(CAST(:name AS text))) WHERE source_id=:sid"),{"sid":refs["road"],"name":"Đường C"})
        assert c.post(links,json=req).status_code==409
        plan=c.get(links).json()
        assert plan["counts"]=={"ready":3,"linked":0,"other_address":2,"blocked":0,"stale":1}
        ready=[r["building_id"] for r in plan["items"] if r["status"]=="ready"]
        req={"preview_token":plan["preview_token"],"building_ids":ready,"confirmed":True}
        result=c.post(links,json=req);assert result.status_code==200,result.text
        assert result.json()["linked"]==3 and result.json()["numbers_created"]==0
        assert c.post(links,json=req).status_code==409
        remaining=c.get(links).json()
        assert remaining["counts"]=={"ready":0,"linked":3,"other_address":2,"blocked":0,"stale":1}
        targets=c.get(module+"/numbering-targets",params={"route_id":rid,"include_descendants":False})
        assert targets.status_code==200,targets.text
        assert {r["id"] for r in targets.json()["items"]}==set(ready)
        with get_admin_engine().connect() as conn:
            assert conn.execute(text("SELECT count(*) FROM house_numbering.buildings WHERE organization_id=:org AND route_id IS NOT NULL"),{"org":world.organization_id}).scalar_one()==3
            assert conn.execute(text("SELECT count(*) FROM house_numbering.address_numbers WHERE organization_id=:org"),{"org":world.organization_id}).scalar_one()==0
            assert conn.execute(text("SELECT count(*) FROM audit.audit_events WHERE organization_id=:org AND action='house_numbering.building.link_verified_address'"),{"org":world.organization_id}).scalar_one()==3
            assert conn.execute(text("SELECT attributes->>'hem' FROM spatial.reference_features WHERE source_id=:sid"),{"sid":refs["desc"]}).scalar_one()=="91/14"
            assert conn.execute(text("SELECT count(*) FROM audit.audit_events WHERE organization_id=:org AND action='house_numbering.building.verify_address'"),{"org":world.organization_id}).scalar_one()==6
            assert conn.execute(text("SELECT count(*) FROM audit.audit_events WHERE organization_id=:org AND action='house_numbering.building.verify_address_batch'"),{"org":world.organization_id}).scalar_one()==1
