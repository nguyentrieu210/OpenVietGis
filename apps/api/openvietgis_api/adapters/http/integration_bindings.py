"""R027 project-scoped QGIS bindings, pairing tokens (Pha A), and conflict ledger (Pha B).

- Initial POST /bindings always creates a disabled 'draft' binding (writes_allowed=False).
- Explicit POST /bindings/{id}/activate activates a draft/paused binding (manager + CSRF).
- Explicit POST /bindings/{id}/pairing-tokens issues a one-time scoped QGIS pairing code
  backed by a project-scoped Service Principal credential (project.read + project.edit,
  never delete, immediately revocable).
- GET/POST /bindings/{id}/conflicts and POST /bindings/{id}/conflicts/{cid}/resolve
  provide per-feature conflict review and resolution (keep_web / keep_qgis / merge / dismiss).
"""

from __future__ import annotations

import base64
import hashlib
import json
import re
from datetime import UTC, datetime, timedelta
from typing import Annotated, Any, Literal
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, HTTPException, Request, Response
from pydantic import BaseModel, ConfigDict, Field, StringConstraints, field_validator
from sqlalchemy import text
from sqlalchemy.engine import RowMapping
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence import gis_access
from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.authorization_repository import (
    SqlResourceGrantRepository,
    SqlServicePrincipalRepository,
)
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.adapters.persistence.tenancy_repository import SqlTenancyRepository
from openvietgis_api.domains.authorization.domain import ResourceRole, ScopeKind
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.service import ServicePrincipalService
from openvietgis_api.domains.integration_hub.profiles import is_explicit_readable_crs

router = APIRouter(tags=["integration-bindings"])
BASE = "/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings"
ReadAuth = Annotated[AuthenticatedRequest, Depends(authenticate_request)]
WriteAuth = Annotated[AuthenticatedRequest, Depends(require_csrf)]
DB = Annotated[Session, Depends(get_db_session, scope="function")]

_FIELDS_PATTERN = re.compile(r"^[A-Za-z][A-Za-z0-9_]{0,63}$")


class CreateBinding(BaseModel):
    model_config = ConfigDict(extra="forbid")
    layer_id: UUID
    adapter_key: Literal["qgis"] = "qgis"
    external_dataset_ref: Annotated[
        str,
        StringConstraints(
            strip_whitespace=True,
            min_length=1,
            max_length=128,
            pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$",
        ),
    ]
    direction: Literal["bidirectional_manual"] = "bidirectional_manual"
    source_crs_code: int = Field(ge=1, le=999999)
    target_crs_code: int = Field(ge=1, le=999999)
    external_primary_key: Literal["ovg_feature_id"] = "ovg_feature_id"
    allowed_field_ids: list[str] = Field(min_length=1, max_length=100)

    @field_validator("allowed_field_ids")
    @classmethod
    def validate_field_ids(cls, value: list[str]) -> list[str]:
        if len(set(value)) != len(value):
            raise ValueError("Duplicate allowed_field_ids.")
        if any(not _FIELDS_PATTERN.fullmatch(field) for field in value):
            raise ValueError("Invalid source field ID.")
        return sorted(value)


class ChangeLifecycle(BaseModel):
    model_config = ConfigDict(extra="forbid")
    action: Literal["pause", "revoke"]
    expected_revision: int = Field(ge=1)


