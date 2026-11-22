import time
from concurrent.futures import ThreadPoolExecutor
import pytest
from starlette.testclient import TestClient
from live_bridge import BridgeStore,BridgeError
from workspace_tools import validate,METHODS
from remote_workspace import OwnedBridge
from remote_server import create_remote
from test_remote import CONFIG,VERIFIER,FakeIdentity,signed,ORIGIN,ORG,PROJECT

LAYER="00000000-0000-4000-8000-000000000004"
OP="00000000-0000-4000-8000-000000000005"
PIN={"layer_id":LAYER,"content_revision":"a"*64,"schema_revision":"b"*64}
EDIT={**PIN,"operation_id":OP,"summary":"Draw point","operations":[{"op_type":"create","feature_id":"point_1","geometry":{"type":"Point","coordinates":[106,11]},"properties":{}}]}

def test_changeset_validation_and_legacy_isolation():
    assert validate("gis_changeset",EDIT)["operations"][0]["geometry"]["coordinates"]==[106,11]
    for bad in [EDIT|{"project_id":PROJECT},EDIT|{"content_revision":"latest"},EDIT|{"operations":[{"op_type":"delete","feature_id":"y","properties":{"x":1}}]},EDIT|{"operations":[EDIT["operations"][0],EDIT["operations"][0]]},EDIT|{"operations":[{"op_type":"create","feature_id":"x","geometry":{"type":"Point","coordinates":[999,0]},"properties":{}}]}]:
        with pytest.raises(BridgeError):validate("gis_changeset",bad)
    s=BridgeStore();d=s.create()
    with pytest.raises(BridgeError):s.submit(d["session_id"],d["ai_token"],"gis_changeset",EDIT)

def test_old_browser_cannot_receive_workspace_mutations():
    import asyncio
    from remote_workspace import Owner
    store=OwnedBridge(BridgeStore(allow_workspace=True))
    store.create(Owner(ORG,"alice",ORG,PROJECT))
    with pytest.raises(BridgeError,match="Update the GIS tab"):
        asyncio.run(store.execute("alice","gis_changeset",EDIT,time.time()+60))

def test_full_tool_discovery_and_approved_changeset_relay():
    store=OwnedBridge(BridgeStore(allow_workspace=True))
    browser={"Origin":ORIGIN,"X-OVG-AI":"1","Cookie":"ovg_session=alice-cookie"}
    rpc_headers={"Accept":"application/json, text/event-stream","MCP-Protocol-Version":"2026-07-28","Authorization":"Bearer "+signed()}
    meta={"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}
    with TestClient(create_remote(CONFIG,identity=FakeIdentity(),workspace=store,verifier=VERIFIER,full_tools=True),base_url=ORIGIN) as client:
        listed=client.post("/mcp",json={"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":meta}},headers=rpc_headers|{"Mcp-Method":"tools/list"})
        assert listed.status_code==200,listed.text
        tools={t["name"]:t for t in listed.json()["result"]["tools"]}
        assert set(tools)==METHODS|{"gis_live_view","gis_live_selection","gis_live_navigate","gis_operation_status"}
        assert tools["gis_changeset"]["annotations"]["destructiveHint"] is True
        s=client.post("/ai-copilot/sessions",headers=browser,json={"organization_id":ORG,"project_id":PROJECT,"workspace_version":1}).json()
        base="/ai-copilot/sessions/"+s["session_id"]; headers=browser|{"X-OVG-Bridge":s["browser_token"]}
        rpc={"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"_meta":meta,"name":"gis_changeset","arguments":{"request":EDIT}}}
        with ThreadPoolExecutor(max_workers=1) as pool:
            future=pool.submit(client.post,"/mcp",json=rpc,headers=rpc_headers|{"Mcp-Method":"tools/call","Mcp-Name":"gis_changeset"})
            cmd=None
            for i in range(40):
                cmd=client.post(base+"/poll",headers=headers).json()["command"]
                if cmd:break
                time.sleep(.1)
            assert cmd and cmd["method"]=="gis_changeset" and cmd["params"]["operation_id"]==OP
            assert not future.done()
            assert client.post(base+"/commands/"+cmd["id"]+"/prepare",headers=headers).json()["authorized"]
            assert client.post(base+"/commands/"+cmd["id"]+"/ack",headers=headers,json={"ok":True,"result":{"id":OP}}).status_code==200
            r=future.result(timeout=10)
            assert r.json()["result"]["structuredContent"]=={"acknowledged":True,"result":{"id":OP}}
