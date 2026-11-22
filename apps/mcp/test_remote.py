import asyncio
import time
from concurrent.futures import ThreadPoolExecutor
from uuid import UUID
import jwt
import httpx
import pytest
from cryptography.hazmat.primitives.asymmetric import rsa
from starlette.testclient import TestClient

from remote_oauth import OidcConfig,KeycloakVerifier
from remote_workspace import GisIdentity,Owner,OwnedBridge
from remote_server import create_remote
from live_bridge import BridgeError

ORIGIN="https://openvietgis.cloud"
ISSUER=ORIGIN+"/realms/openvietgis"
RESOURCE=ORIGIN+"/mcp"
CONFIG=OidcConfig(ISSUER,RESOURCE,frozenset({"approved-mcp"}))
ORG="00000000-0000-4000-8000-000000000001"
PROJECT="00000000-0000-4000-8000-000000000002"
USER="00000000-0000-4000-8000-000000000003"
KEY=rsa.generate_private_key(public_exponent=65537,key_size=2048)
VERIFIER=KeycloakVerifier(CONFIG,signing_key=lambda _:KEY.public_key())

def signed(**overrides):
    payload={"iss":ISSUER,"aud":RESOURCE,"sub":"alice","azp":"approved-mcp","scope":"openid gis:live",
        "typ":"Bearer","iat":int(time.time()),"exp":int(time.time())+300}
    return jwt.encode(payload|overrides,KEY,algorithm="RS256",headers={"kid":"fixture"})

@pytest.mark.parametrize("overrides",[{"aud":"another-api"},{"iss":"https://evil.example"},{"azp":"unapproved"},{"scope":"openid"},{"typ":"ID"},{"exp":1}])
def test_reject_wrong_claims(overrides):
    assert asyncio.run(VERIFIER.verify_token(signed(**overrides))) is None

def test_reject_invalid_signature_and_accept():
    other=rsa.generate_private_key(public_exponent=65537,key_size=2048)
    claims=jwt.decode(signed(),options={"verify_signature":False})
    forged=jwt.encode(claims,other,algorithm="RS256",headers={"kid":"fixture"})
    assert asyncio.run(VERIFIER.verify_token(forged)) is None
    assert asyncio.run(VERIFIER.verify_token(signed())).subject=="alice"

def test_same_origin_identity_and_restricted_scope():
    def handle(req):
        assert "ovg_session=valid-cookie" in req.headers["cookie"]
        if req.url.path.endswith("/me"):
            return httpx.Response(200,json={"id":USER,"status":"active"})
        if req.url.path.endswith("/identity-links"):
            return httpx.Response(200,json=[{"issuer":ISSUER,"subject":"alice"}])
        assert req.url.path.endswith("/access/mine")
        return httpx.Response(200,json={"restricted":False})
    identity=GisIdentity(ISSUER,transport=httpx.MockTransport(handle))
    assert asyncio.run(identity.owner("valid-cookie",ORG,PROJECT))==Owner(USER,"alice",ORG,PROJECT)
    with pytest.raises(BridgeError):
        asyncio.run(identity.owner("",ORG,PROJECT))

class FakeIdentity:
    async def owner(self,cookie,org,project):
        if cookie!="alice-cookie":
            raise BridgeError(401,"Sign in")
        if (org,project)!=(ORG,PROJECT):
            raise BridgeError(403,"Invalid scope")
        return Owner(USER,"alice",org,project)

def test_remote_http_mcp_and_workspace():
    store=OwnedBridge()
    browser={"Origin":ORIGIN,"X-OVG-AI":"1","Cookie":"ovg_session=alice-cookie"}
    rpc_headers={"Accept":"application/json, text/event-stream","MCP-Protocol-Version":"2026-07-28",
        "Authorization":"Bearer "+signed()}
    with TestClient(create_remote(CONFIG,identity=FakeIdentity(),workspace=store,verifier=VERIFIER),base_url=ORIGIN) as client:
        response=client.get("/.well-known/oauth-protected-resource/mcp")
        assert response.status_code==200,response.text
        assert response.json()["resource"]==RESOURCE
        meta={"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}
        rpc={"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":meta}}
        assert client.post("/mcp",json=rpc,headers={"Accept":"application/json, text/event-stream","Mcp-Method":"tools/list"}).status_code==401
        listed=client.post("/mcp",json=rpc,headers=rpc_headers|{"Mcp-Method":"tools/list"})
        assert listed.status_code==200,listed.text
        assert len(listed.json()["result"]["tools"])==3
        legacy={"jsonrpc":"2.0","id":3,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}}}
        init=client.post("/mcp",json=legacy,headers={"Accept":"application/json, text/event-stream","Authorization":"Bearer "+signed()})
        assert init.status_code==200,init.text
        assert init.json()["result"]["protocolVersion"]=="2025-11-25"
        assert client.post("/ai-copilot/sessions",json={"organization_id":ORG,"project_id":PROJECT}).status_code==403
        session=client.post("/ai-copilot/sessions",headers=browser,json={"organization_id":ORG,"project_id":PROJECT})
        assert session.status_code==201,session.text
        data=session.json()
        assert "ai_token" not in data
        sid,key=data["session_id"],data["browser_token"]
        bheaders=browser|{"X-OVG-Bridge":key}
        base="/ai-copilot/sessions/"+sid
        assert client.post(base+"/poll",headers=bheaders|{"Cookie":"ovg_session=bob-cookie"}).status_code==401
        with pytest.raises(BridgeError):
            store.for_subject("bob")
        rpc={"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"_meta":meta,"name":"gis_live_navigate","arguments":{"longitude":105.8,"latitude":21,"zoom":12}}}
        with ThreadPoolExecutor(max_workers=1) as pool:
            future=pool.submit(client.post,"/mcp",json=rpc,headers=rpc_headers|{"Mcp-Method":"tools/call","Mcp-Name":"gis_live_navigate"})
            cmd=None
            for _ in range(20):
                answer=client.post(base+"/poll",headers=bheaders)
                assert answer.status_code==200,answer.text
                cmd=answer.json()["command"]
                if cmd:break
                time.sleep(.1)
            assert cmd and cmd["method"]=="map_navigate"
            assert client.post(base+"/commands/"+cmd["id"]+"/prepare",headers=bheaders).json()["authorized"] is True
            response=client.post(base+"/commands/"+cmd["id"]+"/ack",headers=bheaders,json={"ok":True,"result":None})
            assert response.status_code==200,response.text
            result=future.result(timeout=10)
            assert result.status_code==200,result.text
            assert result.json()["result"]["structuredContent"]["acknowledged"] is True
        assert client.post(base+"/close",headers=bheaders).status_code==200
        with pytest.raises(BridgeError):store.for_subject("alice")
