"""Opt-in R026 sync-preview/sync-push HTTP transport for mapped V8i geometry only.

This changes *OpenVietGIS* only after explicit confirmation. Never writes V8i DGN.
"""
from __future__ import annotations

import hashlib
import json
import os
import ssl
import uuid
from pathlib import Path
from typing import Any
from urllib.request import Request, build_opener, HTTPSHandler

from outbound import CadSyncError, JOURNAL_FILE, OUT_FILE, PLAN_FILE, _write_json, receipt
from receiver import Connection, _NoRedirect

MAX_UPLOAD = 16 * 1024 * 1024
MAX_RESPONSE = 512 * 1024


def _plan_and_bytes(directory: Path) -> tuple[dict[str, Any], bytes]:
    try:
        plan = json.loads((directory / PLAN_FILE).read_text(encoding="utf-8"))
        payload = (directory / OUT_FILE).read_bytes()
    except (ValueError, OSError) as exc:
        raise CadSyncError("Missing local review plan/payload.") from exc
    if plan.get("schema_version") != "ovg.v8i.outbound.v1" or not isinstance(plan.get("edits"), list):
        raise CadSyncError("Invalid outbound plan.")
    if not 0 < len(payload) <= MAX_UPLOAD or hashlib.sha256(payload).hexdigest() != plan.get("gpkg_sha256"):
        raise CadSyncError("Outbound GeoPackage changed since local review.")
    return plan, payload


