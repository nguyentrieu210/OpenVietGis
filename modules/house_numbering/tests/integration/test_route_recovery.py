"""Recovery must remain opt-in, dependency-safe and topology checked."""
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


def test_review_parent_suggestions_and_partial_apply(migrated_database):
    migrate_house_numbering()
    world = seed_access_world()
    def seed(code,name,kind,parent,prefix,wkt,review=True):
        sid=_seed_source(organization_id=world.organization_id,project_id=world.project_id,
            reference_key=f"recovery_{uuid4().hex[:10]}",family="LineString",feature_id=code,wkt=wkt,
            extent={"min_x":0,"min_y":0,"max_x":300,"max_y":300,"crs_code":3857})
        attrs={"code":code,"name":name,"kind":kind,"parent":parent,"prefix":prefix}
        if review:attrs["review_reason"]="legacy"
        with get_admin_engine().begin() as conn:
            conn.execute(text("UPDATE spatial.reference_features SET attributes=CAST(:attrs AS jsonb) WHERE source_id=:sid"),{"sid":sid,"attrs":json.dumps(attrs)})
        return {"source_id":str(sid),"mapping":{"code":"code","name":"name","route_type":"kind","parent_code":"parent","prefix":"prefix"}}
    root=seed("R","Đường A","street","","","LINESTRING(0 0,100 0)",False)
    a=seed("A","Đường A hẻm C19","street","","C19","LINESTRING(30 0,30 30)")
    b=seed("B","Đường A hẻm C19/30A","alley","A","C19/30A","LINESTRING(30 30,60 30)")
    bad=seed("X","Đường A hẻm 99","alley","R","99","LINESTRING(150 150,200 200)")
    body={"layers":[b,bad,a,root]}
    key=lambda layer,code:layer["source_id"]+":"+code
    with _client(world.owner_user_id) as client:
        _,_,module,_=_activate_house(client,world)
        p=client.post(module+"/route-import/preview",json=body)
        assert p.status_code==200,p.text
        rows={r["code"]:r for r in p.json()["items"]}
        assert rows["A"]["suggested_type"]=="alley"
        assert rows["A"]["parent_suggestions"][0]["code"]=="R"
        assert rows["A"]["parent_suggestions"][0]["connected"]
        assert "review_required" in rows["A"]["errors"]
        assert "review_parent_not_connected" in rows["X"]["errors"]
        overrides={key(a,"A"):{"parent_code":"R","prefix":"C19","route_type":"alley","reviewed":True},
            key(b,"B"):{"parent_code":"A","prefix":"C19/30A","reviewed":True},
            key(bad,"X"):{"parent_code":"R","prefix":"99","reviewed":True}}
        body={**body,"overrides":overrides}
        p=client.post(module+"/route-import/preview",json=body)
        assert p.status_code==200,p.text
        assert p.json()["ready"]==3,p.text
        assert p.json()["errors"]==1
        token=p.json()["preview_token"]
        for selected in [[key(b,"B")],[key(bad,"X")],[key(root,"R"),key(root,"R")]]:
            rejected=client.post(module+"/route-import/apply",json={**body,"preview_token":token,"selected_keys":selected})
            assert rejected.status_code==422,rejected.text
        stale=client.post(module+"/route-import/apply",json={**body,"preview_token":"f"*64,"selected_keys":[key(root,"R")]})
        assert stale.status_code==409,stale.text
        old=client.post(module+"/route-import/apply",json={**body,"preview_token":token})
        assert old.status_code==422,old.text
        ok=client.post(module+"/route-import/apply",json={**body,"preview_token":token,"selected_keys":[key(root,"R"),key(a,"A"),key(b,"B")]})
        assert ok.status_code==201,ok.text
        assert ok.json()["created"]==3 and ok.json()["remaining"]==1
        routes={r["name"]:r for r in client.get(module+"/routes").json()["items"]}
        assert len(routes)==3 and routes["Đường A hẻm C19/30A"]["parent_route_id"]==routes["Đường A hẻm C19"]["id"]
        # Raw A was typed street, but an already recovered link remains authoritative.
        after=client.post(module+"/route-import/preview",json={"layers":body["layers"]})
        assert after.status_code==200,after.text
        assert after.json()["existing"]==3,after.text
        assert after.json()["errors"]==1
