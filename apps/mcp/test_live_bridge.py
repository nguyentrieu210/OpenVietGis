import asyncio
import sys
from collections import deque

import httpx
import pytest
from mcp import Client
from mcp.client.stdio import StdioServerParameters
from starlette.testclient import TestClient

from bridge_http import create_app
from bridge_client import BridgeClient, BridgeSettings
from live_bridge import BridgeStore, BridgeError, clean_result, command_params
import live_server


def fixture():
    now = [100.0]
    store = BridgeStore(clock=lambda: now[0])
    session = store.create()
    return now, store, session["session_id"], session["browser_token"], session["ai_token"]


def test_roles_and_lifecycle():
    now, store, sid, browser, ai = fixture()
    with pytest.raises(BridgeError):
        store.poll(sid, ai)
    with pytest.raises(BridgeError):
        store.submit(sid, browser, "map_view", {})
    cid = store.submit(sid, ai, "map_view", {})["command_id"]
    assert store.result(sid, ai, cid)["state"] == "queued"
    assert store.poll(sid, browser)["command"]["id"] == cid
    assert store.poll(sid, browser)["command"] is None
    assert store.ping(sid, browser)["alive"]
    with pytest.raises(BridgeError):
        store.ack(sid, ai, cid, True, {})
    store.ack(sid, browser, cid, False, None)
    assert store.result(sid, ai, cid)["state"] == "denied"
    with pytest.raises(BridgeError):
        store.result(sid, ai, cid)
    store.close(sid, browser)
    with pytest.raises(BridgeError):
        store.submit(sid, ai, "map_view", {})


def test_isolation_validation_and_expiry():
    now, store, sid, browser, ai = fixture()
    other = store.create()
    with pytest.raises(BridgeError):
        store.submit(other["session_id"], ai, "map_view", {})
    for params in [{"longitude":999,"latitude":0,"zoom":5}, {"longitude":0,"latitude":0,"zoom":True}]:
        with pytest.raises(BridgeError):
            store.submit(sid, ai, "map_navigate", params)
    with pytest.raises(BridgeError):
        store.submit(sid, ai, "delete_layer", {})
    cid = store.submit(sid, ai, "map_view", {})["command_id"]
    store.poll(sid,browser)
    for _ in range(5):
        now[0] += 9
        store.ping(sid,browser)
    assert store.result(sid,ai,cid)["state"] == "expired"
    now[0] += 16
    with pytest.raises(BridgeError):
        store.poll(sid,browser)


def test_selection_sanitization():
    raw={"crs":"EPSG:4326","geometry":{"type":"Point","coordinates":[106.6,10.6],"secret":"x"},"attributes":{"private":"x"}}
    assert clean_result("map_selection",raw)=={"crs":"EPSG:4326","geometry":{"type":"Point","coordinates":[106.6,10.6]}}
    with pytest.raises(BridgeError):
        clean_result("map_selection",{"crs":"EPSG:4326","geometry":{"type":"Point","coordinates":[0,0,0]}})


def test_http_browser_and_ai():
    client=TestClient(create_app())
    assert client.post("/v1/sessions",json={}).status_code==403
    session=client.post("/v1/sessions",json={},headers={"Origin":"null"}).json()
    base="/v1/sessions/"+session["session_id"]
    ai={"Authorization":"Bearer "+session["ai_token"]}
    browser={"Authorization":"Bearer "+session["browser_token"]}
    assert client.post(base+"/commands",headers=browser,json={"method":"map_view","params":{}}).status_code==404
    cid=client.post(base+"/commands",headers=ai,json={"method":"map_navigate","params":{"longitude":106.6,"latitude":10.6,"zoom":15}}).json()["command_id"]
    assert client.post(base+"/poll",headers=browser,json={}).json()["command"]["id"]==cid
    assert client.post(base+"/commands/"+cid+"/ack",headers=browser,json={"ok":True,"result":None}).status_code==200
    assert client.get(base+"/commands/"+cid+"/result",headers=ai).json()=={"state":"done","result":None}
    assert client.post(base+"/close",headers=browser).status_code==200
    assert client.post(base+"/poll",headers=browser,json={}).status_code==404


def test_bridge_client_and_mcp(monkeypatch):
    settings=BridgeSettings("https://bridge.example/v1","s"*24,"a"*43)
    replies=deque([httpx.Response(201,json={"command_id":"c"*24}),httpx.Response(200,json={"state":"queued"}),httpx.Response(200,json={"state":"done","result":{"crs":"EPSG:4326"}})])
    def handle(req):
        assert req.headers["Authorization"]=="Bearer "+"a"*43
        assert req.url.host=="bridge.example"
        return replies.popleft()
    monkeypatch.setattr(live_server,"bridge",lambda:BridgeClient(settings,httpx.MockTransport(handle),sleep=lambda _:None))
    async def run():
        async with Client(live_server.mcp) as client:
            assert {t.name for t in (await client.list_tools()).tools}=={"gis_live_view","gis_live_selection","gis_live_navigate"}
            result=await client.call_tool("gis_live_view")
            assert not result.is_error
            assert result.structured_content["acknowledged"] is True
    asyncio.run(run())
    with pytest.raises(ValueError):
        BridgeSettings("http://bridge.example/v1","s"*24,"a"*43)


def test_live_stdio_discovery():
    async def run():
        async with Client(StdioServerParameters(command=sys.executable,args=[live_server.__file__])) as client:
            assert len((await client.list_tools()).tools)==3
    asyncio.run(run())