def _post(
    c: Connection, layer_id: str, action: str, fields: dict[str, str], payload: bytes, *, opener: Any = None
) -> dict[str, Any]:
    c.validated()
    if action not in {"sync-preview", "sync-push"}:
        raise CadSyncError("Unsupported sync command.")
    boundary = "OVG" + uuid.uuid4().hex
    body = bytearray()
    for name, value in fields.items():
        if not name.replace("_", "").isalnum() or any(ch in value for ch in "\r\n\x00"):
            raise CadSyncError("Invalid sync field.")
        body.extend(f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n'.encode())
    body.extend(f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="outbound.gpkg"\r\nContent-Type: application/geopackage+sqlite3\r\n\r\n'.encode())
    body.extend(payload)
    body.extend(f'\r\n--{boundary}--\r\n'.encode())
    # Both UUID and server origin validated before URL construction.
    url = (c.server.rstrip("/") + "/api/v1/organizations/" + str(uuid.UUID(c.organization_id))
           + "/projects/" + str(uuid.UUID(c.project_id))
           + "/integration-hub/adapters/qgis/layers/" + str(uuid.UUID(layer_id)) + "/" + action)
    request = Request(url, data=bytes(body), method="POST", headers={
        "Authorization": "Bearer " + c.bearer_token,
        "Content-Type": "multipart/form-data; boundary=" + boundary,
        "Accept": "application/json", "Cache-Control": "no-store",
    })
    if opener is None:
        opener = build_opener(_NoRedirect(), HTTPSHandler(context=ssl.create_default_context()))
    try:
        with opener.open(request, timeout=45) as response:
            if response.geturl() != url or response.getcode() not in ({200} if action == "sync-preview" else {201}):
                raise CadSyncError("Unexpected HTTP response or redirect.")
            data = response.read(MAX_RESPONSE + 1)
            if len(data) > MAX_RESPONSE:
                raise CadSyncError("Sync response exceeds limit.")
        answer = json.loads(data.decode("utf-8"))
        if not isinstance(answer, dict):
            raise CadSyncError("Malformed server response.")
        return answer
    except CadSyncError:
        raise
    except Exception as exc:
        # This includes read timeout after the server commit: never retry blindly.
        raise CadSyncError("Server request failed/uncertain; check authorization and operation history.") from exc


def _validate_preview(p: dict[str, Any], plan: dict[str, Any], meta: dict[str, Any]) -> None:
    expected = {entry["feature_id"] for entry in plan["edits"]}
    ops = p.get("operations")
    if (
        p.get("layer_id") != meta["layer_id"] or p.get("base_content_revision") != meta["content_revision"]
        or p.get("current_content_revision") != meta["content_revision"]
        or p.get("base_schema_revision") != meta["schema_revision"]
        or p.get("current_schema_revision") != meta["schema_revision"]
        or p.get("stale_revision") is not False or p.get("deletions_blocked") is not True
        or p.get("can_commit") is not True or p.get("create_count") != 0
        or p.get("update_count") != len(expected) or p.get("omitted_server_feature_count") != 0
        or p.get("stored_crs_code") != int(meta["crs"].split(":")[1])
        or not isinstance(ops, list) or len(ops) != len(expected)
        or len({op.get("feature_id") for op in ops if isinstance(op, dict)}) != len(expected)
        or not isinstance(p.get("diff_sha256"), str) or len(p["diff_sha256"]) != 64
    ):
        raise CadSyncError("Server diff does not match mapped local geometry; review denied.")
    for op in ops:
        if (not isinstance(op, dict) or op.get("feature_id") not in expected
                or op.get("op_type") != "update" or op.get("geometry_changed") is not True
                or op.get("changed_fields") != []):
            raise CadSyncError("Server diff contains unauthorized or unexpected feature/field edits.")


def preview_cloud(c: Connection, bundle: str | Path, *, opener: Any = None) -> dict[str, Any]:
    directory, meta = receipt(bundle)
    if (c.server.rstrip("/") != meta["server_origin"].rstrip("/")
            or c.organization_id != meta["organization_id"]
            or c.project_id != meta["project_id"]):
        raise CadSyncError("Current connection must match the received bundle scope.")
    plan, payload = _plan_and_bytes(directory)
    if (directory / JOURNAL_FILE).exists():
        raise CadSyncError("Existing outbound journal: do not create a second in-flight operation.")
    if plan.get("layer_id") != meta["layer_id"] or plan.get("content_revision") != meta["content_revision"]:
        raise CadSyncError("Outbound plan does not match source revision.")
    p = _post(c, meta["layer_id"], "sync-preview", {
        "base_content_revision": meta["content_revision"],
        "base_schema_revision": meta["schema_revision"],
    }, payload, opener=opener)
    _validate_preview(p, plan, meta)
    plan["server_preview"] = {
        "diff_sha256": p["diff_sha256"], "gpkg_sha256": plan["gpkg_sha256"],
        "updated_features": sorted(entry["feature_id"] for entry in plan["edits"]),
    }
    _write_json(directory / PLAN_FILE, plan)
    return {"diff_sha256": p["diff_sha256"], "update_count": p["update_count"],
            "create_count": 0, "delete_count": 0, "requires_explicit_commit": True}


def commit_approved(
    c: Connection, bundle: str | Path, confirmed_diff_sha256: str, *, opener: Any = None
) -> dict[str, Any]:
    """Review-bound commit. Persist the operation ID *before* HTTP write. No blind retries."""
    # Explicit local pilot gate: production writes remain disabled until real
    # Windows/V8i acceptance. Both values must be set intentionally per test project.
    if (os.environ.get("OVG_V8I_WRITEBACK_PILOT") != "ALLOW_EXPLICIT_TEST_WRITE"
            or os.environ.get("OVG_V8I_TEST_PROJECT_ID") != c.project_id):
        raise CadSyncError("V8i Cloud write-back is disabled until an explicitly approved test-project pilot.")
    directory, meta = receipt(bundle)
    if (c.server.rstrip("/") != meta["server_origin"].rstrip("/")
            or c.organization_id != meta["organization_id"]
            or c.project_id != meta["project_id"]):
        raise CadSyncError("Current connection must match the received bundle scope.")
    plan, payload = _plan_and_bytes(directory)
    journal = directory / JOURNAL_FILE
    if journal.exists():
        raise CadSyncError("Pending/finished operation journal exists; cannot retry or duplicate a write.")
    previous = plan.get("server_preview")
    if not isinstance(previous, dict) or previous.get("diff_sha256") != confirmed_diff_sha256:
        raise CadSyncError("Human confirmation must match full server preview digest.")
    if previous.get("gpkg_sha256") != plan["gpkg_sha256"]:
        raise CadSyncError("Review file changed since server preview.")
    p = _post(c, meta["layer_id"], "sync-preview", {
        "base_content_revision": meta["content_revision"],
        "base_schema_revision": meta["schema_revision"],
    }, payload, opener=opener)
    _validate_preview(p, plan, meta)
    if p["diff_sha256"] != confirmed_diff_sha256:
        raise CadSyncError("Server revision/diff changed; re-preview before committing.")
    operation_id = str(uuid.uuid4())
    _write_json(journal, {"schema_version": "ovg.v8i.outbound-journal.v1",
                          "operation_id": operation_id, "state": "sent_unconfirmed",
                          "diff_sha256": confirmed_diff_sha256,
                          "gpkg_sha256": plan["gpkg_sha256"]})
    result = _post(c, meta["layer_id"], "sync-push", {
        "base_content_revision": meta["content_revision"],
        "base_schema_revision": meta["schema_revision"],
        "operation_id": operation_id, "expected_diff_sha256": confirmed_diff_sha256,
        "summary": "Manually approved V8i mapped geometry update",
    }, payload, opener=opener)
    if (result.get("operation_id") != operation_id or result.get("diff_sha256") != confirmed_diff_sha256
            or result.get("operation_count") != len(plan["edits"])
            or result.get("deletions_blocked") is not True
            or result.get("layer_id") != meta["layer_id"]):
        raise CadSyncError("Server acknowledgement inconsistent; journal retained for investigation.")
    _write_json(journal, {
        "schema_version": "ovg.v8i.outbound-journal.v1", "operation_id": operation_id,
        "state": "acknowledged", "changeset_id": result.get("changeset_id"),
        "content_revision": result.get("resulting_content_revision"),
        "diff_sha256": confirmed_diff_sha256,
    })
    return {"changeset_id": result.get("changeset_id"), "operation_id": operation_id,
            "update_count": len(plan["edits"]), "state": "acknowledged"}