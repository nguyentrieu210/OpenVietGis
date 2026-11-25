"""R029 Managed Realtime GIS Client HTTP adapter.

Provides:
- POST/GET/DELETE /managed/sessions: server-managed client sessions with capability flags,
  permission_epoch, heartbeat, and immediate revocation.
- GET /managed/bootstrap: ACL-filtered initial snapshot pinned atomically with cursor & epoch.
- GET /managed/delta: commit-ordered durable delta feed after cursor with retention/epoch checks.
- POST /managed/commits & GET /managed/commits/{operation_id}: server-authoritative R006 commit
  wrapper returning durable receipt + event_cursor (never pretending local save succeeded).
- GET /managed/events: authenticated SSE wake-up stream with 15s bounded auth/revocation recheck.
- POST/GET /managed/desktop-sessions/{id}/commands (+ /respond): P2 allowlisted AI desktop
  command bridge with explicit consent and strict tool allowlist (no arbitrary Python/shell).
"""

from __future__ import annotations

import asyncio
import json
import time
from collections.abc import AsyncIterator
from datetime import UTC, datetime, timedelta
from typing import Annotated, Any, Literal
from uuid import UUID, uuid4

import psycopg
from fastapi import APIRouter, Depends, HTTPException, Query, Request
from fastapi.responses import StreamingResponse
from pydantic import BaseModel, ConfigDict, Field, StringConstraints
from sqlalchemy import text
from sqlalchemy.engine import RowMapping, make_url
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence import gis_access
from openvietgis_api.adapters.persistence.authorization_repository import (
    SqlEffectivePermissionProvider,
)
from openvietgis_api.adapters.persistence.db import get_session_factory
from openvietgis_api.adapters.persistence.editing_repository import SqlEditingRepository
from openvietgis_api.adapters.persistence.rls import require_project_context
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.config import get_settings
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.editing.domain import EditOperationInput, EditOpType

router = APIRouter(tags=["managed-sync"])
BASE = "/organizations/{organization_id}/projects/{project_id}/integration-hub/managed"
ReadAuth = Annotated[AuthenticatedRequest, Depends(authenticate_request)]
WriteAuth = Annotated[AuthenticatedRequest, Depends(require_csrf)]
DB = Annotated[Session, Depends(get_db_session, scope="function")]

ALLOWED_DESKTOP_TOOLS = frozenset(
    {
        "inspect_project",
        "inspect_layer",
        "select_features",
        "zoom_to_extent",
        "preview_topology",
        "stage_edits",
    }
)
MUTATING_DESKTOP_TOOLS = frozenset({"stage_edits"})


class CreateManagedSessionRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    binding_id: UUID | None = None
    device_label: Annotated[
        str, StringConstraints(strip_whitespace=True, min_length=1, max_length=120)
    ] = "QGIS Desktop"
    client_version: Annotated[
        str, StringConstraints(strip_whitespace=True, min_length=1, max_length=40)
    ] = "0.9.0"
    mode: Literal["managed", "legacy_snapshot"] = "managed"
    expires_in_hours: int = Field(default=24, ge=1, le=720)


class ManagedOperationRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    op_type: EditOpType
    feature_id: Annotated[
        str, StringConstraints(strip_whitespace=True, min_length=1, max_length=128)
    ]
    geometry: dict[str, Any] | None = None
    properties: dict[str, Any] | None = None


class ManagedCommitRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    session_id: UUID
    source_id: UUID
    base_content_revision: Annotated[str, StringConstraints(min_length=64, max_length=64)]
    base_schema_revision: Annotated[str, StringConstraints(min_length=64, max_length=64)] | None = (
        None
    )
    operation_id: Annotated[
        str, StringConstraints(strip_whitespace=True, min_length=1, max_length=128)
    ]
    summary: Annotated[
        str, StringConstraints(strip_whitespace=True, min_length=1, max_length=500)
    ] = "QGIS Managed Realtime Edit"
    allow_deletes: bool = False
    operations: list[ManagedOperationRequest] = Field(min_length=1, max_length=1000)


class CreateDesktopCommandRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    tool_name: Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=64)]
    arguments: dict[str, Any] = Field(default_factory=dict)
    ttl_seconds: int = Field(default=120, ge=10, le=600)


class RespondDesktopCommandRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    status: Literal["approved", "rejected", "completed", "failed"]
    result: dict[str, Any] | None = None


def _ensure_counter(session: Session, org: UUID, proj: UUID) -> tuple[int, int, int]:
    row = (
        session.execute(
            text(
                "SELECT last_cursor, permission_epoch, retention_floor "
                "FROM integration_hub.project_event_counters "
                "WHERE organization_id=:o AND project_id=:p"
            ),
            {"o": org, "p": proj},
        )
        .mappings()
        .first()
    )
    if row is None:
        return (0, 1, 0)
    return (int(row["last_cursor"]), int(row["permission_epoch"]), int(row["retention_floor"]))


def _session_wire(row: RowMapping, current_cursor: int | None = None) -> dict[str, Any]:
    out: dict[str, Any] = {
        "id": str(row["id"]),
        "organization_id": str(row["organization_id"]),
        "project_id": str(row["project_id"]),
        "binding_id": str(row["binding_id"]) if row["binding_id"] else None,
        "actor_type": str(row["actor_type"]),
        "actor_id": str(row["actor_id"]),
        "actor_display": str(row["actor_display"]),
        "device_label": str(row["device_label"]),
        "client_version": str(row["client_version"]),
        "mode": str(row["mode"]),
        "capabilities": dict(row["capabilities"] or {}),
        "permission_epoch": int(row["permission_epoch"]),
        "last_ack_cursor": int(row["last_ack_cursor"]),
        "state": str(row["state"]),
        "expires_at": row["expires_at"].isoformat(),
        "revoked_at": row["revoked_at"].isoformat() if row["revoked_at"] else None,
        "last_seen_at": row["last_seen_at"].isoformat(),
        "created_at": row["created_at"].isoformat(),
    }
    if current_cursor is not None:
        out["current_cursor"] = current_cursor
    return out