class ActivateBinding(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: int = Field(ge=1)


class IssuePairingToken(BaseModel):
    model_config = ConfigDict(extra="forbid")
    label: Annotated[
        str, StringConstraints(strip_whitespace=True, min_length=1, max_length=120)
    ] = "QGIS Desktop"
    expires_in_days: int = Field(default=30, ge=1, le=365)


class RecordConflictItem(BaseModel):
    model_config = ConfigDict(extra="forbid")
    feature_id: Annotated[
        str, StringConstraints(strip_whitespace=True, min_length=1, max_length=128)
    ]
    conflict_kind: Literal["attribute", "geometry", "delete_candidate"] = "attribute"
    field_name: str | None = Field(default=None, max_length=128)
    baseline_value: Any = None
    web_value: Any = None
    qgis_value: Any = None


class RecordConflictsRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    items: list[RecordConflictItem] = Field(min_length=1, max_length=100)


class ResolveConflictRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    decision: Literal["keep_web", "keep_qgis", "merge", "dismiss"]
    merged_value: Any = None
    note: str | None = Field(default=None, max_length=500)


_LAYER_SQL = text("""
    SELECT l.source_id, s.geometry_family, s.dimensions, s.validation_state,
           s.stored_crs, s.schema_revision, s.content_revision
    FROM spatial.layers l
    JOIN spatial.data_sources s
      ON s.organization_id=l.organization_id
     AND s.project_id=l.project_id AND s.id=l.source_id
    WHERE l.organization_id=:org AND l.project_id=:proj
      AND l.id=:lid AND l.lifecycle='active' AND s.lifecycle='active'
    FOR SHARE OF l, s
""")
_CANDIDATES_SQL = text("""
    SELECT l.id AS layer_id, l.name AS layer_name, l.source_id,
           s.stored_crs, s.geometry_family, s.dimensions,
           s.validation_state
    FROM spatial.layers l
    JOIN spatial.data_sources s
      ON s.organization_id=l.organization_id
     AND s.project_id=l.project_id AND s.id=l.source_id
    WHERE l.organization_id=:org AND l.project_id=:proj
      AND l.lifecycle='active' AND s.lifecycle='active'
    ORDER BY l.name, l.id
    LIMIT 200
""")
_FIELDS_SQL = text("""
    SELECT field_id FROM spatial.source_fields
    WHERE organization_id=:org AND project_id=:proj AND source_id=:sid
""")
_VIEW_COLS = """
    id, layer_id, adapter_key, external_dataset_ref, direction,
    source_crs_code, target_crs_code, external_primary_key,
    allowed_field_ids, base_content_revision, base_schema_revision,
    state, revision, created_at, updated_at, activated_at, last_paired_at,
    service_principal_id
"""
_SELECT = text(f"""
    SELECT {_VIEW_COLS} FROM integration_hub.connector_bindings
    WHERE organization_id=:org AND project_id=:proj
    ORDER BY created_at DESC, id DESC LIMIT 200
""")
_SELECT_ONE = text(f"""
    SELECT {_VIEW_COLS} FROM integration_hub.connector_bindings
    WHERE organization_id=:org AND project_id=:proj AND id=:id
""")
_INSERT = text(f"""
    INSERT INTO integration_hub.connector_bindings
      (id, organization_id, project_id, layer_id, adapter_key,
       external_dataset_ref, direction, source_crs_code, target_crs_code,
       external_primary_key, allowed_field_ids, base_content_revision,
       base_schema_revision, created_by)
    VALUES (:id, :org, :proj, :lid, :adapter, :dataset, :direction,
            :source_crs, :target_crs, :primary_key, CAST(:fields AS jsonb),
            :content_rev, :schema_rev, :actor)
    ON CONFLICT (organization_id, project_id, layer_id, adapter_key, external_dataset_ref)
      WHERE state <> 'revoked' DO NOTHING
    RETURNING {_VIEW_COLS}
""")
_STATE = text(f"""
    UPDATE integration_hub.connector_bindings
    SET state=:state, revision=revision+1, updated_at=now()
    WHERE organization_id=:org AND project_id=:proj
      AND id=:id AND revision=:expected
      AND state <> 'revoked'
    RETURNING {_VIEW_COLS}
""")
_ACTIVATE = text(f"""
    UPDATE integration_hub.connector_bindings
    SET state='active',
        revision=revision+1,
        activated_at=COALESCE(activated_at, now()),
        updated_at=now()
    WHERE organization_id=:org AND project_id=:proj
      AND id=:id AND revision=:expected
      AND state IN ('draft', 'paused')
    RETURNING {_VIEW_COLS}
""")
_MARK_PAIRED = text(f"""
    UPDATE integration_hub.connector_bindings
    SET state='active',
        revision=CASE WHEN state <> 'active' THEN revision+1 ELSE revision END,
        activated_at=COALESCE(activated_at, now()),
        last_paired_at=now(),
        service_principal_id=COALESCE(service_principal_id, :sp_id),
        updated_at=now()
    WHERE organization_id=:org AND project_id=:proj
      AND id=:id AND state <> 'revoked'
    RETURNING {_VIEW_COLS}
""")
_INSERT_TOKEN = text("""
    INSERT INTO integration_hub.binding_pairing_tokens
      (id, organization_id, project_id, binding_id, label,
       token_prefix, token_hash, service_principal_id, service_credential_id,
       expires_at, created_by)
    VALUES (:id, :org, :proj, :bid, :label,
            :prefix, :token_hash, :sp_id, :cred_id,
            :expires_at, :actor)
    RETURNING id, binding_id, label, token_prefix, expires_at,
              last_used_at, revoked_at, created_at
""")
_LIST_TOKENS = text("""
    SELECT id, binding_id, label, token_prefix, expires_at,
           last_used_at, revoked_at, created_at
    FROM integration_hub.binding_pairing_tokens
    WHERE organization_id=:org AND project_id=:proj AND binding_id=:bid
    ORDER BY created_at DESC LIMIT 50
""")
_GET_TOKEN = text("""
    SELECT id, service_principal_id, service_credential_id, revoked_at
    FROM integration_hub.binding_pairing_tokens
    WHERE organization_id=:org AND project_id=:proj AND binding_id=:bid AND id=:tid
""")
_REVOKE_TOKEN = text("""
    UPDATE integration_hub.binding_pairing_tokens
    SET revoked_at=now()
    WHERE organization_id=:org AND project_id=:proj
      AND binding_id=:bid AND id=:tid AND revoked_at IS NULL
    RETURNING id, binding_id, label, token_prefix, expires_at,
              last_used_at, revoked_at, created_at
""")
_LIST_ACTIVE_TOKENS = text("""
    SELECT id, service_principal_id, service_credential_id
    FROM integration_hub.binding_pairing_tokens
    WHERE organization_id=:org AND project_id=:proj
      AND binding_id=:bid AND revoked_at IS NULL
""")
_REVOKE_ALL_TOKENS = text("""
    UPDATE integration_hub.binding_pairing_tokens
    SET revoked_at=now()
    WHERE organization_id=:org AND project_id=:proj
      AND binding_id=:bid AND revoked_at IS NULL
""")
_LIST_CONFLICTS = text("""
    SELECT id, binding_id, layer_id, feature_id, conflict_kind, field_name,
           baseline_value, web_value, qgis_value, status,
           resolved_value, resolution_note, created_at, resolved_at
    FROM integration_hub.sync_conflicts
    WHERE organization_id=:org AND project_id=:proj AND binding_id=:bid
    ORDER BY CASE WHEN status = 'open' THEN 0 ELSE 1 END, created_at DESC
    LIMIT 200
""")
_INSERT_CONFLICT = text("""
    INSERT INTO integration_hub.sync_conflicts
      (id, organization_id, project_id, binding_id, layer_id, feature_id,
       conflict_kind, field_name, baseline_value, web_value, qgis_value, created_by)
    VALUES (:id, :org, :proj, :bid, :lid, :fid,
            :kind, :field_name, CAST(:baseline AS jsonb), CAST(:web AS jsonb),
            CAST(:qgis AS jsonb), :actor)
    RETURNING id, binding_id, layer_id, feature_id, conflict_kind, field_name,
              baseline_value, web_value, qgis_value, status,
              resolved_value, resolution_note, created_at, resolved_at
""")
_GET_CONFLICT = text("""
    SELECT id, binding_id, layer_id, feature_id, conflict_kind, field_name,
           baseline_value, web_value, qgis_value, status
    FROM integration_hub.sync_conflicts
    WHERE organization_id=:org AND project_id=:proj AND binding_id=:bid AND id=:cid
""")
_RESOLVE_CONFLICT = text("""
    UPDATE integration_hub.sync_conflicts
    SET status=:status,
        resolved_value=CAST(:resolved_value AS jsonb),
        resolution_note=:note,
        resolved_by=:actor,
        resolved_at=now()
    WHERE organization_id=:org AND project_id=:proj
      AND binding_id=:bid AND id=:cid AND status='open'
    RETURNING id, binding_id, layer_id, feature_id, conflict_kind, field_name,
              baseline_value, web_value, qgis_value, status,
              resolved_value, resolution_note, created_at, resolved_at
""")
_COUNT_OPEN_CONFLICTS = text("""
    SELECT binding_id, COUNT(*) AS open_count
    FROM integration_hub.sync_conflicts
    WHERE organization_id=:org AND project_id=:proj AND status='open'
    GROUP BY binding_id
""")


def _sp_service(session: Session) -> ServicePrincipalService:
    return ServicePrincipalService(
        SqlServicePrincipalRepository(session),
        SqlResourceGrantRepository(session),
        SqlTenancyRepository(session),
        SqlAuditRepository(session),
    )


def _scope(
    session: Session,
    auth: AuthenticatedRequest,
    org: UUID,
    proj: UUID,
    *,
    mutation: bool,
):
    if auth.actor.actor_type != "human":
        raise HTTPException(status_code=403, detail="Project manager session required.")
    context = context_for(auth, organization_id=org, project_id=proj)
    prepare_project_scope(
        session, context, permission=Permission.PROJECT_MANAGE, mutation=mutation
    )
    if gis_access.scoped(session, context):
        raise HTTPException(status_code=403, detail="Full project access required.")
    return context


def _public(row: RowMapping, open_conflicts: int = 0) -> dict[str, object]:
    out: dict[str, object] = dict(row)
    out["id"] = str(out["id"])
    out["layer_id"] = str(out["layer_id"])
    out.pop("service_principal_id", None)
    raw_fields = out.get("allowed_field_ids")
    if isinstance(raw_fields, str):
        out["allowed_field_ids"] = json.loads(raw_fields)
    is_active = str(out.get("state")) == "active"
    is_paired = is_active and out.get("last_paired_at") is not None
    out["writes_allowed"] = is_active
    out["deletes_allowed"] = False
    out["connected"] = is_paired
    out["open_conflicts_count"] = open_conflicts
    out["reason"] = (
        "ACTIVE_PAIRED_MANUAL_SYNC"
        if is_paired
        else "ACTIVE_AWAITING_QGIS_PAIRING"
        if is_active
        else "DRAFT_ONLY_AGENT_NOT_PAIRED"
    )
    return out


def _public_token(row: RowMapping) -> dict[str, object]:
    out: dict[str, object] = dict(row)
    out["id"] = str(out["id"])
    out["binding_id"] = str(out["binding_id"])
    return out


def _public_conflict(row: RowMapping) -> dict[str, object]:
    out: dict[str, object] = dict(row)
    out["id"] = str(out["id"])
    out["binding_id"] = str(out["binding_id"])
    out["layer_id"] = str(out["layer_id"])
    return out


def _build_pairing_code(
    *,
    server_url: str,
    organization_id: UUID,
    project_id: UUID,
    binding_id: UUID,
    layer_id: UUID,
    external_dataset_ref: str,
    bearer_token: str,
) -> str:
    payload = json.dumps(
        {
            "v": 1,
            "server_url": server_url.rstrip("/"),
            "organization_id": str(organization_id),
            "project_id": str(project_id),
            "binding_id": str(binding_id),
            "layer_id": str(layer_id),
            "external_dataset_ref": external_dataset_ref,
            "bearer_token": bearer_token,
        },
        separators=(",", ":"),
    ).encode("utf-8")
    encoded = base64.urlsafe_b64encode(payload).decode("ascii").rstrip("=")
    return f"ovg-pair-v1:{encoded}"


@router.get(BASE + "/candidates")
def list_binding_candidates(
    organization_id: UUID, project_id: UUID, auth: ReadAuth, session: DB
) -> dict[str, object]:
    """Expose project-manager-only layer/field choices for QGIS bindings."""
    _scope(session, auth, organization_id, project_id, mutation=False)
    scope = {"org": organization_id, "proj": project_id}
    rows = session.execute(_CANDIDATES_SQL, scope).mappings().all()
    items: list[dict[str, object]] = []
    for row in rows:
        if (
            str(row["validation_state"]) != "ready"
            or str(row["geometry_family"]) not in ("Point", "LineString", "Polygon")
            or str(row["dimensions"]) != "XY"
            or not is_explicit_readable_crs(row["stored_crs"], str(row["validation_state"]))
        ):
            continue
        fields = session.execute(
            _FIELDS_SQL, {**scope, "sid": row["source_id"]}
        ).mappings().all()
        allowed_fields = sorted(
            str(field["field_id"])
            for field in fields
            if _FIELDS_PATTERN.fullmatch(str(field["field_id"]))
        )[:100]
        if not allowed_fields:
            continue
        items.append({
            "layer_id": str(row["layer_id"]),
            "layer_name": str(row["layer_name"]),
            "stored_crs_code": int(row["stored_crs"]),
            "geometry_family": str(row["geometry_family"]),
            "allowed_field_ids": allowed_fields,
            "binding_status": "draft_only",
        })
    return {"items": items}


@router.get(BASE)
def list_bindings(
    organization_id: UUID, project_id: UUID, auth: ReadAuth, session: DB
) -> dict[str, object]:
    _scope(session, auth, organization_id, project_id, mutation=False)
    rows = session.execute(_SELECT, {"org": organization_id, "proj": project_id}).mappings().all()
    conflict_rows = session.execute(
        _COUNT_OPEN_CONFLICTS, {"org": organization_id, "proj": project_id}
    ).mappings().all()
    conflict_map = {str(r["binding_id"]): int(r["open_count"]) for r in conflict_rows}
    return {"items": [_public(row, conflict_map.get(str(row["id"]), 0)) for row in rows]}


@router.post(BASE, status_code=201)
def create_binding(
    organization_id: UUID,
    project_id: UUID,
    body: CreateBinding,
    auth: WriteAuth,
    session: DB,
) -> dict[str, object]:
    context = _scope(session, auth, organization_id, project_id, mutation=True)
    args = {"org": organization_id, "proj": project_id, "lid": body.layer_id}
    layer = session.execute(_LAYER_SQL, args).mappings().first()
    if not layer:
        raise HTTPException(status_code=404, detail="Layer not found.")
    if str(layer["validation_state"]) != "ready" or str(layer["geometry_family"]) not in (
        "Point",
        "LineString",
        "Polygon",
    ) or str(layer["dimensions"]) != "XY":
        raise HTTPException(422, "Only verified single-part 2D layers can be bound.")
    if not is_explicit_readable_crs(layer["stored_crs"], str(layer["validation_state"])):
        raise HTTPException(422, "Layer CRS must be verified and readable.")
    if (
        body.source_crs_code != layer["stored_crs"]
        or body.target_crs_code != layer["stored_crs"]
    ):
        raise HTTPException(
            422, "MVP requires identical verified EPSG CRS; no silent reprojection."
        )
    field_rows = session.execute(
        _FIELDS_SQL, {**args, "sid": layer["source_id"]}
    ).mappings().all()
    present_fields = {str(row["field_id"]) for row in field_rows}
    if not set(body.allowed_field_ids) <= present_fields:
        raise HTTPException(422, "Only known source field IDs may be mapped.")
    binding_id = uuid4()
    params = {
        **args,
        "id": binding_id,
        "adapter": "qgis",
        "dataset": body.external_dataset_ref,
        "direction": body.direction,
        "source_crs": body.source_crs_code,
        "target_crs": body.target_crs_code,
        "primary_key": body.external_primary_key,
        "fields": json.dumps(body.allowed_field_ids),
        "content_rev": str(layer["content_revision"]),
        "schema_rev": str(layer["schema_revision"]),
        "actor": context.actor.actor_id,
    }
    result = session.execute(_INSERT, params).mappings().one_or_none()
    if result is None:
        raise HTTPException(409, "Binding already exists; use or revoke the existing draft.")
    append_project_audit(
        session,
        context,
        audit_id=uuid4(),
        action="integration.binding.draft_created",
        resource_type="connector_binding",
        resource_id=binding_id,
        metadata={"adapter": "qgis", "layer_id": str(body.layer_id), "writes_allowed": False},
        created_at=datetime.now(UTC),
    )
    return _public(result)


@router.post(BASE + "/{binding_id}/activate")
def activate_binding(
    organization_id: UUID,
    project_id: UUID,
    binding_id: UUID,
    body: ActivateBinding,
    auth: WriteAuth,
    session: DB,
) -> dict[str, object]:
    """Explicitly activate a draft or paused QGIS binding for manual two-way sync."""
    context = _scope(session, auth, organization_id, project_id, mutation=True)
    result = session.execute(
        _ACTIVATE,
        {
            "org": organization_id,
            "proj": project_id,
            "id": binding_id,
            "expected": body.expected_revision,
        },
    ).mappings().first()
    if result is None:
        raise HTTPException(
            409, "Binding unavailable, already active/revoked, or revision changed."
        )
    append_project_audit(
        session,
        context,
        audit_id=uuid4(),
        action="integration.binding.activated",
        resource_type="connector_binding",
        resource_id=binding_id,
        metadata={"state": "active", "revision": int(result["revision"])},
        created_at=datetime.now(UTC),
    )
    return _public(result)


@router.post(BASE + "/{binding_id}/lifecycle")
def change_binding_lifecycle(
    organization_id: UUID,
    project_id: UUID,
    binding_id: UUID,
    body: ChangeLifecycle,
    auth: WriteAuth,
    session: DB,
) -> dict[str, object]:
    context = _scope(session, auth, organization_id, project_id, mutation=True)
    result = session.execute(
        _STATE,
        {
            "org": organization_id,
            "proj": project_id,
            "id": binding_id,
            "expected": body.expected_revision,
            "state": "paused" if body.action == "pause" else "revoked",
        },
    ).mappings().first()
    if result is None:
        raise HTTPException(
            409, "Binding unavailable or revision changed; reload binding state."
        )
    if body.action == "revoke":
        active_tokens = session.execute(
            _LIST_ACTIVE_TOKENS, {"org": organization_id, "proj": project_id, "bid": binding_id}
        ).mappings().all()
        sp_svc = _sp_service(session)
        for tok in active_tokens:
            sp_svc.revoke_credential(
                actor=auth.actor,
                organization_id=organization_id,
                service_principal_id=tok["service_principal_id"],
                credential_id=tok["service_credential_id"],
            )
        session.execute(
            _REVOKE_ALL_TOKENS, {"org": organization_id, "proj": project_id, "bid": binding_id}
        )
    append_project_audit(
        session,
        context,
        audit_id=uuid4(),
        action=f"integration.binding.{body.action}",
        resource_type="connector_binding",
        resource_id=binding_id,
        metadata={"state": str(result["state"]), "revision": int(result["revision"])},
        created_at=datetime.now(UTC),
    )
    return _public(result)


@router.get(BASE + "/{binding_id}/pairing-tokens")
def list_pairing_tokens(
    organization_id: UUID,
    project_id: UUID,
    binding_id: UUID,
    response: Response,
    auth: ReadAuth,
    session: DB,
) -> dict[str, object]:
    _scope(session, auth, organization_id, project_id, mutation=False)
    response.headers["Cache-Control"] = "no-store"
    rows = session.execute(
        _LIST_TOKENS, {"org": organization_id, "proj": project_id, "bid": binding_id}
    ).mappings().all()
    return {"items": [_public_token(r) for r in rows]}


@router.post(BASE + "/{binding_id}/pairing-tokens", status_code=201)
def issue_pairing_token(
    organization_id: UUID,
    project_id: UUID,
    binding_id: UUID,
    body: IssuePairingToken,
    request: Request,
    response: Response,
    auth: WriteAuth,
    session: DB,
) -> dict[str, object]:
    """Issue a one-time QGIS Pairing Code backed by a project-scoped Service Credential."""
    context = _scope(session, auth, organization_id, project_id, mutation=True)
    binding = session.execute(
        _SELECT_ONE, {"org": organization_id, "proj": project_id, "id": binding_id}
    ).mappings().first()
    if binding is None:
        raise HTTPException(404, "Binding not found.")
    if str(binding["state"]) == "revoked":
        raise HTTPException(409, "Cannot pair a revoked binding.")

    sp_svc = _sp_service(session)
    sp_id: UUID | None = binding["service_principal_id"]
    if sp_id is None:
        principal = sp_svc.create_principal(
            actor=auth.actor,
            organization_id=organization_id,
            name=f"qgis-binding-{str(binding_id)[:8]}-{uuid4().hex[:4]}",
            description=f"R027 QGIS connector binding for {binding['external_dataset_ref']}",
        )
        sp_id = principal.id
        sp_svc.create_service_grant(
            actor=auth.actor,
            organization_id=organization_id,
            service_principal_id=sp_id,
            scope_kind=ScopeKind.PROJECT,
            workspace_id=None,
            project_id=project_id,
            role=ResourceRole.EDITOR,
        )

    expires_at = datetime.now(UTC) + timedelta(days=body.expires_in_days)
    credential, secret = sp_svc.issue_credential(
        actor=auth.actor,
        organization_id=organization_id,
        service_principal_id=sp_id,
        name=f"QGIS · {body.label}"[:200],
        permission_ceiling=frozenset({Permission.PROJECT_READ, Permission.PROJECT_EDIT}),
        expires_at=expires_at,
    )
    token_id = uuid4()
    token_hash = hashlib.sha256(secret.encode("utf-8")).hexdigest()
    token_row = session.execute(
        _INSERT_TOKEN,
        {
            "id": token_id,
            "org": organization_id,
            "proj": project_id,
            "bid": binding_id,
            "label": body.label,
            "prefix": credential.prefix,
            "token_hash": token_hash,
            "sp_id": sp_id,
            "cred_id": credential.id,
            "expires_at": expires_at,
            "actor": context.actor.actor_id,
        },
    ).mappings().one()
    updated_binding = session.execute(
        _MARK_PAIRED,
        {"org": organization_id, "proj": project_id, "id": binding_id, "sp_id": sp_id},
    ).mappings().one()

    base_url = str(request.base_url).rstrip("/")
    pairing_code = _build_pairing_code(
        server_url=base_url,
        organization_id=organization_id,
        project_id=project_id,
        binding_id=binding_id,
        layer_id=binding["layer_id"],
        external_dataset_ref=str(binding["external_dataset_ref"]),
        bearer_token=secret,
    )
    append_project_audit(
        session,
        context,
        audit_id=uuid4(),
        action="integration.binding.pairing_token_issued",
        resource_type="connector_binding",
        resource_id=binding_id,
        metadata={
            "token_id": str(token_id),
            "label": body.label,
            "expires_in_days": body.expires_in_days,
        },
        created_at=datetime.now(UTC),
    )
    response.headers["Cache-Control"] = "no-store"
    return {
        "token": _public_token(token_row),
        "binding": _public(updated_binding),
        "pairing_code": pairing_code,
        "bearer_token": secret,
    }


@router.post(BASE + "/{binding_id}/pairing-tokens/{token_id}/revoke")
def revoke_pairing_token(
    organization_id: UUID,
    project_id: UUID,
    binding_id: UUID,
    token_id: UUID,
    auth: WriteAuth,
    session: DB,
) -> dict[str, object]:
    context = _scope(session, auth, organization_id, project_id, mutation=True)
    tok = session.execute(
        _GET_TOKEN,
        {"org": organization_id, "proj": project_id, "bid": binding_id, "tid": token_id},
    ).mappings().first()
    if tok is None:
        raise HTTPException(404, "Pairing token not found.")
    if tok["revoked_at"] is None:
        _sp_service(session).revoke_credential(
            actor=auth.actor,
            organization_id=organization_id,
            service_principal_id=tok["service_principal_id"],
            credential_id=tok["service_credential_id"],
        )
    revoked = session.execute(
        _REVOKE_TOKEN,
        {"org": organization_id, "proj": project_id, "bid": binding_id, "tid": token_id},
    ).mappings().first()
    if revoked is None:
        raise HTTPException(409, "Pairing token already revoked.")
    append_project_audit(
        session,
        context,
        audit_id=uuid4(),
        action="integration.binding.pairing_token_revoked",
        resource_type="connector_binding",
        resource_id=binding_id,
        metadata={"token_id": str(token_id)},
        created_at=datetime.now(UTC),
    )
    return _public_token(revoked)


@router.get(BASE + "/{binding_id}/conflicts")
def list_binding_conflicts(
    organization_id: UUID,
    project_id: UUID,
    binding_id: UUID,
    auth: ReadAuth,
    session: DB,
) -> dict[str, object]:
    _scope(session, auth, organization_id, project_id, mutation=False)
    rows = session.execute(
        _LIST_CONFLICTS, {"org": organization_id, "proj": project_id, "bid": binding_id}
    ).mappings().all()
    return {"items": [_public_conflict(r) for r in rows]}


@router.post(BASE + "/{binding_id}/conflicts", status_code=201)
def record_binding_conflicts(
    organization_id: UUID,
    project_id: UUID,
    binding_id: UUID,
    body: RecordConflictsRequest,
    auth: ReadAuth,
    session: DB,
) -> dict[str, object]:
    """Record detected three-way sync conflicts for web review (manager or paired service)."""
    context = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(
        session, context, permission=Permission.PROJECT_EDIT, mutation=True
    )
    if gis_access.scoped(session, context):
        raise HTTPException(403, "Full project access required.")
    binding = session.execute(
        _SELECT_ONE, {"org": organization_id, "proj": project_id, "id": binding_id}
    ).mappings().first()
    if binding is None:
        raise HTTPException(404, "Binding not found.")
    created: list[dict[str, object]] = []
    actor_id = context.actor.actor_id if auth.actor.actor_type == "human" else None
    for item in body.items:
        row = session.execute(
            _INSERT_CONFLICT,
            {
                "id": uuid4(),
                "org": organization_id,
                "proj": project_id,
                "bid": binding_id,
                "lid": binding["layer_id"],
                "fid": item.feature_id,
                "kind": item.conflict_kind,
                "field_name": item.field_name,
                "baseline": json.dumps(item.baseline_value),
                "web": json.dumps(item.web_value),
                "qgis": json.dumps(item.qgis_value),
                "actor": actor_id,
            },
        ).mappings().one()
        created.append(_public_conflict(row))
    append_project_audit(
        session,
        context,
        audit_id=uuid4(),
        action="integration.binding.conflicts_recorded",
        resource_type="connector_binding",
        resource_id=binding_id,
        metadata={"count": len(created)},
        created_at=datetime.now(UTC),
    )
    return {"items": created}


@router.post(BASE + "/{binding_id}/conflicts/{conflict_id}/resolve")
def resolve_binding_conflict(
    organization_id: UUID,
    project_id: UUID,
    binding_id: UUID,
    conflict_id: UUID,
    body: ResolveConflictRequest,
    auth: WriteAuth,
    session: DB,
) -> dict[str, object]:
    """Resolve an open sync conflict by choosing Web, QGIS, merged value, or dismiss."""
    context = _scope(session, auth, organization_id, project_id, mutation=True)
    existing = session.execute(
        _GET_CONFLICT,
        {"org": organization_id, "proj": project_id, "bid": binding_id, "cid": conflict_id},
    ).mappings().first()
    if existing is None:
        raise HTTPException(404, "Conflict not found.")
    if str(existing["status"]) != "open":
        raise HTTPException(409, "Conflict is already resolved.")

    status_map = {
        "keep_web": "resolved_web",
        "keep_qgis": "resolved_qgis",
        "merge": "resolved_merged",
        "dismiss": "dismissed",
    }
    if body.decision == "keep_web":
        chosen_value = existing["web_value"]
    elif body.decision == "keep_qgis":
        chosen_value = existing["qgis_value"]
    elif body.decision == "merge":
        if body.merged_value is None:
            raise HTTPException(422, "merged_value is required when decision is 'merge'.")
        chosen_value = body.merged_value
    else:
        chosen_value = None

    resolved = session.execute(
        _RESOLVE_CONFLICT,
        {
            "org": organization_id,
            "proj": project_id,
            "bid": binding_id,
            "cid": conflict_id,
            "status": status_map[body.decision],
            "resolved_value": json.dumps(chosen_value),
            "note": body.note,
            "actor": context.actor.actor_id,
        },
    ).mappings().first()
    if resolved is None:
        raise HTTPException(409, "Conflict could not be updated.")
    append_project_audit(
        session,
        context,
        audit_id=uuid4(),
        action="integration.binding.conflict_resolved",
        resource_type="connector_binding",
        resource_id=binding_id,
        metadata={
            "conflict_id": str(conflict_id),
            "feature_id": str(existing["feature_id"]),
            "decision": body.decision,
        },
        created_at=datetime.now(UTC),
    )
    return _public_conflict(resolved)
