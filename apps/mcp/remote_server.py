"""OAuth-protected Streamable HTTP MCP candidate; run only with approved IdP configuration."""
from __future__ import annotations
import os
from typing import Any
from urllib.parse import urlsplit

from mcp.server import MCPServer
from mcp.server.auth.middleware.auth_context import get_access_token
from mcp.server.auth.settings import AuthSettings
from mcp.server.transport_security import TransportSecuritySettings
from mcp.types import ToolAnnotations
from pydantic import AnyHttpUrl
from starlette.requests import Request
from starlette.responses import JSONResponse
from starlette.routing import Route
from starlette.middleware.base import BaseHTTPMiddleware

from bridge_http import body
from live_bridge import BridgeError
from remote_oauth import KeycloakVerifier,OidcConfig,SCOPE
from remote_workspace import GisIdentity,OwnedBridge

def create_remote(config, identity=None, workspace=None, verifier=None):
    origin=f"{urlsplit(config.resource).scheme}://{urlsplit(config.resource).netloc}"
    identity=identity or GisIdentity(config.issuer)
    workspace=workspace or OwnedBridge()
    verifier=verifier or KeycloakVerifier(config)
    mcp=MCPServer("OpenVietGIS Live",token_verifier=verifier,auth=AuthSettings(
        issuer_url=AnyHttpUrl(config.issuer),resource_server_url=AnyHttpUrl(config.resource),
        required_scopes=[SCOPE],validate_token_resource=True))

    async def operate(method,params):
        token=get_access_token()
        if token is None or not token.subject or not token.expires_at:
            raise ValueError("OAuth authentication required")
        try:
            return await workspace.execute(token.subject,method,params,token.expires_at)
        except BridgeError as e:
            raise ValueError(str(e)) from None

    @mcp.tool(annotations=ToolAnnotations(read_only_hint=True,destructive_hint=False),structured_output=True)
    async def gis_live_view()->dict[str,Any]:
        """Read the connected live GIS viewport after user approval."""
        return await operate("map_view",{})

    @mcp.tool(annotations=ToolAnnotations(read_only_hint=True,destructive_hint=False),structured_output=True)
    async def gis_live_selection()->dict[str,Any]:
        """Read the current selected geometry, WGS84, after GIS user approval."""
        return await operate("map_selection",{})

    @mcp.tool(annotations=ToolAnnotations(read_only_hint=False,destructive_hint=False),structured_output=True)
    async def gis_live_navigate(longitude:float,latitude:float,zoom:float)->dict[str,Any]:
        """Move the connected live camera with approval. No persistent changes."""
        return await operate("map_navigate",{"longitude":longitude,"latitude":latitude,"zoom":zoom})

    def browser_guard(request):
        if request.headers.get("origin")!=origin or request.headers.get("x-ovg-ai")!="1":
            raise BridgeError(403,"Trusted GIS origin required")
        return request.cookies.get("ovg_session")

    def pass_key(request):
        key=request.headers.get("x-ovg-bridge","")
        if not 40<=len(key)<=64:
            raise BridgeError(404,"Workspace not connected")
        return key

    async def open_session(request):
        cookie=browser_guard(request)
        data=await body(request)
        owner=await identity.owner(cookie,data.get("organization_id"),data.get("project_id"))
        return JSONResponse(workspace.create(owner),status_code=201)

    async def current(request):
        cookie=browser_guard(request)
        sid=request.path_params["sid"]
        key=pass_key(request)
        stored=workspace.owner_for_browser(sid,key)
        # Current cookie, project authorization and identity are verified every browser request.
        owner=await identity.owner(cookie,stored.organization_id,stored.project_id)
        workspace.owned_by(sid,owner,key)
        return sid,key

    async def poll(request):
        sid,key=await current(request)
        return JSONResponse(workspace.store.poll(sid,key))

    async def ping(request):
        sid,key=await current(request)
        return JSONResponse(workspace.store.ping(sid,key))

    async def prepare(request):
        sid,key=await current(request)
        return JSONResponse(workspace.store.check_claimed(sid,key,request.path_params["cid"]))

    async def ack(request):
        sid,key=await current(request)
        data=await body(request)
        return JSONResponse(workspace.store.ack(sid,key,request.path_params["cid"],data.get("ok"),data.get("result")))

    async def close(request):
        sid,key=await current(request)
        return JSONResponse(workspace.store.close(sid,key))

    app=mcp.streamable_http_app(stateless_http=True,json_response=True,max_request_body_size=128000,
        transport_security=TransportSecuritySettings(enable_dns_rebinding_protection=True,
          allowed_hosts=[urlsplit(config.resource).netloc],allowed_origins=[origin]))
    app.router.routes.extend([
        Route("/ai-copilot/sessions",open_session,methods=["POST"]),
        Route("/ai-copilot/sessions/{sid}/poll",poll,methods=["POST"]),
        Route("/ai-copilot/sessions/{sid}/ping",ping,methods=["POST"]),
        Route("/ai-copilot/sessions/{sid}/commands/{cid}/prepare",prepare,methods=["POST"]),
        Route("/ai-copilot/sessions/{sid}/commands/{cid}/ack",ack,methods=["POST"]),
        Route("/ai-copilot/sessions/{sid}/close",close,methods=["POST"]),
    ])
    async def handler(request,call_next):
        try:
            response=await call_next(request)
        except BridgeError as e:
            response=JSONResponse({"error":str(e)},status_code=e.status)
        response.headers["Cache-Control"]="no-store"
        return response
    app.add_middleware(BaseHTTPMiddleware,dispatch=handler)
    return app


def create_from_env():
    config=OidcConfig(os.environ["OVG_OIDC_ISSUER"],os.environ["OVG_MCP_RESOURCE"],
        frozenset(c.strip() for c in os.environ["OVG_MCP_OAUTH_CLIENTS"].split(",") if c.strip()))
    return create_remote(config,verifier=KeycloakVerifier(config,jwks_url=os.environ.get("OVG_OIDC_JWKS_URL")))
