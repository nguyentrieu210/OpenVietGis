"""Pure-stdlib HTTP client for R029 Managed Realtime GIS Client.

Zero PyQt/QGIS imports so it can be unit tested in any standard Python environment.
"""

from __future__ import annotations

import json
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any

from .client import (
    DEFAULT_SYNC_TIMEOUT_SECONDS,
    USER_AGENT,
    ApiError,
    ConfigurationError,
    OpenVietGisConfig,
    _extract_http_error_detail,
    _redact_secret,
)


@dataclass(frozen=True)
class ManagedSessionInfo:
    id: str
    organization_id: str
    project_id: str
    state: str
    permission_epoch: int
    last_ack_cursor: int
    current_cursor: int
    expires_at: str
    can_edit: bool
    can_delete: bool


@dataclass(frozen=True)
class ManagedBootstrapPage:
    layer_id: str
    source_id: str
    name: str
    geometry_family: str
    dimensions: str
    crs_code: int | None
    content_revision: str
    schema_revision: str
    cursor: int
    permission_epoch: int
    fields: tuple[dict[str, Any], ...]
    features: tuple[dict[str, Any], ...]
    has_more: bool
    next_offset: int | None


@dataclass(frozen=True)
class ManagedDeltaPage:
    reset_required: bool
    next_cursor: int
    permission_epoch: int
    has_more: bool
    changes: tuple[dict[str, Any], ...]
    reason: str | None = None


@dataclass(frozen=True)
class ManagedCommitReceipt:
    status: str
    changeset_id: str
    operation_id: str
    source_id: str
    resulting_content_revision: str
    operation_count: int
    event_cursor: int
    audit_event_id: str


def _managed_base(config: OpenVietGisConfig) -> str:
    if not isinstance(config, OpenVietGisConfig):
        raise ConfigurationError("config must be an OpenVietGisConfig instance.")
    return (
        f"{config.base_url}/api/v1/organizations/{config.organization_id}"
        f"/projects/{config.project_id}/integration-hub/managed"
    )


def _json_request(
    config: OpenVietGisConfig,
    method: str,
    url: str,
    body: dict[str, Any] | None = None,
    timeout: float = DEFAULT_SYNC_TIMEOUT_SECONDS,
) -> dict[str, Any]:
    data = json.dumps(body).encode("utf-8") if body is not None else None
    headers = {
        "Authorization": f"Bearer {config.bearer_token}",
        "Accept": "application/json",
        "User-Agent": USER_AGENT,
    }
    if data is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read()
    except urllib.error.HTTPError as exc:
        detail = _extract_http_error_detail(exc, config.bearer_token)
        raise ApiError(f"HTTP {exc.code}: {detail}", status_code=exc.code) from None
    except urllib.error.URLError as exc:
        reason = _redact_secret(str(exc.reason), config.bearer_token)
        raise ApiError(f"Network error: {reason}") from None
    except OSError as exc:
        reason = _redact_secret(str(exc), config.bearer_token)
        raise ApiError(f"Connection error: {reason}") from None
    try:
        parsed = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, ValueError) as exc:
        raise ApiError("Invalid JSON response from managed API.") from exc
    if not isinstance(parsed, dict):
        raise ApiError("Unexpected JSON structure from managed API.")
    return parsed


def build_managed_events_url(config: OpenVietGisConfig, session_id: str) -> str:
    qs = urllib.parse.urlencode({"session_id": session_id})
    return f"{_managed_base(config)}/events?{qs}"


def create_managed_session(
    config: OpenVietGisConfig,
    *,
    device_label: str = "QGIS Desktop",
    client_version: str = "0.9.0",
    binding_id: str | None = None,
) -> ManagedSessionInfo:
    payload: dict[str, Any] = {
        "device_label": device_label,
        "client_version": client_version,
        "mode": "managed",
    }
    if binding_id:
        payload["binding_id"] = binding_id
    res = _json_request(config, "POST", f"{_managed_base(config)}/sessions", payload)
    caps = dict(res.get("capabilities") or {})
    return ManagedSessionInfo(
        id=str(res["id"]),
        organization_id=str(res["organization_id"]),
        project_id=str(res["project_id"]),
        state=str(res["state"]),
        permission_epoch=int(res["permission_epoch"]),
        last_ack_cursor=int(res.get("last_ack_cursor", 0)),
        current_cursor=int(res.get("current_cursor", res.get("last_ack_cursor", 0))),
        expires_at=str(res["expires_at"]),
        can_edit=bool(caps.get("can_edit", False)),
        can_delete=bool(caps.get("can_delete", False)),
    )


def revoke_managed_session(config: OpenVietGisConfig, session_id: str) -> dict[str, Any]:
    return _json_request(config, "DELETE", f"{_managed_base(config)}/sessions/{session_id}")