def _load_active_session(
    session: Session,
    org: UUID,
    proj: UUID,
    session_id: UUID,
    auth: AuthenticatedRequest,
    *,
    allow_manager: bool = False,
    require_current_epoch: bool = True,
) -> RowMapping:
    row = (
        session.execute(
            text(
                "SELECT * FROM integration_hub.managed_sessions "
                "WHERE organization_id=:o AND project_id=:p AND id=:sid"
            ),
            {"o": org, "p": proj, "sid": session_id},
        )
        .mappings()
        .first()
    )
    if row is None:
        raise HTTPException(404, "Managed session not found.")
    _require_session_access(session, auth, org, proj, row, allow_manager=allow_manager)
    now = datetime.now(UTC)
    if str(row["state"]) != "active" or row["revoked_at"] is not None:
        raise HTTPException(403, "Managed session is revoked or inactive.")
    if row["expires_at"] <= now:
        raise HTTPException(401, "Managed session has expired.")
    if (
        require_current_epoch
        and int(row["permission_epoch"]) != _ensure_counter(session, org, proj)[1]
    ):
        raise HTTPException(403, "Managed session permission epoch changed; reconnect.")
    return row


def _owns_session(auth: AuthenticatedRequest, row: RowMapping) -> bool:
    return row["actor_id"] == auth.actor.actor_id and row["actor_type"] == auth.actor.actor_type


def _is_project_manager(
    session: Session, auth: AuthenticatedRequest, org: UUID, proj: UUID
) -> bool:
    ctx = context_for(auth, organization_id=org, project_id=proj)
    return Permission.PROJECT_MANAGE in SqlEffectivePermissionProvider(session).resolve(ctx)


def _require_session_access(
    session: Session,
    auth: AuthenticatedRequest,
    org: UUID,
    proj: UUID,
    row: RowMapping,
    *,
    allow_manager: bool = False,
) -> None:
    if _owns_session(auth, row):
        return
    if allow_manager and _is_project_manager(session, auth, org, proj):
        return
    # Do not disclose whether another actor's session is active or expired.
    raise HTTPException(404, "Managed session not found.")


def _prep_scope(
    session: Session,
    auth: AuthenticatedRequest,
    org: UUID,
    proj: UUID,
    permission: Permission,
    *,
    mutation: bool,
) -> AuthorizationContext:
    ctx = context_for(auth, organization_id=org, project_id=proj)
    prepare_project_scope(session, ctx, permission=permission, mutation=mutation)
    return ctx


def _can_edit_project(
    session: Session,
    auth: AuthenticatedRequest,
    org: UUID,
    proj: UUID,
) -> bool:
    """Inspect the current actor's grants without disturbing its READ RLS scope.

    A PROJECT_EDIT + mutation=False call is invalid for SqlSpatialRepository.
    Worse, prepare_scope() clears the established project claim *before*
    rejecting that combination, so the subsequent R029 INSERT loses its RLS
    context. Permission inspection must not re-bind or clear project scope.
    """
    ctx = context_for(auth, organization_id=org, project_id=proj)
    return Permission.PROJECT_EDIT in SqlEffectivePermissionProvider(session).resolve(ctx)


def _notify_project_channel(session: Session, org: UUID, proj: UUID, kind: str) -> None:
    payload = json.dumps(
        {"o": str(org), "p": str(proj), "a": None, "b": None, "kind": kind},
        separators=(",", ":"),
    )
    session.execute(
        text("SELECT pg_notify('ovg_team_collaboration', :payload)"),
        {"payload": payload},
    )


