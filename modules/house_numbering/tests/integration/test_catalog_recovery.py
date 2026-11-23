"""Restoring code-scoped address aliases never rewrites official route or survey data."""
import json
from uuid import uuid4

from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client

from modules.house_numbering.migrate import upgrade
from modules.house_numbering.tests.integration.test_house_numbering_edge_acceptance import (
    _activate_house,
    _create_route,
)
from modules.house_numbering.tests.integration.test_house_numbering_runtime import (
    _seed_source,
)


def test_catalog_reviewed_alias_is_code_scoped_and_stale_safe(migrated_database):
    upgrade()
    world=seed_access_world()
    def seed(fid,wkt,family,attrs):
        sid=_seed_source(organization_id=world.organization_id,project_id=world.project_id,
            reference_key=f"catalog_{uuid4().hex[:8]}",family=family,feature_id=fid,wkt=wkt,
            extent={"min_x":0,"min_y":0,"max_x":100,"max_y":150,"crs_code":3857})
        with get_admin_engine().begin() as conn:
            conn.execute(text("UPDATE spatial.reference_features SET attributes=CAST(:attrs AS jsonb) WHERE source_id=:sid"),{"sid":sid,"attrs":json.dumps(attrs)})
        return sid
    with _client(world.owner_user_id) as c:
        _,_,module,_=_activate_house(c,world)
        old=_create_route(c,module,name="Đường số 9",source_id=seed("old","LINESTRING(0 0,100 0)","LineString",{"ten":"Đường số 9"}),feature_id="old")
        ids=[];sources=[]
        for fid,y,alley,code in [("a",3,"",old["id"]),("b",8,"",old["id"]),("far",100,"",old["id"]),("wrong_prefix",4,"77",old["id"]),("nocode",5,"","")]:
            attrs={"ten_duong":"Đường số 9 Ấp 21","hem":alley,"tuyen_id":code}
            sid=seed(fid,f"POINT(20 {y})","Point",attrs);sources.append(sid)
            r=c.post(module+"/buildings",json={"source_id":str(sid),"feature_id":fid,"eligibility":"eligible"})
            assert r.status_code==201,r.text
            ids.append(r.json()["id"])
        seed("pending","LINESTRING(20 0,20 50)","LineString",{"ten":"Đường số 9 Ấp 21 hẻm 77","prefix":"77","route_type":"alley","route_code":"legacy_77","review_reason":"parent_required"})
        ep=module+"/building-assignment/catalog-recovery"
        resp=c.post(ep+"/preview",json={})
        assert resp.status_code==200,resp.text
        report=resp.json()
        assert report["counts"]["total"]==4 and report["counts"]["prefix_mismatch"]==1
        assert report["counts"]["same_prefix"]==3 and report["counts"]["near"]==2
        assert report["counts"]["source_path_houses"]==1
        assert len(report["items"])==1
        row=report["items"][0]
        assert row["reviewable"] and row["near"]==2 and row["count"]==3
        payload={"selections":{row["id"]:row["token"]},"reason":"Đã xác minh tên đường tại Ấp 21"}
        assert c.post(ep+"/apply",json={**payload,"selections":{row["id"]:"0"*64}}).status_code==409
        applied=c.post(ep+"/apply",json=payload);assert applied.status_code==200,applied.text
        assert applied.json()=={"aliases":1,"routes":1,"potential_houses":2}
        assert c.post(ep+"/apply",json=payload).status_code==409
        result=c.post(module+"/building-assignment/preview",json={"building_ids":ids,"strict":True,"limit":100})
        assert result.status_code==200,result.text
        high={x["building_id"] for x in result.json()["items"] if x["confidence"]=="high"}
        assert high==set(ids[:2]),result.text
        with get_admin_engine().connect() as conn:
            route=conn.execute(text("SELECT name,reviewed_aliases FROM house_numbering.routes WHERE id=:id"),{"id":old["id"]}).mappings().one()
            assert route["name"]=="Đường số 9" and len(route["reviewed_aliases"])==1
            assert conn.execute(text("SELECT count(*) FROM house_numbering.buildings WHERE organization_id=:org AND route_id IS NOT NULL"),{"org":world.organization_id}).scalar_one()==0
            assert conn.execute(text("SELECT attributes->>'ten_duong' FROM spatial.reference_features WHERE source_id=:sid"),{"sid":sources[0]}).scalar_one()=="Đường số 9 Ấp 21"
            assert conn.execute(text("SELECT count(*) FROM audit.audit_events WHERE organization_id=:org AND action='house_numbering.route.review_address_alias'"),{"org":world.organization_id}).scalar_one()==1
