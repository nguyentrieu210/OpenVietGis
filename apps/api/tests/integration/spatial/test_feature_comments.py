from uuid import uuid4
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


def test_feature_comments_mentions_and_assignments(migrated_database):
    world=seed_access_world()
    org=f"/api/v1/organizations/{world.organization_id}"
    root=f"{org}/projects/{world.project_id}"
    base=root+"/collaboration"
    with _client(world.owner_user_id) as owner,_client(world.member_user_id) as member,_client(world.wrong_user_id) as outsider:
        def check(r,status=200):
            assert r.status_code==status,r.text
            return r.json()
        check(owner.post(org+"/grants",json={"subject_kind":"membership","subject_id":str(world.member_membership_id),"scope_kind":"project","project_id":str(world.project_id),"role":"editor"}),201)
        layer=check(owner.post(root+"/layers/manual",json={"operation_id":str(uuid4()),"name":"Discussion","geometry_family":"Point","fields":[{"field_id":"nm","name":"Name","type":"text","nullable":False}]}),201)
        source=layer["source_id"]
        def commit(operations):
            src=check(owner.get(root+"/layers/"+layer["layer_id"]))["source"]
            return check(owner.post(root+"/sources/"+source+"/changesets",json={"operation_id":str(uuid4()),"base_content_revision":src["content_revision"],"base_schema_revision":src["schema_revision"],"summary":"Test","operations":operations}),201)
        commit([{"op_type":"create","feature_id":f,"geometry":{"type":"Point","coordinates":[106.5,10.7]},"properties":{"nm":f}} for f in ("A","B")])
        target={"source_id":source,"feature_id":"A"}
        comment=target|{"body":"Check boundary","mention_ids":[str(world.member_user_id)]}
        assert outsider.post(base+"/comments",json=comment).status_code in (403,404)
        assert owner.post(base+"/comments",json=comment|{"mention_ids":[str(world.wrong_user_id)]}).status_code==422
        assert owner.post(base+"/comments",json=comment|{"feature_id":"missing"}).status_code==404
        msg=check(owner.post(base+"/comments",json=comment),201)
        assert msg["mentions"][0]["id"]==str(world.member_user_id)
        assert abs(msg["longitude"]-106.5)<1e-8 and abs(msg["latitude"]-10.7)<1e-8
        assert check(member.get(base+"/comments",params=target))["items"]==[msg]
        assert check(member.get(base+"/comments",params=target|{"feature_id":"B"}))["items"]==[]
        assert check(member.get(base+"/messages"))["items"]==[]
        pins=check(member.get(base+"/comment-pins"))["items"]
        assert len(pins)==1 and pins[0]["feature_id"]=="A" and pins[0]["comment_count"]==1
        assert outsider.get(base+"/comments",params=target).status_code in (403,404)
        inbox=check(member.get(base+"/notifications"))
        assert inbox["unread"]==1 and inbox["items"][0]["kind"]=="mention"
        assert check(owner.get(base+"/notifications"))["unread"]==0
        assert owner.post(base+"/mentions/"+msg["id"]+"/read").status_code==404
        check(member.post(base+"/mentions/"+msg["id"]+"/read"))
        check(member.post(base+"/mentions/"+msg["id"]+"/read"))
        assert check(member.get(base+"/notifications"))["unread"]==0
        payload={"title":"Inspect","assignee_id":str(world.member_user_id)}
        task=check(owner.post(base+"/tasks",json=payload|target|{"longitude":1,"latitude":2}),201)
        assert task["feature_id"]=="A" and task["longitude"]==msg["longitude"]
        assert owner.post(base+"/tasks",json=payload|target|{"feature_id":"B","source_message_id":msg["id"]}).status_code==422
        commit([{"op_type":"delete","feature_id":"A"}])
        converted=check(owner.post(base+"/tasks",json=payload|{"source_message_id":msg["id"]}),201)
        assert converted["source_message_id"]==msg["id"] and converted["feature_id"]=="A" and converted["longitude"]==msg["longitude"]
        assert check(member.get(base+"/comments",params=target))["items"]==[msg]
        check(owner.post(root+"/access/rules",json={"user_id":str(world.member_user_id),"layer_id":layer["layer_id"],"actions":["read"],"attribute_filter":{"nm":"B"}}),201)
        assert member.get(base+"/comments",params=target).status_code==403
        assert member.get(base+"/notifications").status_code==403
