"""Numbering a route tree in one coherent, stale-safe batch."""
from uuid import uuid4
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client
from modules.house_numbering.migrate import upgrade
from modules.house_numbering.tests.integration.test_house_numbering_runtime import _seed_source
from modules.house_numbering.tests.integration.test_house_numbering_edge_acceptance import _activate_house

def test_batch_route_tree(migrated_database):
    upgrade()
    world=seed_access_world()
    def seed(fid,wkt,family):
        return _seed_source(organization_id=world.organization_id,project_id=world.project_id,
            reference_key=f"batch_{uuid4().hex[:8]}",family=family,feature_id=fid,wkt=wkt,
            extent={"min_x":0,"min_y":-10,"max_x":120,"max_y":60,"crs_code":3857})
    with _client(world.owner_user_id) as c:
        _,_,module,_=_activate_house(c,world)
        def post(path,body):
            r=c.post(module+path,json=body)
            assert r.status_code in (200,201), r.text
            return r.json()
        rule=post("/rules",{"version":"batch-safe","status":"draft","max_distance_m":50})
        root=post("/routes",{"name":"Road","route_type":"street","source_id":str(seed("R","LINESTRING(0 0,100 0)","LineString")),"feature_id":"R","direction_state":"forward","status":"confirmed"})
        child=post("/routes",{"name":"Hẻm C19","route_type":"alley","parent_route_id":root["id"],"prefix":"C19","source_id":str(seed("A","LINESTRING(0 20,100 20)","LineString")),"feature_id":"A","direction_state":"forward","status":"confirmed"})
        def house(name,x,y,route):
            return post("/buildings",{"route_id":route["id"],"source_id":str(seed(name,f"POINT({x} {y})","Point")),"feature_id":name,"eligibility":"eligible"})["id"]
        a,b,d,e,f=[house(n,x,y,r) for n,x,y,r in [("a",10,-3,root),("b",20,-3,root),("d",30,3,root),("e",10,17,child),("f",20,17,child)]]
        targets=c.get(module+"/numbering-targets",params={"route_id":root["id"]})
        assert targets.status_code==200,targets.text
        assert targets.json()["total"]==5
        assert c.get(module+"/numbering-targets",params={"route_id":root["id"],"include_descendants":False}).json()["total"]==3
        def preview(ids):
            return post("/previews",{"building_ids":ids,"rule_id":rule["id"],"operation_id":str(uuid4())})
        dup=c.post(module+"/previews",json={"building_ids":[a,a],"rule_id":rule["id"],"operation_id":str(uuid4())})
        assert dup.status_code==422,dup.text
        p=preview([f,b,e,d,a])
        assert p["audit"]["routes"]==2 and p["audit"]["can_save"]
        assert p["audit"]["proposed"]==5 and p["audit"]["duplicate_count"]==0
        assert p["audit"]["parity_error_count"]==0 and p["audit"]["ordering_error_count"]==0
        assert {v["building_id"]:v["number"] for v in p["candidate_changes"]}=={a:"2",b:"4",d:"1",e:"C19/2",f:"C19/4"},p
        stale=preview([a,b])
        applied=c.post(module+"/previews/"+p["id"]+"/apply")
        assert applied.status_code==200,applied.text
        failed=c.post(module+"/previews/"+stale["id"]+"/apply")
        assert failed.status_code==409,failed.text
        again=preview([a,b])
        assert not again["candidate_changes"] and len(again["warnings"])==2
        assert c.get(module+"/numbering-targets",params={"route_id":root["id"]}).json()["total"]==0