@router.post(BASE + "/sessions", status_code=201)
def create_managed_session(
    organization_id: UUID,
    project_id: UUID,
    body: CreateManagedSessionRequest,
    auth: WriteAuth,
    session: DB,
) -> dict[str, Any]:
    """Negotiate a server-managed realtime GIS client session."""
    ctx = _prep_scope(
        session, auth, organization_id, project_id, Permission.PROJECT_READ, mutation=False
    )
    if body.binding_id is not None:
        require_project_context(session, ctx, permission=Permission.PROJECT_READ)
        service = auth.actor.actor_type == "service_principal"
        if service:
            if auth.actor.actor_id is None or auth.credential_id is None:
                raise HTTPException(403, "Managed binding is not active or authorized.")
            # Only verified request identity; never derive these values from the body.
            session.execute(
                text(
                    "SELECT set_config('openvietgis.qgis_pairing_principal', :actor, true), "
                    "set_config('openvietgis.qgis_pairing_credential', :credential, true)"
                ),
                {"actor": str(auth.actor.actor_id), "credential": str(auth.credential_id)},
            )
        binding = (
            session.execute(
                text(
                    "SELECT state FROM integration_hub.connector_bindings "
                    "WHERE organization_id=:o AND project_id=:p AND id=:bid"
                ),
                {"o": organization_id, "p": project_id, "bid": body.binding_id},
            )
            .mappings()
            .first()
        )
        if service:
            session.execute(
                text(
                    "SELECT set_config('openvietgis.qgis_pairing_principal', '', true), "
                    "set_config('openvietgis.qgis_pairing_credential', '', true)"
                )
            )
        if binding is None or str(binding["state"]) != "active":
            raise HTTPException(403, "Managed binding is not active or authorized.")

    can_edit = _can_edit_project(session, auth, organization_id, project_id)
    last_cursor, perm_epoch, _ = _ensure_counter(session, organization_id, project_id)
    now = datetime.now(UTC)
    expires_at = now + timedelta(hours=body.expires_in_hours)
    sid = uuid4()
    capabilities = {
        "can_read": True,
        "can_edit": can_edit,
        "can_delete": False,
        "realtime_delta": True,
        "ai_desktop_bridge": True,
    }
    actor_display = (
        getattr(auth.actor, "display_name", None)
        or getattr(auth.actor, "email", None)
        or str(auth.actor.actor_id)
    )
    row = (
        session.execute(
            text(
                "INSERT INTO integration_hub.managed_sessions ("
                "id, organization_id, project_id, binding_id, actor_type, actor_id, "
                "actor_display, device_label, client_version, mode, capabilities, "
                "permission_epoch, last_ack_cursor, state, expires_at, last_seen_at, created_at"
                ") VALUES ("
                ":id, :o, :p, :bid, :atype, :aid, :adisp, :dev, :ver, :mode, "
                "CAST(:caps AS jsonb), :epoch, :cur, 'active', :exp, :now, :now"
                ") RETURNING *"
            ),
            {
                "id": sid,
                "o": organization_id,
                "p": project_id,
                "bid": body.binding_id,
                "atype": auth.actor.actor_type,
                "aid": auth.actor.actor_id,
                "adisp": str(actor_display)[:200],
                "dev": body.device_label,
                "ver": body.client_version,
                "mode": body.mode,
                "caps": json.dumps(capabilities),
                "epoch": perm_epoch,
                "cur": last_cursor,
                "exp": expires_at,
                "now": now,
            },
        )
        .mappings()
        .one()
    )
    append_project_audit(
        session,
        ctx,
        audit_id=uuid4(),
        action="integration.managed_session.created",
        resource_type="managed_session",
        resource_id=sid,
        metadata={
            "device_label": body.device_label,
            "client_version": body.client_version,
            "can_edit": can_edit,
        },
        created_at=now,
    )
    session.commit()
    return _session_wire(row, current_cursor=last_cursor)


@router.get(BASE + "/sessions")
def list_managed_sessions(
    organization_id: UUID,
    project_id: UUID,
    auth: ReadAuth,
    session: DB,
) -> dict[str, Any]:
    """List managed desktop sessions and current project cursor health."""
    _prep_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ, mutation=False)
    last_cursor, perm_epoch, retention_floor = _ensure_counter(session, organization_id, project_id)
    manager = _is_project_manager(session, auth, organization_id, project_id)
    rows = (
        session.execute(
            text(
                "SELECT * FROM integration_hub.managed_sessions "
                "WHERE organization_id=:o AND project_id=:p "
                "AND (:manager OR (actor_id=:aid AND actor_type=:atype)) "
                "ORDER BY created_at DESC LIMIT 100"
            ),
            {
                "o": organization_id,
                "p": project_id,
                "manager": manager,
                "aid": auth.actor.actor_id,
                "atype": auth.actor.actor_type,
            },
        )
        .mappings()
        .all()
    )
    return {
        "current_cursor": last_cursor,
        "permission_epoch": perm_epoch,
        "retention_floor": retention_floor,
        "items": [_session_wire(r) for r in rows],
    }


@router.get(BASE + "/sessions/{session_id}")
def get_managed_session(
    organization_id: UUID,
    project_id: UUID,
    session_id: UUID,
    auth: ReadAuth,
    session: DB,
) -> dict[str, Any]:
    """Inspect managed session status, capability flags, and current cursor."""
    _prep_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ, mutation=False)
    row = (
        session.execute(
            text(
                "SELECT * FROM integration_hub.managed_sessions "
                "WHERE organization_id=:o AND project_id=:p AND id=:sid"
            ),
            {"o": organization_id, "p": project_id, "sid": session_id},
        )
        .mappings()
        .first()
    )
    if row is None:
        raise HTTPException(404, "Managed session not found.")
    _require_session_access(session, auth, organization_id, project_id, row, allow_manager=True)
    last_cursor, _, _ = _ensure_counter(session, organization_id, project_id)
    return _session_wire(row, current_cursor=last_cursor)


@router.delete(BASE + "/sessions/{session_id}")
def revoke_managed_session(
    organization_id: UUID,
    project_id: UUID,
    session_id: UUID,
    auth: WriteAuth,
    session: DB,
    bump_epoch: Annotated[bool, Query()] = False,
) -> dict[str, Any]:
    """Revoke a managed session immediately and notify connected clients."""
    ctx = _prep_scope(
        session, auth, organization_id, project_id, Permission.PROJECT_READ, mutation=False
    )
    row = (
        session.execute(
            text(
                "SELECT * FROM integration_hub.managed_sessions "
                "WHERE organization_id=:o AND project_id=:p AND id=:sid"
            ),
            {"o": organization_id, "p": project_id, "sid": session_id},
        )
        .mappings()
        .first()
    )
    if row is None:
        raise HTTPException(404, "Managed session not found.")
    if not _owns_session(auth, row) or bump_epoch:
        if not _is_project_manager(session, auth, organization_id, project_id):
            raise HTTPException(403, "Project manage permission required.")
        prepare_project_scope(session, ctx, permission=Permission.PROJECT_MANAGE, mutation=True)
    now = datetime.now(UTC)
    updated = (
        session.execute(
            text(
                "UPDATE integration_hub.managed_sessions "
                "SET state='revoked', revoked_at=:now, last_seen_at=:now "
                "WHERE organization_id=:o AND project_id=:p AND id=:sid "
                "RETURNING *"
            ),
            {"o": organization_id, "p": project_id, "sid": session_id, "now": now},
        )
        .mappings()
        .one()
    )
    if bump_epoch:
        session.execute(
            text(
                "INSERT INTO integration_hub.project_event_counters "
                "(organization_id, project_id, last_cursor, "
                "permission_epoch, retention_floor, updated_at) "
                "VALUES (:o, :p, 0, 2, 0, :now) "
                "ON CONFLICT (organization_id, project_id) "
                "DO UPDATE SET permission_epoch = "
                "integration_hub.project_event_counters.permission_epoch + 1, "
                "updated_at = :now"
            ),
            {"o": organization_id, "p": project_id, "now": now},
        )
    _notify_project_channel(session, organization_id, project_id, "session_revoked")
    append_project_audit(
        session,
        ctx,
        audit_id=uuid4(),
        action="integration.managed_session.revoked",
        resource_type="managed_session",
        resource_id=session_id,
        metadata={"bump_epoch": bump_epoch},
        created_at=now,
    )
    session.commit()
    return _session_wire(updated)


