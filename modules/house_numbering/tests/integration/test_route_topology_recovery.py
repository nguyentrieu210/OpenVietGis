"""Real PostGIS topology recovery: explicit grouping and actual parent geometry."""
import json
from uuid import uuid4

from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client

from modules.house_numbering.migrate import upgrade as migrate_house_numbering
from modules.house_numbering.tests.integration.test_house_numbering_edge_acceptance import (
    _activate_house,
)
from modules.house_numbering.tests.integration.test_house_numbering_runtime import (
    _seed_source,
)


def test_recovery_groups_and_missing_parent_using_existing_gis_geometry(migrated_database):
    migrate_house_numbering()
    world=seed_access_world()
    def seed(code,name,kind,parent,prefix,wkt,review=True):
        sid=_seed_source(organization_id=world.organization_id,project_id=world.project_id,
            reference_key=f"topology_{uuid4().hex[:10]}",family="LineString",feature_id=code,wkt=wkt,
            extent={"min_x":0,"min_y":0,"max_x":300,"max_y":300,"crs_code":3857})
        attrs={"code":code,"name":name,"kind":kind,"parent":parent,"prefix":prefix}
        if review:attrs["review_reason"]="legacy"
        with get_admin_engine().begin() as conn:
            conn.execute(text("UPDATE spatial.reference_features SET attributes=CAST(:attrs AS jsonb) WHERE source_id=:sid"),{"sid":sid,"attrs":json.dumps(attrs)})
        return {"source_id":str(sid),"mapping":{"code":"code","name":"name","route_type":"kind","parent_code":"parent","prefix":"prefix"}}
    root=seed("R","Đường A","street","","","LINESTRING(0 0,100 0)",False)
    a=seed("A","Đường A hẻm 12/3","alley","","12/3","LINESTRING(30 30,60 30)")
    b=seed("B","Đường A hẻm 12/3","street","","12/3","LINESTRING(60 30,90 30)")
    bad1=seed("D1","Đường A hẻm 99","alley","R","99","LINESTRING(5 0,5 10)")
    bad2=seed("D2","Đường A hẻm 99","alley","R","99","LINESTRING(100 100,120 120)")
    parent=seed("P","Other", "street","","","LINESTRING(30 0,30 30)",False)
    far=seed("F","Other","street","","","LINESTRING(250 250,260 260)",False)
    layers=[root,a,b,bad1,bad2]
    body={"layers":layers}
    with _client(world.owner_user_id) as client:
        _,_,module,_=_activate_house(client,world)
        response=client.post(module+"/route-import/preview",json=body)
        assert response.status_code==200,response.text
        p=response.json()
        opts={g["prefix"]:g for g in p["merge_options"]}
        assert opts["12/3"]["status"]=="connected"
        assert opts["99"]["status"]=="disconnected"
        assert len(p["missing_parents"])==1
        missing=p["missing_parents"][0]
        assert missing["prefix"]=="12" and missing["children"]==2
        blocked=client.post(module+"/route-import/preview",json={**body,"merge_groups":[opts["99"]["id"]]})
        assert blocked.status_code==422,blocked.text
        parent_feature={"task_id":missing["id"],"source_id":parent["source_id"],"feature_id":"P"}
        merge_body={**body,"merge_groups":[opts["12/3"]["id"]],"parent_features":[parent_feature]}
        far_body={**merge_body,"parent_features":[{**parent_feature,"source_id":far["source_id"],"feature_id":"F"}]}
        bad=client.post(module+"/route-import/preview",json=far_body)
        assert bad.status_code==200,bad.text
        bad_parent=next(r for r in bad.json()["items"] if r.get("manual_parent"))
        assert "review_parent_has_no_connected_child" in bad_parent["errors"]
        grouped=client.post(module+"/route-import/preview",json=merge_body)
        assert grouped.status_code==200,grouped.text
        rows=grouped.json()["items"]
        merged=next(r for r in rows if r.get("segments"))
        manual=next(r for r in rows if r.get("manual_parent"))
        assert len(merged["segments"])==2 and merged["route_type"]=="alley"
        assert manual["manual_parent_child_count"]==1
        assert grouped.json()["missing_parents"]==[]
        overrides={manual["key"]:{"parent_code":"R","prefix":"12","reviewed":True},
                   merged["key"]:{"parent_code":manual["code"],"prefix":"12/3","reviewed":True}}
        confirmed={**merge_body,"overrides":overrides}
        preview=client.post(module+"/route-import/preview",json=confirmed)
        assert preview.status_code==200,preview.text
        p=preview.json()
        assert p["ready"]==3,p
        selected=[r["key"] for r in p["items"] if r["state"]=="ready"]
        result=client.post(module+"/route-import/apply",json={**confirmed,"selected_keys":selected,"preview_token":p["preview_token"]})
        assert result.status_code==201,result.text
        assert result.json()["created"]==3
        actual=client.get(module+"/routes").json()["items"]
        assert len(actual)==3
        child=next(r for r in actual if r["prefix"]=="12/3")
        saved_parent=next(r for r in actual if r["prefix"]=="12")
        assert child["parent_route_id"]==saved_parent["id"] and len(child["segments"])==2
        # Reopening the source without a merge request must restore the saved group.
        again=client.post(module+"/route-import/preview",json=body)
        assert again.status_code==200,again.text
        assert again.json()["existing"]==2,again.text
        assert len(again.json()["missing_parents"])==0
        replay=client.post(module+"/route-import/preview",json=confirmed)
        assert replay.status_code==200,replay.text
        assert replay.json()["existing"]==3,replay.text