def bootstrap_managed_layer(
    config: OpenVietGisConfig,
    *,
    layer_id: str,
    session_id: str,
    limit: int = 1000,
    offset: int = 0,
) -> ManagedBootstrapPage:
    qs = urllib.parse.urlencode(
        {"layer_id": layer_id, "session_id": session_id, "limit": limit, "offset": offset}
    )
    res = _json_request(config, "GET", f"{_managed_base(config)}/bootstrap?{qs}")
    return ManagedBootstrapPage(
        layer_id=str(res["layer_id"]),
        source_id=str(res["source_id"]),
        name=str(res["name"]),
        geometry_family=str(res["geometry_family"]),
        dimensions=str(res["dimensions"]),
        crs_code=int(res["crs_code"]) if res.get("crs_code") is not None else None,
        content_revision=str(res["content_revision"]),
        schema_revision=str(res["schema_revision"]),
        cursor=int(res["cursor"]),
        permission_epoch=int(res["permission_epoch"]),
        fields=tuple(res.get("fields") or ()),
        features=tuple(res.get("features") or ()),
        has_more=bool(res.get("has_more", False)),
        next_offset=int(res["next_offset"]) if res.get("next_offset") is not None else None,
    )


def fetch_managed_delta(
    config: OpenVietGisConfig,
    *,
    session_id: str,
    cursor: int,
    limit: int = 200,
) -> ManagedDeltaPage:
    qs = urllib.parse.urlencode({"session_id": session_id, "cursor": cursor, "limit": limit})
    res = _json_request(config, "GET", f"{_managed_base(config)}/delta?{qs}")
    return ManagedDeltaPage(
        reset_required=bool(res.get("reset_required", False)),
        next_cursor=int(res.get("next_cursor", cursor)),
        permission_epoch=int(res.get("permission_epoch", 1)),
        has_more=bool(res.get("has_more", False)),
        changes=tuple(res.get("changes") or ()),
        reason=str(res["reason"]) if res.get("reason") else None,
    )


def commit_managed_changes(
    config: OpenVietGisConfig,
    *,
    session_id: str,
    source_id: str,
    base_content_revision: str,
    base_schema_revision: str | None,
    operation_id: str,
    operations: list[dict[str, Any]],
    summary: str = "QGIS Managed Realtime Edit",
    allow_deletes: bool = False,
) -> ManagedCommitReceipt:
    payload: dict[str, Any] = {
        "session_id": session_id,
        "source_id": source_id,
        "base_content_revision": base_content_revision,
        "operation_id": operation_id,
        "summary": summary,
        "allow_deletes": allow_deletes,
        "operations": operations,
    }
    if base_schema_revision:
        payload["base_schema_revision"] = base_schema_revision
    res = _json_request(config, "POST", f"{_managed_base(config)}/commits", payload)
    return ManagedCommitReceipt(
        status=str(res["status"]),
        changeset_id=str(res["changeset_id"]),
        operation_id=str(res["operation_id"]),
        source_id=str(res["source_id"]),
        resulting_content_revision=str(res["resulting_content_revision"]),
        operation_count=int(res["operation_count"]),
        event_cursor=int(res.get("event_cursor", 0)),
        audit_event_id=str(res["audit_event_id"]),
    )


def get_managed_commit_receipt(
    config: OpenVietGisConfig,
    *,
    operation_id: str,
) -> ManagedCommitReceipt:
    encoded = urllib.parse.quote(operation_id, safe="")
    res = _json_request(config, "GET", f"{_managed_base(config)}/commits/{encoded}")
    return ManagedCommitReceipt(
        status=str(res["status"]),
        changeset_id=str(res["changeset_id"]),
        operation_id=str(res["operation_id"]),
        source_id=str(res["source_id"]),
        resulting_content_revision=str(res["resulting_content_revision"]),
        operation_count=int(res["operation_count"]),
        event_cursor=int(res.get("event_cursor", 0)),
        audit_event_id=str(res["audit_event_id"]),
    )


def list_desktop_commands(
    config: OpenVietGisConfig,
    *,
    session_id: str,
    status: str = "pending",
) -> list[dict[str, Any]]:
    qs = urllib.parse.urlencode({"status": status})
    res = _json_request(
        config, "GET", f"{_managed_base(config)}/desktop-sessions/{session_id}/commands?{qs}"
    )
    return list(res.get("items") or [])


def respond_desktop_command(
    config: OpenVietGisConfig,
    *,
    session_id: str,
    command_id: str,
    status: str,
    result: dict[str, Any] | None = None,
) -> dict[str, Any]:
    payload: dict[str, Any] = {"status": status}
    if result is not None:
        payload["result"] = result
    return _json_request(
        config,
        "POST",
        f"{_managed_base(config)}/desktop-sessions/{session_id}/commands/{command_id}/respond",
        payload,
    )