@router.get(BASE + "/bootstrap")
def bootstrap_managed_layer(
    organization_id: UUID,
    project_id: UUID,
    auth: ReadAuth,
    session: DB,
    layer_id: Annotated[UUID, Query()],
    session_id: Annotated[UUID, Query()],
    limit: Annotated[int, Query(ge=1, le=5000)] = 1000,
    offset: Annotated[int, Query(ge=0)] = 0,
) -> dict[str, Any]:
    """Return ACL-filtered layer features pinned atomically with cursor and revision."""
    ctx = _prep_scope(
        session, auth, organization_id, project_id, Permission.PROJECT_READ, mutation=False
    )
    _load_active_session(session, organization_id, project_id, session_id, auth)

    layer = (
        session.execute(
            text(
                "SELECT l.id, l.name, l.source_id, s.geometry_family, s.dimensions, "
                "s.stored_crs AS stored_crs_code, s.content_revision, s.schema_revision, "
                "s.feature_count "
                "FROM spatial.layers l "
                "JOIN spatial.data_sources s "
                "  ON s.id=l.source_id AND s.organization_id=l.organization_id "
                "  AND s.project_id=l.project_id "
                "WHERE l.organization_id=:o AND l.project_id=:p AND l.id=:lid "
                "AND l.lifecycle='active' AND s.lifecycle='active'"
            ),
            {"o": organization_id, "p": project_id, "lid": layer_id},
        )
        .mappings()
        .first()
    )
    if layer is None:
        raise HTTPException(404, "Layer not found.")

    if gis_access.scoped(session, ctx) and not gis_access.source_allowed(
        session, ctx, layer["source_id"], "read"
    ):
        raise HTTPException(403, "Layer is outside your authorized GIS scope.")

    last_cursor, perm_epoch, _ = _ensure_counter(session, organization_id, project_id)
    fields = (
        session.execute(
            text(
                "SELECT field_id AS name, name AS display_name, type AS data_type, nullable "
                "FROM spatial.source_fields "
                "WHERE organization_id=:o AND project_id=:p AND source_id=:sid "
                "ORDER BY ordinal ASC"
            ),
            {"o": organization_id, "p": project_id, "sid": layer["source_id"]},
        )
        .mappings()
        .all()
    )

    params: dict[str, Any] = {
        "o": organization_id,
        "p": project_id,
        "sid": layer["source_id"],
        "lim": limit + 1,
        "off": offset,
    }
    rows = (
        session.execute(
            text(
                "SELECT f.feature_id, ST_AsGeoJSON(f.geometry, 15, 0) AS geom_json, "
                "f.attributes AS properties, f.content_revision "
                "FROM spatial.reference_features f "
                "WHERE f.organization_id=:o AND f.project_id=:p AND f.source_id=:sid "
                "ORDER BY f.feature_id ASC LIMIT :lim OFFSET :off"
            ),
            params,
        )
        .mappings()
        .all()
    )
    pin = (
        session.execute(
            text(
                "SELECT content_revision, schema_revision FROM spatial.data_sources "
                "WHERE organization_id=:o AND project_id=:p AND id=:sid AND lifecycle='active'"
            ),
            {"o": organization_id, "p": project_id, "sid": layer["source_id"]},
        )
        .mappings()
        .first()
    )
    if (
        pin is None
        or pin["content_revision"] != layer["content_revision"]
        or pin["schema_revision"] != layer["schema_revision"]
        or _ensure_counter(session, organization_id, project_id)[:2] != (last_cursor, perm_epoch)
    ):
        raise HTTPException(409, "Snapshot changed during read; reopen layer.")
    has_more = len(rows) > limit
    page = rows[:limit]
    features = [
        {
            "feature_id": str(r["feature_id"]),
            "geometry": json.loads(r["geom_json"]) if r["geom_json"] else None,
            "properties": dict(r["properties"] or {}),
            "content_revision": str(r["content_revision"]),
        }
        for r in page
    ]
    return {
        "layer_id": str(layer["id"]),
        "source_id": str(layer["source_id"]),
        "name": str(layer["name"]),
        "geometry_family": str(layer["geometry_family"]),
        "dimensions": str(layer["dimensions"]),
        "crs_code": int(layer["stored_crs_code"]) if layer["stored_crs_code"] else None,
        "content_revision": str(layer["content_revision"]),
        "schema_revision": str(layer["schema_revision"]),
        "cursor": last_cursor,
        "permission_epoch": perm_epoch,
        "cacheable": not gis_access.scoped(session, ctx) and layer["feature_count"] is not None,
        "feature_count": int(layer["feature_count"])
        if not gis_access.scoped(session, ctx) and layer["feature_count"] is not None
        else None,
        "fields": [
            {
                "name": str(f["name"]),
                "display_name": str(f["display_name"]),
                "data_type": str(f["data_type"]),
                "nullable": bool(f["nullable"]),
            }
            for f in fields
        ],
        "features": features,
        "has_more": has_more,
        "next_offset": offset + len(features) if has_more else None,
    }


