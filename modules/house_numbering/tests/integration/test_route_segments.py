"""Grouped Core line links and alphanumeric alley import on real PostGIS."""
import json
from uuid import uuid4
from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client
from modules.house_numbering.migrate import upgrade as migrate_house_numbering
from modules.house_numbering.tests.integration.test_house_numbering_runtime import _seed_source
from modules.house_numbering.tests.integration.test_house_numbering_edge_acceptance import _activate_house

def test_grouped_segments_alpha_hierarchy_and_alias(migrated_database):
    migrate_house_numbering()
    world=seed_access_world()
    def seed(fid,wkt,attrs,family="LineString"):
        sid=_seed_source(organization_id=world.organization_id,project_id=world.project_id,
            reference_key=f"seg_{uuid4().hex[:8]}",family=family,feature_id=fid,wkt=wkt,
            extent={"min_x":0,"min_y":0,"max_x":300,"max_y":100,"crs_code":3857})
        with get_admin_engine().begin() as conn:
            conn.execute(text("UPDATE spatial.reference_features SET attributes=CAST(:attrs AS jsonb) WHERE source_id=:sid"),{"sid":sid,"attrs":json.dumps(attrs)})
        return sid
    def layer(code,logical,name,parent,prefix,wkt):
        sid=seed(code,wkt,{"code":code,"logical":logical,"name":name,"parent":parent,"prefix":prefix})
        return {"source_id":str(sid),"default_type":"alley" if parent else "street",
                "mapping":{"code":"code","logical_code":"logical","name":"name","parent_code":"parent","prefix":"prefix"}}
    main_a=layer("M1","MAIN","Đường A","","","LINESTRING(0 0,100 0)")
    main_b=layer("M2","MAIN","Đường A","","","LINESTRING(200 0,100 0)")
    alley=layer("A1","","Hẻm C19","M2","c19","LINESTRING(150 0,150 40)")
    child=layer("A2","","Hẻm C19/30A","A1","30a","LINESTRING(150 40,200 40)")
    body={"layers":[main_a,main_b,alley,child]}
    with _client(world.owner_user_id) as client:
        _,_,module,_=_activate_house(client,world)
        bad=layer("M3","MAIN","Đường A","","","LINESTRING(250 0,300 0)")
        invalid=client.post(module+"/route-import/preview",json={"layers":[main_a,main_b,bad]})
        assert invalid.status_code==200,invalid.text
        assert invalid.json()["errors"]==1,invalid.text
        assert "invalid_route_segments" in invalid.json()["items"][0]["errors"]
        branch=layer("M4","MAIN","Đường A","","","LINESTRING(100 0,100 20)")
        invalid_branch=client.post(module+"/route-import/preview",json={"layers":[main_a,main_b,branch]})
        assert invalid_branch.status_code==200 and invalid_branch.json()["errors"]==1,invalid_branch.text
        p=client.post(module+"/route-import/preview",json=body)
        assert p.status_code==200,p.text
        assert p.json()["ready"]==3 and p.json()["errors"]==0,p.text
        got=client.post(module+"/route-import/apply",json={**body,"preview_token":p.json()["preview_token"]})
        assert got.status_code==201,got.text
        routes={r["name"]:r for r in client.get(module+"/routes").json()["items"]}
        main=routes["Đường A"]; assert len(main["segments"])==2
        assert routes["Hẻm C19"]["parent_route_id"]==main["id"]
        assert routes["Hẻm C19/30A"]["prefix"]=="C19/30A"
        again=client.post(module+"/route-import/preview",json=body)
        assert again.status_code==200 and again.json()["existing"]==3,again.text
        rule=client.post(module+"/rules",json={"version":"seg-test","status":"draft","max_distance_m":50}).json()
        ins=client.post(module+"/inspect",json={"x":170,"y":-5,"crs_code":3857,"route_id":main["id"],"rule_id":rule["id"]})
        assert ins.status_code==200,ins.text
        assert abs(ins.json()["location"]["measure_m"]-170)<1
        assert ins.json()["location"]["side"]=="right"
        ins_alley=client.post(module+"/inspect",json={"x":170,"y":35,"crs_code":3857,"route_id":routes["Hẻm C19/30A"]["id"],"rule_id":rule["id"]})
        assert ins_alley.status_code==200,ins_alley.text
        assert ins_alley.json()["suggestion"]["number"]=="C19/30A/2"
        pt=seed("B","POINT(170 -5)",{"ten_duong":"Đường A","hem":"","tuyen_id":"M2"},"Point")
        b=client.post(module+"/buildings",json={"source_id":str(pt),"feature_id":"B"})
        assert b.status_code==201,b.text
        pa=client.post(module+"/building-assignment/preview",json={"limit":100})
        assert pa.status_code==200,pa.text
        assert pa.json()["items"][0]["suggested_route_id"]==main["id"]
        assert pa.json()["items"][0]["confidence"]=="high"
        applied=client.post(module+"/building-assignment/apply",json={"limit":100,"preview_token":pa.json()["preview_token"],"assignments":{b.json()["id"]:main["id"]}})
        assert applied.status_code==200,applied.text
        pv=client.post(module+"/previews",json={"building_ids":[b.json()["id"]],"rule_id":rule["id"],"operation_id":str(uuid4())})
        assert pv.status_code==201 and pv.json()["candidate_changes"],pv.text
        with get_admin_engine().begin() as conn:
            conn.execute(text("UPDATE spatial.data_sources SET content_revision=:rev WHERE id=:sid"),{"rev":"e"*64,"sid":main_b["source_id"]})
            conn.execute(text("UPDATE spatial.reference_features SET content_revision=:rev WHERE source_id=:sid"),{"rev":"e"*64,"sid":main_b["source_id"]})
        stale=client.post(module+"/previews/"+pv.json()["id"]+"/apply")
        assert stale.status_code==409,stale.text
        # Source geometry outside project is not readable by the route creator.
        other=seed_access_world()
        outsider=_seed_source(organization_id=other.organization_id,project_id=other.project_id,
            reference_key=f"seg_out_{uuid4().hex[:8]}",family="LineString",feature_id="O",
            wkt="LINESTRING(300 0,400 0)",extent={"min_x":300,"min_y":0,"max_x":400,"max_y":0,"crs_code":3857})
        fresh=seed("N","LINESTRING(200 0,300 0)",{})
        bad_create=client.post(module+"/routes",json={"name":"Cross project","source_id":str(fresh),"feature_id":"N",
            "segments":[{"source_id":str(fresh),"feature_id":"N"},{"source_id":str(outsider),"feature_id":"O"}]})
        assert bad_create.status_code==422,bad_create.text
