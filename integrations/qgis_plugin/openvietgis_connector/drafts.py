"""Private, atomic crash recovery journals for managed QGIS layers."""
from __future__ import annotations

import hashlib
import json
import os
import tempfile
from pathlib import Path
from typing import Any


def draft_path(root: Path, origin: str, organization: str, project: str, layer: str) -> Path:
    key = hashlib.sha256(json.dumps([origin, organization, project, layer]).encode()).hexdigest()
    return root / (key + ".json")


def write_draft(path: Path, data: dict[str, Any]) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix=".draft-", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(data, handle, ensure_ascii=False, allow_nan=False)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def read_draft(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict) or data.get("version") != 1:
        raise ValueError("Bản nháp không đúng phiên bản.")
    return data


def merge_snapshots(
    baseline: dict[str, Any], local: dict[str, Any], remote: dict[str, Any]
) -> tuple[dict[str, Any], list[str]]:
    """Three-way merge by stable ID; never silently overwrite concurrent edits."""
    missing = object()
    merged = {}
    conflicts = []
    for oid in sorted(baseline.keys() | local.keys() | remote.keys()):
        old = baseline.get(oid, missing)
        left = local.get(oid, missing)
        right = remote.get(oid, missing)
        if left == right or right == old:
            value = left
        elif left == old:
            value = right
        else:
            conflicts.append(oid)
            continue
        if value is not missing:
            merged[oid] = value
    return merged, conflicts


def rebind_pending(pending: dict[str, Any], baseline: dict, current: dict, *,
                   content_revision: str, schema_revision: str, source_id: str,
                   session_id: str) -> dict[str, Any]:
    """Keep the exact idempotent request after a fresh, authorized receipt miss."""
    request = pending["request"]
    if (request["base_content_revision"] != content_revision
            or request["base_schema_revision"] != schema_revision
            or request["source_id"] != source_id
            or request["operation_id"] != pending["operation_id"]
            or baseline != current):
        raise ValueError("Giao dịch chưa xác nhận và dữ liệu đã đổi; cần đối chiếu.")
    return {**pending, "request": {**request, "session_id": session_id}}
