"""Pure-stdlib HTTP client for R029 Managed Realtime GIS Client.

Zero PyQt/QGIS imports so it can be unit tested in any standard Python environment.
"""

from __future__ import annotations

import json
import math
import re
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
from .snapshot_cache import cache_path, read_cached_record, write_snapshot

MAX_MANAGED_RESPONSE_BYTES = 32 * 1024 * 1024
MAX_BOOTSTRAP_PAGE = 5_000
MAX_DELTA_PAGE = 500
MAX_MANAGED_FEATURES = 1_000_000
MAX_MANAGED_OPERATIONS = 1_000
MAX_MANAGED_LAYERS = 100


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
    cacheable: bool = False
    feature_count: int | None = None


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
            raw = resp.read(MAX_MANAGED_RESPONSE_BYTES + 1)
            if len(raw) > MAX_MANAGED_RESPONSE_BYTES:
                raise ApiError("Phản hồi managed API vượt quá 32 MiB.")
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
    client_version: str = "1.0.0",
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
    if type(limit) is not int or not 1 <= limit <= MAX_BOOTSTRAP_PAGE:
        raise ApiError("Giới hạn trang bootstrap phải từ 1 đến 5000.")
    if type(offset) is not int or offset < 0 or offset > MAX_MANAGED_FEATURES:
        raise ApiError("Offset bootstrap không hợp lệ.")
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
        cacheable=res.get("cacheable") is True,
        feature_count=int(res["feature_count"]) if res.get("feature_count") is not None else None,
    )