@router.get(BASE + "/delta")
def fetch_managed_delta(
    organization_id: UUID,
    project_id: UUID,
    auth: ReadAuth,
    session: DB,
    session_id: Annotated[UUID, Query()],
    cursor: Annotated[int, Query(ge=0)] = 0,
    limit: Annotated[int, Query(ge=1, le=500)] = 200,
) -> dict[str, Any]:
    """Fetch commit-ordered authorized deltas after cursor without full layer reload."""
    ctx = _prep_scope(
        session, auth, organization_id, project_id, Permission.PROJECT_READ, mutation=False
    )
    sess_row = _load_active_session(
        session, organization_id, project_id, session_id, auth, require_current_epoch=False
    )
    last_cursor, perm_epoch, retention_floor = _ensure_counter(session, organization_id, project_id)
    if (
        int(sess_row["permission_epoch"]) != perm_epoch
        or cursor < retention_floor
        or cursor > last_cursor
    ):
        return {
            "reset_required": True,
            "reason": "permission_epoch_or_retention_changed",
            "next_cursor": last_cursor,
            "permission_epoch": perm_epoch,
            "has_more": False,
            "changes": [],
        }

    # Quiet poll: skip event and feature queries entirely. Never advance an
    # acknowledgement based on the server response rather than the caller cursor.
    if cursor == last_cursor:
        if cursor > int(sess_row["last_ack_cursor"]):
            session.execute(
                text(
                    "UPDATE integration_hub.managed_sessions "
                    "SET last_ack_cursor=GREATEST(last_ack_cursor,:cur), "
                    "last_seen_at=now() WHERE organization_id=:o AND project_id=:p AND id=:sid"
                ),
                {"o": organization_id, "p": project_id, "sid": session_id, "cur": cursor},
            )
            session.commit()
        return {
            "reset_required": False,
            "next_cursor": cursor,
            "permission_epoch": perm_epoch,
            "has_more": False,
            "changes": [],
        }

    events = (
        session.execute(
            text(
                "SELECT cursor, kind, source_id, changeset_id, content_revision, committed_at "
                "FROM integration_hub.project_events "
                "WHERE organization_id=:o AND project_id=:p AND cursor > :cur AND cursor <= :high "
                "ORDER BY cursor ASC LIMIT :lim"
            ),
            {
                "o": organization_id,
                "p": project_id,
                "cur": cursor,
                "high": last_cursor,
                "lim": limit + 1,
            },
        )
        .mappings()
        .all()
    )
    has_more = len(events) > limit
    page = events[:limit]
    if not page:
        if cursor > int(sess_row["last_ack_cursor"]):
            session.execute(
                text(
                    "UPDATE integration_hub.managed_sessions "
                    "SET last_ack_cursor=GREATEST(last_ack_cursor,:cur), "
                    "last_seen_at=now() WHERE organization_id=:o AND project_id=:p AND id=:sid"
                ),
                {"o": organization_id, "p": project_id, "sid": session_id, "cur": cursor},
            )
            session.commit()
        return {
            "reset_required": False,
            "next_cursor": cursor,
            "permission_epoch": perm_epoch,
            "has_more": False,
            "changes": [],
        }

    scanned_through = int(page[-1]["cursor"])
    is_scoped = gis_access.scoped(session, ctx)

    changes: list[dict[str, Any]] = []
    by_source: dict[UUID, list[RowMapping]] = {}
    for ev in page:
        kind = str(ev["kind"])
        if kind != "changeset" or ev["changeset_id"] is None or ev["source_id"] is None:
            return {
                "reset_required": True,
                "reason": kind,
                "next_cursor": last_cursor,
                "permission_epoch": perm_epoch,
                "has_more": False,
                "changes": [],
            }
        src_id = ev["source_id"]
        if is_scoped and not gis_access.source_allowed(session, ctx, src_id, "read"):
            continue
        if is_scoped:
            return {
                "reset_required": True,
                "reason": "scoped_snapshot_required",
                "next_cursor": last_cursor,
                "permission_epoch": perm_epoch,
                "has_more": False,
                "changes": [],
            }
        by_source.setdefault(src_id, []).append(ev)

    # Collapse touched IDs at each source's final event in this bounded page.
    # If newer events exist beyond the page, the source revision check below
    # rejects rather than mixing an old revision with current geometry.
    delivered_features = 0
    for group in sorted(by_source.values(), key=lambda group: int(group[-1]["cursor"])):
        ev = group[-1]
        src_id = ev["source_id"]
        layer_rows = (
            session.execute(
                text(
                    "SELECT l.id AS layer_id, s.stored_crs AS stored_crs_code, s.content_revision "
                    "FROM spatial.layers l JOIN spatial.data_sources s "
                    "ON s.id=l.source_id AND s.organization_id=l.organization_id "
                    "AND s.project_id=l.project_id "
                    "WHERE l.organization_id=:o AND l.project_id=:p AND l.source_id=:sid "
                    "AND l.lifecycle='active' AND s.lifecycle='active' "
                    "ORDER BY l.id LIMIT 17"
                ),
                {"o": organization_id, "p": project_id, "sid": src_id},
            )
            .mappings()
            .all()
        )
        if not layer_rows:
            continue
        if len(layer_rows) > 16 or str(ev["content_revision"]) != str(
            layer_rows[0]["content_revision"]
        ):
            return {
                "reset_required": True,
                "reason": "source_revision_or_alias_limit",
                "next_cursor": last_cursor,
                "permission_epoch": perm_epoch,
                "has_more": False,
                "changes": [],
            }

        op_rows = (
            session.execute(
                text(
                    "SELECT DISTINCT feature_id, op_type FROM editing.changeset_operations "
                    "WHERE organization_id=:o AND project_id=:p "
                    "AND changeset_id=ANY(CAST(:cids AS uuid[])) LIMIT 5001"
                ),
                {
                    "o": organization_id,
                    "p": project_id,
                    "cids": [item["changeset_id"] for item in group],
                },
            )
            .mappings()
            .all()
        )
        touched_ids = sorted({str(op["feature_id"]) for op in op_rows})
        if len(op_rows) > 5000:
            return {
                "reset_required": True,
                "reason": "delta_operation_budget",
                "next_cursor": last_cursor,
                "permission_epoch": perm_epoch,
                "has_more": False,
                "changes": [],
            }
        if not touched_ids:
            continue

        live_rows = (
            session.execute(
                text(
                    "SELECT f.feature_id, ST_AsGeoJSON(f.geometry, 15, 0) AS geom_json, "
                    "f.attributes AS properties, f.content_revision "
                    "FROM spatial.reference_features f "
                    "WHERE f.organization_id=:o AND f.project_id=:p AND f.source_id=:sid "
                    "AND f.feature_id = ANY(CAST(:fids AS varchar[]))"
                ),
                {
                    "o": organization_id,
                    "p": project_id,
                    "sid": src_id,
                    "fids": touched_ids,
                },
            )
            .mappings()
            .all()
        )
        live_by_id = {str(r["feature_id"]): r for r in live_rows}
        upserts = [
            {
                "feature_id": fid,
                "geometry": json.loads(r["geom_json"]) if r["geom_json"] else None,
                "properties": dict(r["properties"] or {}),
                "content_revision": str(r["content_revision"]),
            }
            for fid, r in live_by_id.items()
        ]
        removals = [fid for fid in touched_ids if fid not in live_by_id]
        delivered_features += len(touched_ids) * len(layer_rows)
        if delivered_features > 5000:
            return {
                "reset_required": True,
                "reason": "delta_delivery_budget",
                "next_cursor": last_cursor,
                "permission_epoch": perm_epoch,
                "has_more": False,
                "changes": [],
            }
        for layer_row in layer_rows:
            changes.append(
                {
                    "cursor": int(ev["cursor"]),
                    "layer_id": str(layer_row["layer_id"]),
                    "source_id": str(src_id),
                    "changeset_id": str(ev["changeset_id"]),
                    "coalesced_changesets": len(group),
                    "content_revision": str(
                        ev["content_revision"] or layer_row["content_revision"]
                    ),
                    "committed_at": ev["committed_at"].isoformat(),
                    "upserts": upserts,
                    "removals": removals,
                }
            )

    if _ensure_counter(session, organization_id, project_id)[:2] != (last_cursor, perm_epoch):
        return {
            "reset_required": True,
            "reason": "concurrent_project_change",
            "next_cursor": last_cursor,
            "permission_epoch": perm_epoch,
            "has_more": False,
            "changes": [],
        }
    session.execute(
        text(
            "UPDATE integration_hub.managed_sessions "
            "SET last_ack_cursor = GREATEST(last_ack_cursor, :cur), last_seen_at = now() "
            "WHERE organization_id=:o AND project_id=:p AND id=:sid"
        ),
        {"o": organization_id, "p": project_id, "sid": session_id, "cur": cursor},
    )
    session.commit()
    return {
        "reset_required": False,
        "next_cursor": scanned_through,
        "permission_epoch": perm_epoch,
        "has_more": has_more,
        "changes": changes,
    }


