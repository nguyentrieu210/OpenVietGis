"""Same-origin GIS session validation and live-workspace ownership, no database access."""
from __future__ import annotations
import asyncio
import secrets
import time
from dataclasses import dataclass,field
from uuid import UUID

import httpx
from live_bridge import BridgeStore, BridgeError

@dataclass(frozen=True)
class Owner:
    user_id: str
    subject: str
    organization_id: str
    project_id: str


class GisIdentity:
    def __init__(self, issuer, base="http://127.0.0.1:8019/api/v1", transport=None):
        self.issuer=issuer
        self.base=base.rstrip("/")
        self.transport=transport
        if self.base != "http://127.0.0.1:8019/api/v1" and self.transport is None:
            raise ValueError("Core identity API must be the fixed local production service")

    async def owner(self, session_cookie, org, project):
        try:
            org,project=str(UUID(org)),str(UUID(project))
        except (ValueError,TypeError,AttributeError):
            raise BridgeError(422,"Invalid project") from None
        if not session_cookie or len(session_cookie)>1024:
            raise BridgeError(401,"Sign in to OpenVietGIS")
        try:
            async with httpx.AsyncClient(transport=self.transport,timeout=5,trust_env=False,
                    follow_redirects=False,cookies={"ovg_session":session_cookie}) as client:
                async def get(path):
                    r=await client.get(self.base+path)
                    if r.status_code != 200:
                        raise BridgeError(401 if r.status_code==401 else 403,"GIS permission unavailable")
                    return r.json()
                me,links,access=await asyncio.gather(get("/me"),get("/me/identity-links"),
                    get(f"/organizations/{org}/projects/{project}/access/mine"))
                if me.get("status")!="active" or access.get("restricted") is not False:
                    raise BridgeError(403,"Unrestricted project read access required")
                subjects={x["subject"] for x in links if x.get("issuer")==self.issuer and isinstance(x.get("subject"),str)}
                if len(subjects)!=1:
                    raise BridgeError(403,"Exactly one linked Keycloak identity required")
                return Owner(str(UUID(me["id"])),subjects.pop(),org,project)
        except (httpx.HTTPError,ValueError,KeyError,TypeError,AttributeError):
            raise BridgeError(403,"GIS identity verification failed") from None


@dataclass
class OwnedSession:
    owner: Owner
    ai_key: str = field(repr=False)
    full_workspace: int = 0
    last_receipt: tuple[str, dict] | None = field(default=None, repr=False)


class OwnedBridge:
    """At most one live tab per linked identity. Pair secrets never leave server."""
    def __init__(self, store=None):
        self.store=store or BridgeStore()
        self.owned:dict[str,OwnedSession]={}

    def _prune(self):
        self.store._purge()
        for sid in list(self.owned):
            if sid not in self.store.sessions:
                del self.owned[sid]

    def create(self, owner, full_workspace=False):
        with self.store.lock:
            self._prune()
            # A fresh explicit connection replaces the old tab for this identity.
            for sid,record in list(self.owned.items()):
                if record.owner.subject==owner.subject:
                    del self.owned[sid]
                    del self.store.sessions[sid]
            data=self.store.create()
            self.owned[data["session_id"]]=OwnedSession(owner,data.pop("ai_token"),full_workspace)
            return data

    def owned_by(self,sid,owner,browser_token):
        with self.store.lock:
            self._prune()
            record=self.owned.get(sid)
            if not record or record.owner!=owner:
                raise BridgeError(404,"Workspace not connected")
            self.store._get(sid,browser_token,"browser")
            return record

    def owner_for_browser(self,sid,browser_token):
        with self.store.lock:
            self._prune()
            self.store._get(sid,browser_token,"browser")
            record=self.owned.get(sid)
            if not record:
                raise BridgeError(404,"Workspace not connected")
            return record.owner

    def for_subject(self,subject):
        with self.store.lock:
            self._prune()
            matches=[(sid,r.ai_key) for sid,r in self.owned.items() if r.owner.subject==subject]
            if len(matches)!=1:
                raise BridgeError(404,"Open and connect your GIS workspace first")
            return matches[0]

    def operation_status(self,subject,ticket):
        from advanced_tools import LONG
        sid,key=self.for_subject(subject)
        with self.store.lock:
            session=self.store._get(sid,key,"ai")
            record=self.owned[sid]
            if record.last_receipt and record.last_receipt[0]==ticket:
                return record.last_receipt[1]
            if not session.command or session.command.id!=ticket or session.command.method not in LONG:
                raise BridgeError(404,"Operation ticket unavailable in this workspace")
            result=self.store.result(sid,key,ticket)
            if result["state"] in {"done","denied","expired"}:
                record.last_receipt=(ticket,result)
            return result

    async def execute(self,subject,method,params,token_expiry):
        sid,key=self.for_subject(subject)
        if token_expiry<=time.time():
            raise BridgeError(401,"OAuth token expired")
        with self.store.lock:
            from workspace_tools import METHODS as WORKSPACE_METHODS
            if method in WORKSPACE_METHODS and (sid not in self.owned or not self.owned[sid].full_workspace):
                raise BridgeError(409,"Update the GIS tab to connect full workspace tools")
            from advanced_tools import TOOLS,LONG
            if method in TOOLS and self.owned[sid].full_workspace<2:
                raise BridgeError(409,"Update the GIS tab to connect advanced workspace tools")
            cid=self.store.submit(sid,key,method,params)["command_id"]
            c=self.store.sessions[sid].command
            c.expires=min(c.expires,self.store.clock()+max(0,token_expiry-time.time()))
            if method in LONG:
                return {"state":"queued","operation_ticket":cid,"expires_in":max(0,round(c.expires-self.store.clock())),"next":"Wait for GIS approval, then call gis_operation_status. Do not resubmit while pending."}
        deadline=time.monotonic()+43
        try:
            while time.monotonic()<deadline:
                if time.time()>=token_expiry:
                    raise BridgeError(401,"OAuth token expired")
                result=self.store.result(sid,key,cid)
                if result["state"]=="done":
                    return {"acknowledged":True,"result":result["result"]}
                if result["state"] in ("denied","expired"):
                    raise BridgeError(409,"GIS user declined or command expired")
                await asyncio.sleep(.5)
            raise BridgeError(409,"GIS acknowledgement timed out")
        finally:
            self.store.cancel(sid,key,cid)
