"""R027 project-scoped QGIS binding drafts.

This is a real persistent metadata registry, but NOT an active connector.
No endpoint grants sync rights, accepts secrets, writes features, launches
agents, or claims automatic/bi-directional delivery.
"""

from __future__ import annotations

import json
import re
from datetime import UTC, datetime
from typing import Annotated, Literal
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel, ConfigDict, Field, StringConstraints, field_validator
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence import gis_access
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.domains.authorization.permissions import Permission
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
    # Label-only, never a file path/URL/token. Real agent pairing happens
    # through a different, security-reviewed protocol.
    external_dataset_ref: Annotated[
        str, StringConstraints(strip_whitespace=True, min_length=1, max_length=128,
                               pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
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
_FIELDS_SQL = text("""
    SELECT field_id FROM spatial.source_fields
    WHERE organization_id=:org AND project_id=:proj AND source_id=:sid
""")
_VIEW_COLS = """
    id, layer_id, adapter_key, external_dataset_ref, direction,
    source_crs_code, target_crs_code, external_primary_key,
    allowed_field_ids, base_content_revision, base_schema_revision,
    state, revision, created_at, updated_at
"""
_SELECT = text(f"""
    SELECT {_VIEW_COLS} FROM integration_hub.connector_bindings
    WHERE organization_id=:org AND project_id=:proj
    ORDER BY created_at DESC, id DESC LIMIT 200
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

def _scope(session: Session, auth: AuthenticatedRequest, org: UUID, proj: UUID):
    if auth.actor.actor_type != "human":
        raise HTTPException(status_code=403, detail="Project manager session required.")
    context = context_for(auth, organization_id=org, project_id=proj)
    prepare_project_scope(session, context, permission=Permission.PROJECT_MANAGE)
    if gis_access.scoped(session, context):
        raise HTTPException(status_code=403, detail="Full project access required.")
    return context


def _public(row: object) -> dict[str, object]:
    out = dict(row)  # Only explicitly selected safe columns; no credentials.
    out["id"] = str(out["id"])
    out["layer_id"] = str(out["layer_id"])
    if isinstance(out.get("allowed_field_ids"), str):
        out["allowed_field_ids"] = json.loads(out["allowed_field_ids"])
    out["writes_allowed"] = False
    out["deletes_allowed"] = False
    out["connected"] = False
    out["reason"] = "DRAFT_ONLY_AGENT_NOT_PAIRED"
    return out


@router.get(BASE)
def list_bindings(
    organization_id: UUID, project_id: UUID, auth: ReadAuth, session: DB
) -> dict[str, object]:
    _scope(session, auth, organization_id, project_id)
    rows = session.execute(_SELECT, {"org": organization_id, "proj": project_id}).mappings()
    return {"items": [_public(row) for row in rows]}


@router.post(BASE, status_code=201)
def create_binding(
    organization_id: UUID, project_id: UUID, body: CreateBinding,
    auth: WriteAuth, session: DB,
) -> dict[str, object]:
    context = _scope(session, auth, organization_id, project_id)
    args = {"org": organization_id, "proj": project_id, "lid": body.layer_id}
    layer = session.execute(_LAYER_SQL, args).mappings().first()
    if not layer:
        raise HTTPException(status_code=404, detail="Layer not found.")
    if str(layer["validation_state"]) != "ready" or str(layer["geometry_family"]) not in (
        "Point", "LineString", "Polygon"
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
        **args, "id": binding_id, "adapter": "qgis",
        "dataset": body.external_dataset_ref, "direction": body.direction,
        "source_crs": body.source_crs_code, "target_crs": body.target_crs_code,
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
        session, context, audit_id=uuid4(),
        action="integration.binding.draft_created", resource_type="connector_binding",
        resource_id=binding_id,
        metadata={"adapter": "qgis", "layer_id": str(body.layer_id), "writes_allowed": False},
        created_at=datetime.now(UTC),
    )
    return _public(result)


@router.post(BASE + "/{binding_id}/lifecycle")
def change_binding_lifecycle(
    organization_id: UUID, project_id: UUID, binding_id: UUID,
    body: ChangeLifecycle, auth: WriteAuth, session: DB,
) -> dict[str, object]:
    context = _scope(session, auth, organization_id, project_id)
    result = session.execute(
        _STATE, {
            "org": organization_id, "proj": project_id, "id": binding_id,
            "expected": body.expected_revision,
            "state": "paused" if body.action == "pause" else "revoked",
        },
    ).mappings().first()
    if result is None:
        raise HTTPException(
            409, "Binding unavailable or revision changed; reload binding state."
        )
    append_project_audit(
        session, context, audit_id=uuid4(),
        action=f"integration.binding.{body.action}",
        resource_type="connector_binding", resource_id=binding_id,
        metadata={"state": str(result["state"]), "revision": int(result["revision"])},
        created_at=datetime.now(UTC),
    )
    return _public(result)