@router.post(BASE + "/commits", status_code=201)
def commit_managed_changes(
    organization_id: UUID,
    project_id: UUID,
    body: ManagedCommitRequest,
    auth: WriteAuth,
    session: DB,
) -> dict[str, Any]:
    """Server-authoritative commit via R006 changesets returning durable receipt + cursor."""
    ctx = _prep_scope(
        session, auth, organization_id, project_id, Permission.PROJECT_EDIT, mutation=True
    )
    sess_row = _load_active_session(session, organization_id, project_id, body.session_id, auth)
    caps = dict(sess_row["capabilities"] or {})
    if not caps.get("can_edit", False):
        raise HTTPException(403, "Managed session does not have edit capability.")

    has_delete = any(op.op_type == EditOpType.DELETE for op in body.operations)
    if has_delete and not body.allow_deletes:
        raise HTTPException(
            403,
            "Deletions are blocked by default; pass allow_deletes=true with explicit approval.",
        )

    if body.operation_id.strip().startswith("field-sync-"):
        raise HTTPException(422, "Reserved internal operation_id prefix.")

    edit_ops = [
        EditOperationInput(
            op_type=op.op_type,
            feature_id=op.feature_id,
            geometry=op.geometry,
            properties=op.properties,
        )
        for op in body.operations
    ]
    repo = SqlEditingRepository(session)
    cs = repo.commit_changeset(
        context=ctx,
        source_id=body.source_id,
        base_content_revision=body.base_content_revision,
        base_schema_revision=body.base_schema_revision,
        operation_id=body.operation_id,
        summary=body.summary,
        operations=edit_ops,
    )
    # Rebind verified scope after the editing repository commits its transaction.
    _prep_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ, mutation=False)
    ev_row = (
        session.execute(
            text(
                "SELECT cursor FROM integration_hub.project_events "
                "WHERE organization_id=:o AND project_id=:p AND changeset_id=:cid "
                "ORDER BY cursor DESC LIMIT 1"
            ),
            {"o": organization_id, "p": project_id, "cid": cs.id},
        )
        .mappings()
        .first()
    )
    event_cursor = int(ev_row["cursor"]) if ev_row else 0
    return {
        "status": "accepted",
        "changeset_id": str(cs.id),
        "operation_id": cs.operation_id,
        "source_id": str(cs.source_id),
        "resulting_content_revision": cs.resulting_content_revision,
        "operation_count": cs.operation_count,
        "event_cursor": event_cursor,
        "audit_event_id": str(cs.audit_event_id),
        "created_at": cs.created_at.isoformat(),
    }