def load_managed_snapshot(
    config: OpenVietGisConfig, *, layer_id: str, session_id: str,
    permission_epoch: int, limit: int = 1000, cancelled=lambda: False,
) -> tuple[ManagedBootstrapPage, list[dict[str, Any]]]:
    """Load a coherent native snapshot before materializing any QGIS layer."""
    if type(limit) is not int or not 1 <= limit <= MAX_BOOTSTRAP_PAGE:
        raise ApiError("Giới hạn snapshot phải từ 1 đến 5000.")
    first = None
    offset = 0
    features: list[dict[str, Any]] = []
    ids: set[str] = set()
    while True:
        if cancelled():
            raise ApiError("Đã hủy tải lớp.")
        page = bootstrap_managed_layer(
            config, layer_id=layer_id, session_id=session_id, limit=limit, offset=offset,
        )
        if page.layer_id != layer_id or page.permission_epoch != permission_epoch:
            raise ApiError("Phạm vi hoặc quyền của lớp đã thay đổi; hãy mở lại.")
        if page.crs_code not in (4326, 3857, 4756, 5899) or page.dimensions != "XY":
            raise ApiError("CRS hoặc số chiều chưa được hỗ trợ cho đồng bộ trực tiếp.")
        if page.geometry_family not in ("Point", "LineString", "Polygon"):
            raise ApiError("Kiểu hình học chưa được hỗ trợ cho đồng bộ trực tiếp.")
        if not page.source_id or not all(
            re.fullmatch(r"[a-f0-9]{64}", token)
            for token in (page.content_revision, page.schema_revision)
        ):
            raise ApiError("Định danh nguồn hoặc revision không hợp lệ.")
        if first is None:
            first = page
            names = [f.get("name") for f in page.fields]
            if any(not isinstance(n, str) or not n for n in names):
                raise ApiError("Tên trường không hợp lệ.")
            if len({n.casefold() for n in names}) != len(names) or any(
                n.casefold() in {"ovg_feature_id","ovg_local_fid"} for n in names
            ):
                raise ApiError("Tên trường bị trùng hoặc trùng mã định danh hệ thống.")
        elif (
            page.source_id, page.crs_code, page.geometry_family, page.dimensions,
            page.content_revision, page.schema_revision, page.fields
        ) != (
            first.source_id, first.crs_code, first.geometry_family, first.dimensions,
            first.content_revision, first.schema_revision, first.fields
        ):
            raise ApiError("Lớp thay đổi trong lúc tải; hãy mở lại để tránh mất dữ liệu.")
        if len(page.features) > limit:
            raise ApiError("Trang dữ liệu vượt quá giới hạn.")
        for item in page.features:
            oid = item.get("feature_id")
            if not isinstance(oid, str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", oid) or oid in ids:
                raise ApiError("Mã đối tượng không hợp lệ hoặc trùng lặp.")
            ids.add(oid)
            features.append(item)
            if len(features) > MAX_MANAGED_FEATURES:
                raise ApiError("Snapshot vượt quá 1.000.000 đối tượng.")
        if not page.has_more:
            if page.next_offset is not None:
                raise ApiError("Con trỏ trang cuối không hợp lệ.")
            return first, features
        if not page.features or page.next_offset != offset + len(page.features):
            raise ApiError("Con trỏ phân trang không hợp lệ; hãy mở lại lớp.")
        offset = page.next_offset


def fetch_managed_delta(
    config: OpenVietGisConfig,
    *,
    session_id: str,
    cursor: int,
    limit: int = 200,
) -> ManagedDeltaPage:
    if type(cursor) is not int or cursor < 0:
        raise ApiError("Cursor đồng bộ không hợp lệ.")
    if type(limit) is not int or not 1 <= limit <= MAX_DELTA_PAGE:
        raise ApiError("Giới hạn delta phải từ 1 đến 500.")
    qs = urllib.parse.urlencode({"session_id": session_id, "cursor": cursor, "limit": limit})
    res = _json_request(config, "GET", f"{_managed_base(config)}/delta?{qs}")
    return validate_managed_delta(res, cursor=cursor)


def validate_managed_delta(res: dict[str, Any], *, cursor: int) -> ManagedDeltaPage:
    """Validate the complete cursor envelope before any provider mutation."""
    reset, more = res.get("reset_required"), res.get("has_more")
    nxt, epoch = res.get("next_cursor"), res.get("permission_epoch")
    changes = res.get("changes")
    if (type(reset) is not bool or type(more) is not bool or type(nxt) is not int
            or nxt < 0 or type(epoch) is not int or epoch < 1 or not isinstance(changes, list)):
        raise ApiError("Phản hồi đồng bộ không hợp lệ.")
    if reset:
        if changes or more:
            raise ApiError("Phản hồi yêu cầu tải lại không hợp lệ.")
    else:
        if nxt < cursor or (more and nxt == cursor):
            raise ApiError("Con trỏ đồng bộ không tiến hợp lệ.")
        previous = cursor
        seen: set[tuple[int, str]] = set()
        for item in changes:
            if not isinstance(item, dict):
                raise ApiError("Thay đổi đồng bộ không hợp lệ.")
            seq, lid, src = item.get("cursor"), item.get("layer_id"), item.get("source_id")
            if (type(seq) is not int or seq <= cursor or seq < previous or seq > nxt
                    or not isinstance(lid, str) or not lid or not isinstance(src, str) or not src
                    or (seq, lid) in seen
                    or not isinstance(item.get("content_revision"), str)
                    or not re.fullmatch(r"[a-f0-9]{64}", item["content_revision"])):
                raise ApiError("Revision hoặc định danh đồng bộ không hợp lệ.")
            seen.add((seq, lid))
            previous = seq
            ups, removals = item.get("upserts"), item.get("removals")
            if not isinstance(ups, list) or not isinstance(removals, list):
                raise ApiError("Danh sách cập nhật đồng bộ không hợp lệ.")
            ids = [u.get("feature_id") if isinstance(u, dict) else None for u in ups] + removals
            if any(not isinstance(x, str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", x)
                   for x in ids) or len(ids) != len(set(ids)):
                raise ApiError("Mã đối tượng đồng bộ không hợp lệ hoặc trùng lặp.")
    return ManagedDeltaPage(reset, nxt, epoch, more, tuple(changes), res.get("reason"))



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
    if not isinstance(operations, list) or not 1 <= len(operations) <= MAX_MANAGED_OPERATIONS:
        raise ApiError("Một commit managed phải có từ 1 đến 1000 thao tác.")
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


def diff_managed_snapshots(
    baseline: dict[str, dict[str, Any]],
    current: dict[str, dict[str, Any]],
    *,
    allow_deletes: bool = False,
) -> list[dict[str, Any]]:
    """Diff server-confirmed snapshot against QGIS, including native local saves."""
    removed = baseline.keys() - current.keys()
    if removed and not allow_deletes:
        raise ApiError("Có đối tượng bị xóa; cần xác nhận trước khi lưu lên Web.")
    ops = [
        {"op_type": "delete", "feature_id": oid}
        for oid in sorted(removed)
    ]
    for oid in sorted(current):
        if oid not in baseline:
            ops.append({"op_type": "create", "feature_id": oid, **current[oid]})
        elif current[oid] != baseline[oid]:
            ops.append({"op_type": "update", "feature_id": oid, **current[oid]})
    if len(ops) > MAX_MANAGED_OPERATIONS:
        raise ApiError("Một lần lưu vượt quá 1000 thay đổi; hãy chia nhỏ thao tác.")
    return ops


def native_geometry_wkt(geometry: dict[str, Any]) -> str:
    """Convert supported native XY coordinates without allocating a temporary layer."""
    def position(value):
        if not isinstance(value, (list, tuple)) or len(value) != 2:
            raise ApiError("Hình học phải có đúng hai tọa độ XY.")
        if any(type(v) not in (int, float) or not math.isfinite(v) for v in value):
            raise ApiError("Tọa độ không hợp lệ.")
        return " ".join(str(v) for v in value)

    kind = geometry.get("type")
    coords = geometry.get("coordinates")
    if kind == "Point":
        return "POINT (" + position(coords) + ")"
    if kind == "LineString" and isinstance(coords, list) and len(coords) >= 2:
        return "LINESTRING (" + ", ".join(map(position, coords)) + ")"
    if kind == "Polygon" and isinstance(coords, list) and coords:
        if any(not isinstance(r, list) or len(r) < 4 or r[0] != r[-1] for r in coords):
            raise ApiError("Vòng polygon không hợp lệ.")
        return "POLYGON (" + ", ".join("(" + ", ".join(map(position, r)) + ")" for r in coords) + ")"
    raise ApiError("Kiểu hình học chưa được hỗ trợ trong chế độ đồng bộ.")


def catch_up_cached_layer(config, session, probe, record, cancelled):
    """Apply a bounded, complete event interval and verify the final server pin."""
    cursor = record["cursor"]
    if cursor < 0 or cursor > probe.cursor:
        return None
    features = {str(f["feature_id"]): f for f in record["features"]}
    if len(features) != len(record["features"]):
        return None
    revision = record["revision"]
    for _ in range(8):
        if cancelled():
            return None
        delta = fetch_managed_delta(config, session_id=session.id, cursor=cursor)
        if (delta.reset_required or delta.permission_epoch != probe.permission_epoch
                or delta.next_cursor < cursor or delta.next_cursor > probe.cursor):
            return None
        for change in delta.changes:
            if str(change["source_id"]) != probe.source_id:
                continue
            if str(change["layer_id"]) != probe.layer_id:
                continue
            for oid in change.get("removals", []):
                features.pop(str(oid), None)
            for f in change.get("upserts", []):
                features[str(f["feature_id"])] = f
            revision = str(change["content_revision"])
        if not delta.has_more:
            if (delta.next_cursor != probe.cursor or revision != probe.content_revision
                    or len(features) != probe.feature_count):
                return None
            # Delta queries read live rows. Re-probe to rule out a concurrent edit or ACL change.
            final = bootstrap_managed_layer(config, layer_id=probe.layer_id,
                                            session_id=session.id, limit=1, offset=0)
            if (not final.cacheable or final.cursor != probe.cursor
                    or final.permission_epoch != probe.permission_epoch
                    or final.content_revision != probe.content_revision
                    or final.schema_revision != probe.schema_revision):
                return None
            return list(features.values())
        if delta.next_cursor <= cursor:
            return None
        cursor = delta.next_cursor
    return None


def load_managed_layers(
    config, layer_ids, binding_id, cancelled=lambda: False, progress=lambda n: None, cache_root=None
):
    """Network-only initial load, safe to run outside the QGIS GUI thread."""
    if not isinstance(layer_ids, (list, tuple)) or not 1 <= len(layer_ids) <= MAX_MANAGED_LAYERS:
        raise ApiError("Mỗi phiên chỉ mở tối đa 100 lớp managed.")
    if cancelled():
        return None
    session = create_managed_session(config, binding_id=binding_id)
    result = []
    for index, layer_id in enumerate(layer_ids):
        first = None
        features = []
        path = cache_path(cache_root, config, layer_id) if cache_root is not None else None
        if path is not None and path.is_file():
            # Tiny authenticated read validates current access and revisions before reuse.
            probe = bootstrap_managed_layer(config, layer_id=layer_id,
                                            session_id=session.id, limit=1, offset=0)
            if probe.permission_epoch != session.permission_epoch:
                raise ApiError("Quyền thay đổi trong lúc tải; hãy mở lại.")
            record = read_cached_record(path, probe, allow_older=True)
            cached = None
            if record is not None:
                if record["revision"] == probe.content_revision:
                    cached = record["features"]
                else:
                    try:
                        cached = catch_up_cached_layer(config, session, probe, record, cancelled)
                    except ApiError as exc:
                        if exc.status_code in (401, 403):
                            raise
                    if cached is not None and not cancelled():
                        write_snapshot(path, probe, cached)
            if cached is not None and not cancelled():
                result.append((probe, cached))
                progress(100 * (index + 1) / len(layer_ids))
                continue
        try:
            first, features = load_managed_snapshot(
                config, layer_id=layer_id, session_id=session.id,
                permission_epoch=session.permission_epoch, limit=5000, cancelled=cancelled,
            )
        except ApiError:
            if cancelled():
                return None
            raise
        if path is not None and not cancelled():
            write_snapshot(path, first, features)
        result.append((first, features))
        progress(100 * (index + 1) / len(layer_ids))
    return session, result


def send_pending_commit(config, pending, *, recover=False, permit_replay=True):
    """Only network I/O. A retry checks the durable receipt before sending again."""
    if recover:
        try:
            return get_managed_commit_receipt(config, operation_id=pending["operation_id"])
        except ApiError as exc:
            if exc.status_code != 404:
                raise
            if not permit_replay:
                raise ApiError("Giao dịch chưa xác nhận; cần đối chiếu trước khi gửi lại.") from exc
    return commit_managed_changes(config, **pending["request"])


def validate_managed_operations(operations, fields):
    """Preflight mandatory fields without inventing business values."""
    required = {f["name"]: f.get("display_name") or f["name"] for f in fields
                if not f.get("nullable", True)}
    missing = set()
    affected = 0
    for op in operations:
        if op["op_type"] == "delete":
            continue
        props = op.get("properties") or {}
        bad = {label for name, label in required.items() if props.get(name) is None}
        if bad:
            affected += 1
            missing.update(bad)
        if not op.get("geometry"):
            raise ApiError("Đối tượng chưa có hình học hợp lệ. Hãy hoàn tất hình học trước khi lưu.")
    if affected:
        raise ApiError(
            f"{affected} đối tượng thiếu trường bắt buộc: {', '.join(sorted(missing))}. "
            "Mở bảng thuộc tính hoặc biểu mẫu đối tượng, điền các trường này rồi lưu lại. "
            "Bản nháp vẫn được giữ."
        )
