"""Real PostGIS acceptance for reviewable building-to-route assignment."""
import json
from uuid import uuid4

from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client

from modules.house_numbering.migrate import upgrade as migrate_house_numbering
from modules.house_numbering.tests.integration.test_house_numbering_edge_acceptance import (
    _activate_house,
    _create_route,
)
from modules.house_numbering.tests.integration.test_house_numbering_runtime import (
    _seed_source,
)


def test_assignment_prefers_address_over_nearest_and_never_overwrites(migrated_database):
    migrate_house_numbering()
    world=seed_access_world()
    def seed(fid,wkt,family,attrs):
        source=_seed_source(organization_id=world.organization_id,project_id=world.project_id,
            reference_key=f"assignment_{uuid4().hex[:8]}",family=family,feature_id=fid,wkt=wkt,
            extent={"min_x":0,"min_y":0,"max_x":100,"max_y":100,"crs_code":3857})
        with get_admin_engine().begin() as conn:
            conn.execute(text("UPDATE spatial.reference_features SET attributes=CAST(:attrs AS jsonb) WHERE source_id=:source"),
                         {"source":source,"attrs":json.dumps(attrs)})
        return source
    main_source=seed("main","LINESTRING(0 0,100 0)","LineString",{})
    alley_source=seed("branch","LINESTRING(50 0,50 80)","LineString",{})
    with _client(world.owner_user_id) as client:
        _,_,module,_=_activate_house(client,world)
        main=_create_route(client,module,name="Đường Nguyễn Văn Linh",source_id=main_source,feature_id="main")
        alley=_create_route(client,module,name="Hẻm 155",source_id=alley_source,feature_id="branch",parent_route_id=main["id"],prefix="155")
        buildings={}
        for fid,point,attrs,existing in [
            ("b1","POINT(55 2)",{"ten_duong":"Nguyen Van Linh","hem":"155"},None),
            ("b2","POINT(20 3)",{"ten_duong":"Nguyễn Văn Linh","hem":""},None),
            ("b3","POINT(54 5)",{},None),
            ("b4","POINT(22 4)",{"ten_duong":"Nguyễn Văn Linh","hem":"155"},main["id"]),
        ]:
            s=seed(fid,point,"Point",attrs)
            resp=client.post(module+"/buildings",json={"route_id":existing,"source_id":str(s),"feature_id":fid})
            assert resp.status_code==201,resp.text
            buildings[fid]=resp.json()["id"]
        body={"limit":100,"max_distance_m":80}
        preview=client.post(module+"/building-assignment/preview",json=body)
        assert preview.status_code==200,preview.text
        plan=preview.json()
        assert plan["counts"]["high"]==2 and plan["counts"]["low"]==1,plan
        rows={r["feature_id"]:r for r in plan["items"]}
        assert set(rows)=={"b1","b2","b3"}
        assert rows["b1"]["suggested_route_id"]==alley["id"]
        assert rows["b1"]["candidates"][0]["distance_m"]>rows["b1"]["candidates"][1]["distance_m"]
        assert rows["b3"]["suggested_route_id"] is None
        assert all(b["route_id"] is None for b in client.get(module+"/buildings").json()["items"] if b["id"]!=buildings["b4"])
        stale=client.post(module+"/building-assignment/apply",json={**body,"preview_token":"0"*64,"assignments":{buildings["b1"]:alley["id"]}})
        assert stale.status_code==409,stale.text
        apply=client.post(module+"/building-assignment/apply",json={**body,"preview_token":plan["preview_token"],
            "assignments":{buildings["b1"]:alley["id"],buildings["b2"]:main["id"]}})
        assert apply.status_code==200,apply.text
        assert apply.json()["assigned"]==2
        after={b["id"]: b for b in client.get(module+"/buildings").json()["items"]}
        assert after[buildings["b1"]]["route_id"]==alley["id"]
        assert after[buildings["b2"]]["route_id"]==main["id"]
        assert after[buildings["b3"]]["route_id"] is None
        assert after[buildings["b4"]]["route_id"]==main["id"]
        pending=seed("pending","LINESTRING(0 50,100 50)","LineString",{"route_code":"legacy","review_reason":"parent_required;invalid_prefix","parent_code":"missing","ten":"Hẻm 77"})
        s=seed("b5","POINT(50 52)","Point",{"ten_duong":"Nguyễn Văn Linh","hem":"77","tuyen_id":"legacy"})
        b5=client.post(module+"/buildings",json={"source_id":str(s),"feature_id":"b5"}).json()["id"]
        s2=seed("b6","POINT(30 5)","Point",{"ten_duong":"Nguyễn Văn Linh","hem":""})
        b6=client.post(module+"/buildings",json={"source_id":str(s2),"feature_id":"b6"}).json()["id"]
        audit=client.post(module+"/building-assignment/audit",json={"group":"parent_missing","limit":100})
        assert audit.status_code==200,audit.text
        report=audit.json()
        assert report["total"]==3
        assert sum(g["count"] for g in report["groups"])==3
        assert report["items"][0]["building_id"]==b5
        assert report["items"][0]["pending"][0]["source_id"]==str(pending)
        cluster=report["clusters"][0]["key"]
        clustered=client.post(module+"/building-assignment/audit",json={"group":"parent_missing","cluster":cluster}).json()
        assert [x["building_id"] for x in clustered["items"]]==[b5]
        ready=client.post(module+"/building-assignment/audit",json={"group":"ready"}).json()
        assert [x["building_id"] for x in ready["items"]]==[b6]
        scoped=client.post(module+"/building-assignment/preview",json={"building_ids":[b6],"strict":True,"limit":100})
        assert scoped.status_code==200,scoped.text
        assert scoped.json()["counts"]["high"]==1
        assert scoped.json()["items"][0]["building_id"]==b6
        assert client.post(module+"/building-assignment/preview",json={"building_ids":[b5,b5],"limit":100}).status_code==422
        assert client.post(module+"/building-assignment/preview",json={"building_ids":[buildings["b1"]],"limit":100}).status_code==409
        apply=client.post(module+"/building-assignment/apply",json={"building_ids":[b6],"limit":100,"strict":True,"preview_token":scoped.json()["preview_token"],"assignments":{b6:main["id"]}})
        assert apply.status_code==200,apply.text
        assert client.post(module+"/building-assignment/audit",json={}).json()["total"]==2
