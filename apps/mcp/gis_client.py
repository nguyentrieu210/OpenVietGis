"""Scope-pinned GIS HTTP adapter; no arbitrary URL or mutation tool."""
from __future__ import annotations
import json
import os
import re
from dataclasses import dataclass, field
from urllib.parse import urlsplit
from uuid import UUID

import httpx

_MAX_BYTES = 2_000_000
_PIN = re.compile(r"^[a-f0-9]{64}$")
_FEATURE = re.compile(r"^[A-Za-z0-9_-]{1,128}$")


@dataclass(frozen=True)
class Settings:
    api_base: str
    organization_id: UUID
    project_id: UUID
    token: str = field(repr=False)

    def __post_init__(self):
        parsed = urlsplit(self.api_base)
        if (
            parsed.scheme != "https" or not parsed.hostname or parsed.username
            or parsed.password or parsed.query or parsed.fragment
            or parsed.path.rstrip("/") != "/api/v1"
        ):
            raise ValueError("Configure the HTTPS GIS /api/v1 endpoint")
        if not self.token or any(c.isspace() for c in self.token):
            raise ValueError("An explicit scoped GIS service credential is required")

    @classmethod
    def from_env(cls):
        return cls(
            os.environ.get("OVG_API_BASE", "https://openvietgis.cloud/api/v1"),
            UUID(os.environ["OVG_ORG_ID"]), UUID(os.environ["OVG_PROJECT_ID"]),
            os.environ["OVG_SERVICE_TOKEN"],
        )


class GisClient:
    def __init__(self, settings: Settings, transport=None):
        self.settings = settings
        self.transport = transport

    def _get(self, suffix="", params=None):
        url = (
            self.settings.api_base.rstrip("/")
            + f"/organizations/{self.settings.organization_id}/projects/{self.settings.project_id}"
            + suffix
        )
        try:
            with httpx.Client(
                transport=self.transport, timeout=15, follow_redirects=False, trust_env=False,
                headers={"Authorization": f"Bearer {self.settings.token}", "Accept": "application/json"},
            ) as client:
                with client.stream("GET", url, params=params) as response:
                    if response.status_code != 200:
                        raise ValueError(f"GIS request denied or failed (HTTP {response.status_code})")
                    data = bytearray()
                    for chunk in response.iter_bytes():
                        if len(data) + len(chunk) > _MAX_BYTES:
                            raise ValueError("GIS result too large; lower the requested limit")
                        data.extend(chunk)
                    result = json.loads(data)
        except (httpx.HTTPError, json.JSONDecodeError):
            raise ValueError("GIS request or response failed") from None
        if not isinstance(result, dict):
            raise ValueError("Unexpected GIS response")
        return result

    def project(self):
        return self._get()

    def map_state(self):
        return self._get("/map")

    def layers(self, limit=50, after_id=None):
        if not 1 <= limit <= 100:
            raise ValueError("limit must be 1–100")
        params = {"limit": limit}
        if after_id:
            params["after_id"] = str(UUID(str(after_id)))
        return self._get("/layers", params)

    def layer(self, layer_id):
        return self._get(f"/layers/{UUID(str(layer_id))}")

    def features(self, layer_id, content_revision, schema_revision, limit=20,
                 after_feature_id=None, include_geometry=False):
        if not 1 <= limit <= 100:
            raise ValueError("limit must be 1–100")
        if not (_PIN.fullmatch(content_revision) and _PIN.fullmatch(schema_revision)):
            raise ValueError("Both revision pins must be 64-character hexadecimal strings")
        params = {
            "content_revision": content_revision, "schema_revision": schema_revision,
            "limit": limit, "include_geometry": str(include_geometry).lower(),
            "response_crs": 4326,
        }
        if after_feature_id is not None:
            if not _FEATURE.fullmatch(after_feature_id):
                raise ValueError("Invalid feature cursor")
            params["after_feature_id"] = after_feature_id
        return self._get(f"/layers/{UUID(str(layer_id))}/features", params)
