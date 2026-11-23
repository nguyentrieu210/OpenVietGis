"""Local AI-facing relay client. Only an explicitly paired live map tab."""
from __future__ import annotations
import os
import re
import time
from dataclasses import dataclass, field
from urllib.parse import urlsplit

import httpx

from live_bridge import command_params

ID = re.compile(r"^[A-Za-z0-9_-]{20,64}$")
SECRET = re.compile(r"^[A-Za-z0-9_-]{40,64}$")


@dataclass(frozen=True)
class BridgeSettings:
    base: str
    session: str
    token: str = field(repr=False)
    allow_loopback: bool = False

    def __post_init__(self):
        p = urlsplit(self.base)
        loopback = self.allow_loopback and p.scheme == "http" and p.hostname == "127.0.0.1"
        if (not (p.scheme == "https" or loopback) or not p.hostname or p.username
                or p.password or p.query or p.fragment or p.path.rstrip("/") != "/v1"):
            raise ValueError("Expected trusted HTTPS live bridge URL")
        if not ID.fullmatch(self.session) or not SECRET.fullmatch(self.token):
            raise ValueError("Invalid pairing information")

    @classmethod
    def from_env(cls):
        return cls(os.environ["OVG_BRIDGE_BASE"], os.environ["OVG_BRIDGE_SESSION"],
                   os.environ["OVG_BRIDGE_TOKEN"], os.environ.get("OVG_BRIDGE_DEV") == "1")


class BridgeClient:
    def __init__(self, settings, transport=None, clock=time.monotonic, sleep=time.sleep):
        self.settings = settings
        self.transport = transport
        self.clock = clock
        self.sleep = sleep

    def run(self, method, params):
        params = command_params(method, params)
        endpoint = f"{self.settings.base.rstrip('/')}/sessions/{self.settings.session}/commands"
        try:
            with httpx.Client(transport=self.transport, timeout=5, trust_env=False, follow_redirects=False,
                              headers={"Authorization": "Bearer " + self.settings.token}) as client:
                response = client.post(endpoint, json={"method": method, "params": params})
                if response.status_code != 201:
                    raise ValueError(f"Live bridge unavailable (HTTP {response.status_code})")
                cid = response.json().get("command_id")
                if not isinstance(cid, str) or not ID.fullmatch(cid):
                    raise ValueError("Invalid live bridge response")
                deadline = self.clock() + 43
                while self.clock() < deadline:
                    resp = client.get(endpoint + "/" + cid + "/result")
                    if resp.status_code != 200:
                        raise ValueError(f"Live session unavailable (HTTP {resp.status_code})")
                    data = resp.json()
                    if data.get("state") == "done":
                        return {"acknowledged": True, "result": data.get("result")}
                    if data.get("state") in ("expired", "denied"):
                        raise ValueError("GIS user declined the command or it expired")
                    if data.get("state") not in ("queued", "claimed"):
                        raise ValueError("Invalid live bridge status")
                    self.sleep(.6)
        except (httpx.HTTPError, KeyError, TypeError):
            raise ValueError("Live bridge request failed") from None
        raise ValueError("No live GIS acknowledgement received")
