"""Authenticated R029 and collaboration assertions against the Rust API."""
from uuid import uuid4

from attribute_query import baseline, project_path, session_headers
from openvietgis_api.security.csrf import make_csrf_token


def check_realtime_collaboration(c,f,second):
    uid,_,token=second
    uid=str(uid)
    auth=session_headers(f["session_a_token"], **{"x-openvietgis-csrf":f["csrf_a_token"]})
    other=session_headers(token, **{"x-openvietgis-csrf":make_csrf_token(token,baseline.DEFAULT_CSRF_SECRET)})
    foreign=session_headers(f["session_b_token"], **{"x-openvietgis-csrf":f["csrf_b_token"]})
    project=project_path(f["org_a_id"],f["project_a_id"])
    base=project+"/integration-hub/managed"
    def req(method,path,expected=200,headers=auth,**kwargs):
        r=c.request(method,path,headers=headers,timeout=40,**kwargs)
        assert r.status_code==expected,(method,path,r.status_code,r.text[:600])
        return r.json() if r.content else None
    a=req("POST",base+"/sessions",201,json={"device_label":"QA desktop A"})
    b=req("POST",base+"/sessions",201,other,json={"device_label":"QA desktop B"})
    edit=f["layers"]["edit_4326"]
    lid,sid=edit["layer_id"],edit["source_id"]
    def boot(session,headers=auth,**kw):
        return req("GET",base+"/bootstrap",headers=headers,params={"session_id":session,"layer_id":lid,**kw})
    first=boot(a["id"],limit=1)
    assert first["source_id"]==sid and first["content_revision"]
    assert first["has_more"] and first["next_offset"]==1
    following=boot(a["id"],limit=1,offset=1)
    assert following["content_revision"]==first["content_revision"]
    req("GET",base+"/bootstrap",404,other,params={"session_id":a["id"],"layer_id":lid})
    req("GET",base+"/bootstrap",404,foreign,params={"session_id":a["id"],"layer_id":lid})
    start=boot(b["id"],other,limit=1)["cursor"]
    body={"session_id":a["id"],"source_id":sid,"base_content_revision":first["content_revision"],"base_schema_revision":first["schema_revision"],"operation_id":"spec065-r029-"+str(uuid4()),"operations":[{"op_type":"create","feature_id":"spec065-r029-"+uuid4().hex,"geometry":{"type":"Point","coordinates":[105.9,21.1]},"properties":{"code":"R029","ward":"W1","metric_val":1,"active":True}}]}
    receipt=req("POST",base+"/commits",201,json=body)
    assert receipt["event_cursor"]>start
    assert req("POST",base+"/commits",201,json=body)==receipt
    assert req("GET",base+"/commits/"+body["operation_id"])==receipt
    req("GET",base+"/commits/"+body["operation_id"],404,other)
    req("POST",base+"/commits",409,json=body|{"operation_id":"spec065-stale-"+uuid4().hex})
    delta=req("GET",base+"/delta",headers=other,params={"session_id":b["id"],"cursor":start,"limit":1})
    assert not delta["reset_required"] and delta["next_cursor"]>=receipt["event_cursor"]
    assert any(x["source_id"]==sid and any(y["feature_id"]==body["operations"][0]["feature_id"] for y in x["upserts"]) for x in delta["changes"]),delta
    quiet=req("GET",base+"/delta",headers=other,params={"session_id":b["id"],"cursor":delta["next_cursor"]})
    assert not quiet["changes"]
    reset=req("GET",base+"/delta",headers=other,params={"session_id":b["id"],"cursor":delta["next_cursor"]+100})
    assert reset["reset_required"]
    commands=base+"/desktop-sessions/"+a["id"]+"/commands"
    command=req("POST",commands,201,json={"tool_name":"zoom_to_extent","arguments":{"bbox":[1,2,3,4]}})
    req("POST",commands,403,json={"tool_name":"run_python"})
    req("POST",commands+"/"+command["id"]+"/respond",json={"status":"completed","result":{"ok":True}})
    req("POST",commands+"/"+command["id"]+"/respond",409,json={"status":"failed"})
    req("DELETE",base+"/sessions/"+a["id"])
    req("GET",base+"/delta",403,params={"session_id":a["id"],"cursor":start})
    print("PASS actual R029 two actor sessions, bootstrap, immutable receipt, delta recovery/reset, allowed commands, revoke",flush=True)
    chat=project+"/collaboration"
    public=req("POST",chat+"/messages",201,json={"body":"Kiểm tra dự án"})
    private=req("POST",chat+"/messages",201,json={"body":"Riêng tư","recipient_id":uid})
    assert public["id"] in {r["id"] for r in req("GET",chat+"/messages",headers=other)["items"]}
    assert private["id"] not in {r["id"] for r in req("GET",chat+"/messages",headers=other)["items"]}
    assert private["id"] in {r["id"] for r in req("GET",chat+"/messages",headers=other,params={"peer":f["user_a_id"]})["items"]}
    req("GET",chat+"/messages",404,foreign)
    task=req("POST",chat+"/tasks",201,json={"title":"Kiểm tra bản đồ","assignee_id":uid,"priority":"high","source_message_id":public["id"]})
    req("POST",chat+"/tasks",422,json={"title":"Không công khai","assignee_id":uid,"source_message_id":private["id"]})
    assert task["id"] in {r["id"] for r in req("GET",chat+"/tasks",headers=other,params={"mine":"true"})["items"]}
    updated=req("PATCH",chat+"/tasks/"+task["id"],headers=other,json={"status":"in_progress","revision":1})
    assert updated["revision"]==2 and updated["status"]=="in_progress"
    req("PATCH",chat+"/tasks/"+task["id"],409,other,json={"status":"done","revision":1})
    assert task["id"] in {r["id"] for r in req("GET",chat+"/notifications",headers=other)["items"]}
    req("POST",chat+"/notifications/"+task["id"]+"/read",headers=other)
    print("PASS actual project chat private/public separation, task assignment, stale revision, notification read",flush=True)
    org="/api/v1/organizations/"+f["org_a_id"]+"/chat"
    rooms=req("GET",org+"/rooms")["items"]
    assert any(r["kind"]=="org" for r in rooms)
    group=req("POST",org+"/rooms",201,json={"name":"Tổ đo đạc","member_ids":[uid]})
    path=org+"/rooms/"+group["id"]
    msg=req("POST",path+"/messages",201,json={"body":"Kiểm tra @biên tập","mention_ids":[uid]})
    reply=req("POST",path+"/messages",201,other,json={"body":"Đã nhận","reply_to_id":msg["id"]})
    assert reply["reply_to"]["id"]==msg["id"]
    req("POST",org+"/messages/"+msg["id"]+"/reactions",headers=other,json={"emoji":"👍"})
    req("POST",org+"/messages/"+msg["id"]+"/pin")
    assert msg["id"] in {r["id"] for r in req("GET",path+"/pins",headers=other)["items"]}
    req("GET",path+"/messages",404,foreign)
    req("POST",path+"/read",headers=other)
    req("DELETE",path+"/members/"+uid,204)
    req("GET",path+"/messages",404,other)
    print("PASS actual org group chat, mention/reply/reaction/pin, read and membership revocation",flush=True)
