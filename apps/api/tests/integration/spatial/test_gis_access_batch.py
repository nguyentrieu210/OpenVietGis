from uuid import uuid4
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_layers import _path
from tests.integration.spatial.test_sources import _client
from tests.integration.m3.test_extension_lifecycle_security import _manifest

def test_batch_rules_and_shared_activation(migrated_database):
    world=seed_access_world()
    path=_path(world);root=path.rsplit("/",1)[0];org=root.split("/projects/")[0]
    with _client(world.owner_user_id) as admin, _client(world.member_user_id) as member:
        def check(r,status=200):
            assert r.status_code==status,r.text
            return r.json()
        def layer(name):
            return check(admin.post(path+"/manual",json={"operation_id":str(uuid4()),"name":name,"geometry_family":"Point","fields":[{"field_id":"nm","name":"Tên","type":"text","nullable":False}]}),201)
        a,b=layer("A"),layer("B")
        check(admin.post(org+"/grants",json={"subject_kind":"membership","subject_id":str(world.member_membership_id),"scope_kind":"project","project_id":str(world.project_id),"role":"editor"}),201)
        manifest=check(admin.post(org+"/extensions/manifests",json=_manifest()),201)
        check(admin.post(root+"/extensions/activations",json={"manifest_id":manifest["id"]}),201)
        uid=str(world.member_user_id)
        rule_a={"user_id":uid,"layer_id":a["layer_id"],"actions":["read","geometry"],"attribute_filter":{"nm":"A"}}
        rule_b={"user_id":uid,"layer_id":b["layer_id"],"actions":["read"],"region":{"type":"Polygon","coordinates":[[[106.4,10.6],[106.55,10.6],[106.55,10.8],[106.4,10.8],[106.4,10.6]]]}}
        url=root+"/access/rules/batch"
        assert admin.post(url,json={"rules":[rule_a,{**rule_b,"layer_id":str(uuid4())}]}).status_code==404
        assert check(admin.get(root+"/access"))["rules"]==[]
        assert not check(member.get(root+"/access/mine"))["restricted"]
        assert check(admin.post(url,json={"rules":[rule_a,rule_b]}))=={"created":2,"unchanged":0,"revoked":0}
        assert check(admin.post(url,json={"rules":[rule_a,rule_b]}))=={"created":0,"unchanged":2,"revoked":0}
        assert check(member.get(root+"/access/mine"))["restricted"]
        assert any(r["status"]=="active" and r["extension_key"]=="audit_notes" for r in check(member.get(root+"/extensions/activations"))["items"])
        assert member.post(url,json={"rules":[rule_a]}).status_code==403
        ids=[r["id"] for r in check(admin.get(root+"/access"))["rules"]]
        assert len(ids)==2
        assert admin.post(url,json={"revoke_ids":[ids[0],str(uuid4())]}).status_code==404
        assert len(check(admin.get(root+"/access"))["rules"])==2
        assert check(admin.post(url,json={"revoke_ids":ids}))=={"created":0,"unchanged":0,"revoked":2}
        assert check(admin.get(root+"/access"))["rules"]==[]