@router.get(BASE + "/commits/{operation_id}")
def get_managed_commit_receipt(
    organization_id: UUID,
    project_id: UUID,
    operation_id: str,
    auth: ReadAuth,
    session: DB,
    source_id: Annotated[UUID | None, Query()] = None,
) -> dict[str, Any]:
    """Recover durable receipt for an uncertain commit using scoped operation_id."""
    _prep_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ, mutation=False)
    matches = (
        session.execute(
            text(
                "SELECT c.id, c.source_id, c.operation_id, c.resulting_content_revision, "
                "c.operation_count, c.audit_event_id, c.created_at, e.cursor "
                "FROM editing.changesets c "
                "LEFT JOIN integration_hub.project_events e "
                "  ON e.organization_id=c.organization_id AND e.project_id=c.project_id "
                "  AND e.changeset_id=c.id "
                "WHERE c.organization_id=:o AND c.project_id=:p "
                "  AND c.operation_id=:op_id AND c.actor_id=:aid AND c.actor_type=:atype "
                "  AND c.source_id=COALESCE(CAST(:src AS uuid), c.source_id) "
                "ORDER BY c.created_at DESC LIMIT 2"
            ),
            {
                "o": organization_id,
                "p": project_id,
                "op_id": operation_id.strip(),
                "src": source_id,
                "aid": auth.actor.actor_id,
                "atype": auth.actor.actor_type,
            },
        )
        .mappings()
        .all()
    )
    if not matches:
        raise HTTPException(404, "Commit receipt not found for this operation_id.")
    if len(matches) > 1:
        raise HTTPException(409, "Multiple commit receipts match this operation_id.")
    row = matches[0]
    return {
        "status": "accepted",
        "changeset_id": str(row["id"]),
        "operation_id": str(row["operation_id"]),
        "source_id": str(row["source_id"]),
        "resulting_content_revision": str(row["resulting_content_revision"]),
        "operation_count": int(row["operation_count"]),
        "event_cursor": int(row["cursor"]) if row["cursor"] is not None else 0,
        "audit_event_id": str(row["audit_event_id"]),
        "created_at": row["created_at"].isoformat(),
    }


def _session_still_valid(request: Request, org: UUID, proj: UUID, session_id: UUID) -> bool:
    try:
        with get_session_factory()() as session:
            auth = authenticate_request(request, session, get_settings())
            _prep_scope(session, auth, org, proj, Permission.PROJECT_READ, mutation=False)
            _load_active_session(session, org, proj, session_id, auth)
            return True
    except Exception:
        return False


@router.get(BASE + "/events")
async def stream_managed_events(
    organization_id: UUID,
    project_id: UUID,
    request: Request,
    auth: ReadAuth,
    session: DB,
    session_id: Annotated[UUID, Query()],
) -> StreamingResponse:
    """SSE wake-up stream with 15s bounded session/revocation recheck."""
    _prep_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ, mutation=False)
    _load_active_session(session, organization_id, project_id, session_id, auth)
    dsn = (
        make_url(get_settings().runtime_db_url)
        .set(drivername="postgresql")
        .render_as_string(hide_password=False)
    )
    org_s, proj_s = str(organization_id), str(project_id)

    async def gen() -> AsyncIterator[str]:
        yield "retry: 3000\n\n"
        conn = await psycopg.AsyncConnection.connect(dsn, autocommit=True)
        try:
            async with conn.cursor() as cur:
                await cur.execute("LISTEN ovg_team_collaboration")
            last_hb = time.monotonic()
            while not await request.is_disconnected():
                note = None
                try:
                    async for n in conn.notifies(timeout=5.0, stop_after=1):
                        note = n
                except TimeoutError:
                    note = None
                if note is not None:
                    try:
                        p = json.loads(note.payload)
                    except ValueError:
                        p = {}
                    if isinstance(p, dict) and p.get("o") == org_s and p.get("p") == proj_s:
                        kind = str(p.get("kind") or "spatial_change")
                        if not await asyncio.to_thread(
                            _session_still_valid, request, organization_id, project_id, session_id
                        ):
                            yield 'event: session_revoked\ndata: {"revoked":true}\n\n'
                            return
                        # A valid session must ignore another session's revoke notice.
                        if kind in {"spatial_change", "desktop_command"}:
                            yield f"event: {kind}\ndata: {json.dumps({'kind': kind})}\n\n"
                if time.monotonic() - last_hb >= 15.0:
                    if not await asyncio.to_thread(
                        _session_still_valid, request, organization_id, project_id, session_id
                    ):
                        yield 'event: session_revoked\ndata: {"revoked":true}\n\n'
                        return
                    yield 'event: heartbeat\ndata: {"ok":true}\n\n'
                    last_hb = time.monotonic()
        except asyncio.CancelledError:
            raise
        finally:
            await conn.close()

    return StreamingResponse(
        gen(),
        media_type="text/event-stream",
        headers={"Cache-Control": "no-cache", "X-Accel-Buffering": "no"},
    )


