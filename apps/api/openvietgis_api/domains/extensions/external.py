"""Metadata-only, isolated partner UI contract. No server-side URL fetching."""

from __future__ import annotations

import re
from typing import Any
from urllib.parse import urlsplit

from openvietgis_api.domains.platform_errors import PlatformValidationError

EXTERNAL_PREFIX = "partner_"
EXTERNAL_CAPABILITIES = frozenset(
    {
        "host_context",
        "host_notice",
        "map_view",
        "map_navigate",
        "map_selection",
        "map_overlay",
        "tool_register",
    }
)
_HOST = re.compile(r"(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}\Z")


def validate_external_manifest(
    *,
    extension_key: str,
    permissions: list[str],
    commands: list[dict[str, Any]],
    capabilities: list[str],
    entity_schemas: dict[str, dict[str, Any]],
    ui_contributions: list[dict[str, Any]],
) -> None:
    """Partner UI has no implicit Core/record permissions or backend runtime."""
    invalid = PlatformValidationError("invalid isolated partner plugin manifest")
    if (
        not extension_key.startswith(EXTERNAL_PREFIX)
        or len(extension_key) <= len(EXTERNAL_PREFIX)
        or permissions
        or commands
        or entity_schemas
        or not capabilities
        or len(set(capabilities)) != len(capabilities)
        or not set(capabilities) <= EXTERNAL_CAPABILITIES
        or len(ui_contributions) != 1
    ):
        raise invalid
    ui = ui_contributions[0]
    if not isinstance(ui, dict) or set(ui) != {
        "id",
        "slot",
        "label",
        "route_suffix",
        "runtime",
        "url",
        "sdk_version",
    }:
        raise invalid
    if (
        ui["id"] != "partner_panel"
        or ui["slot"] != "project_panel"
        or ui["runtime"] != "sandboxed_iframe"
        or ui["sdk_version"] != "1.0"
        or ui["route_suffix"] != "/partner"
        or not isinstance(ui["label"], str)
        or not ui["label"].strip()
        or len(ui["label"]) > 200
    ):
        raise invalid
    url = ui["url"]
    if not isinstance(url, str) or len(url) > 2048 or re.search(r"[\x00-\x20\x7f-\xff\\]", url):
        raise invalid
    try:
        parts = urlsplit(url)
        host = parts.hostname or ""
        if (
            parts.scheme != "https"
            or not _HOST.fullmatch(host)
            or parts.netloc != host
            or host == "openvietgis.cloud"
            or host.endswith((".openvietgis.cloud", ".local", ".localhost", ".internal", ".test"))
            or parts.query
            or parts.fragment
            or not parts.path.startswith("/")
            or "?" in url
            or "#" in url
        ):
            raise invalid
    except ValueError as exc:
        raise invalid from exc