# --- P2 AI Desktop Bridge (Allowlisted Typed Commands + Explicit Consent) ---


@router.post(BASE + "/desktop-sessions/{session_id}/commands", status_code=201)
def enqueue_desktop_command(
    organization_id: UUID,
    project_id: UUID,
    session_id: UUID,
    body: CreateDesktopCommandRequest,
    auth: WriteAuth,
    session: DB,
) -> dict[str, Any]:
    """Enqueue an allowlisted typed command for an active opted-in QGIS session."""
    ctx = _prep_scope(
        session, auth, organization_id, project_id, Permission.PROJECT_READ, mutation=False
    )
    if body.tool_name not in ALLOWED_DESKTOP_TOOLS:
        raise HTTPException(
            403,
            f"Tool '{body.tool_name}' is not in the R029 desktop allowlist.",
        )
    if body.tool_name in MUTATING_DESKTOP_TOOLS:
        prepare_project_scope(session, ctx, permission=Permission.PROJECT_EDIT, mutation=True)
    _load_active_session(session, organization_id, project_id, session_id, auth, allow_manager=True)
    now = datetime.now(UTC)
    cmd_id = uuid4()
    expires_at = now + timedelta(seconds=body.ttl_seconds)
    requires_consent = True
    row = (
        session.execute(
            text(
                "INSERT INTO integration_hub.desktop_commands ("
                "id, organization_id, project_id, session_id, tool_name, arguments, "
                "requires_consent, status, requested_by, expires_at, created_at"
                ") VALUES ("
                ":id, :o, :p, :sid, :tool, CAST(:args AS jsonb), "
                ":consent, 'pending', :req_by, :exp, :now"
                ") RETURNING *"
            ),
            {
                "id": cmd_id,
                "o": organization_id,
                "p": project_id,
                "sid": session_id,
                "tool": body.tool_name,
                "args": json.dumps(body.arguments),
                "consent": requires_consent,
                "req_by": auth.actor.actor_id,
                "exp": expires_at,
                "now": now,
            },
        )
        .mappings()
        .one()
    )
    _notify_project_channel(session, organization_id, project_id, "desktop_command")
    append_project_audit(
        session,
        ctx,
        audit_id=uuid4(),
        action="integration.desktop_command.enqueued",
        resource_type="desktop_command",
        resource_id=cmd_id,
        metadata={"session_id": str(session_id), "tool_name": body.tool_name},
        created_at=now,
    )
    session.commit()
    return _command_wire(row)


def _command_wire(row: RowMapping) -> dict[str, Any]:
    return {
        "id": str(row["id"]),
        "session_id": str(row["session_id"]),
        "tool_name": str(row["tool_name"]),
        "arguments": dict(row["arguments"] or {}),
        "requires_consent": bool(row["requires_consent"]),
        "status": str(row["status"]),
        "result": dict(row["result"]) if row["result"] is not None else None,
        "expires_at": row["expires_at"].isoformat(),
        "created_at": row["created_at"].isoformat(),
        "resolved_at": row["resolved_at"].isoformat() if row["resolved_at"] else None,
    }


@router.get(BASE + "/desktop-sessions/{session_id}/commands")
def list_desktop_commands(
    organization_id: UUID,
    project_id: UUID,
    session_id: UUID,
    auth: ReadAuth,
    session: DB,
    status: Annotated[str | None, Query()] = None,
) -> dict[str, Any]:
    """List pending or recent allowlisted commands for a managed QGIS session."""
    _prep_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ, mutation=False)
    _load_active_session(session, organization_id, project_id, session_id, auth)
    sql = (
        "SELECT * FROM integration_hub.desktop_commands "
        "WHERE organization_id=:o AND project_id=:p AND session_id=:sid "
    )
    params: dict[str, Any] = {"o": organization_id, "p": project_id, "sid": session_id}
    if status:
        sql += "AND status=:st "
        params["st"] = status
    sql += "ORDER BY created_at DESC LIMIT 50"
    rows = session.execute(text(sql), params).mappings().all()
    return {"items": [_command_wire(r) for r in rows]}


@router.post(BASE + "/desktop-sessions/{session_id}/commands/{command_id}/respond")
def respond_desktop_command(
    organization_id: UUID,
    project_id: UUID,
    session_id: UUID,
    command_id: UUID,
    body: RespondDesktopCommandRequest,
    auth: WriteAuth,
    session: DB,
) -> dict[str, Any]:
    """Record user consent decision and bounded command result from QGIS."""
    _prep_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ, mutation=False)
    _load_active_session(session, organization_id, project_id, session_id, auth)
    now = datetime.now(UTC)
    row = (
        session.execute(
            text(
                "UPDATE integration_hub.desktop_commands "
                "SET status=:st, result=CAST(:res AS jsonb), resolved_at=:now "
                "WHERE organization_id=:o AND project_id=:p AND session_id=:sid AND id=:cid "
                "  AND status IN ('pending', 'approved') AND expires_at > :now "
                "RETURNING *"
            ),
            {
                "o": organization_id,
                "p": project_id,
                "sid": session_id,
                "cid": command_id,
                "st": body.status,
                "res": json.dumps(body.result) if body.result is not None else None,
                "now": now,
            },
        )
        .mappings()
        .first()
    )
    if row is None:
        raise HTTPException(409, "Command not found, already resolved, or expired.")
    session.commit()
    return _command_wire(row)
