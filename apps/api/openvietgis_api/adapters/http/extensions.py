"""R014 Generic Internal Extension Framework HTTP adapter."""

from __future__ import annotations

from typing import Annotated, Any
from uuid import UUID

from fastapi import APIRouter, Depends, Query, Response
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.official_modules import (
    discover_official_module_runtimes,
    official_manifest,
)
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.extensions_repository import SqlExtensionsRepository
from openvietgis_api.domains.extensions.domain import (
    ExtensionDataRecord,
    ExtensionManifestRecord,
    ProjectActivationRecord,
    matches_official_manifest,
)
from openvietgis_api.domains.platform_errors import PlatformConflictError, PlatformNotFoundError

router = APIRouter(tags=["extensions"])
_ORG = "/organizations/{organization_id}"
_PROJ = f"{_ORG}/projects/{{project_id}}"


def _manifest_wire(m: ExtensionManifestRecord) -> dict[str, Any]:
    return {
        "id": str(m.id),
        "organization_id": str(m.organization_id),
        "extension_key": m.extension_key,
        "version": m.version,
        "api_compatibility": m.api_compatibility,
        "display_name": m.display_name,
        "description": m.description,
        "permissions": m.permissions,
        "commands": m.commands,
        "capabilities": m.capabilities,
        "entity_schemas": m.entity_schemas,
        "ui_contributions": m.ui_contributions,
        "enabled": m.enabled,
        "created_at": m.created_at.isoformat(),
    }


def _validate_official_runtime(manifest: ExtensionManifestRecord) -> None:
    shipped = official_manifest(manifest.extension_key)
    if shipped is not None and not matches_official_manifest(_manifest_wire(manifest), shipped):
        raise PlatformConflictError(
            "stored official manifest differs from shipped runtime",
            code="OFFICIAL_MANIFEST_MISMATCH",
        )


def _act_wire(a: ProjectActivationRecord) -> dict[str, Any]:
    return {
        "id": str(a.id),
        "manifest_id": str(a.manifest_id),
        "extension_key": a.extension_key,
        "status": a.status,
        "revision": a.revision,
        "activated_at": a.activated_at.isoformat(),
        "disabled_at": a.disabled_at.isoformat() if a.disabled_at else None,
        "updated_at": a.updated_at.isoformat(),
    }


def _rec_wire(r: ExtensionDataRecord) -> dict[str, Any]:
    return {
        "id": str(r.id),
        "extension_key": r.extension_key,
        "entity_name": r.entity_name,
        "payload": r.payload,
        "linked_layer_id": str(r.linked_layer_id) if r.linked_layer_id else None,
        "linked_feature_id": r.linked_feature_id,
        "created_at": r.created_at.isoformat(),
        "updated_at": r.updated_at.isoformat(),
        "manifest_id": str(r.manifest_id),
        "revision": r.revision,
        "operation_id": str(r.operation_id) if r.operation_id else None,
        "link_context": r.link_context,
    }


class RegisterManifestRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    extension_key: str
    version: str
    api_compatibility: str = "1.x"
    display_name: str
    description: str = ""
    permissions: list[str] = Field(default_factory=list)
    commands: list[dict[str, Any]] = Field(default_factory=list)
    capabilities: list[str] = Field(default_factory=list)
    entity_schemas: dict[str, dict[str, Any]]
    ui_contributions: list[dict[str, Any]] = Field(default_factory=list)


class ActivateExtensionRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    manifest_id: UUID


class CreateExtensionRecordRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    entity_name: str
    payload: dict[str, Any]
    linked_layer_id: UUID | None = None
    linked_feature_id: str | None = None
    expected_link_context: dict[str, Any] | None = None
    operation_id: UUID | None = None


class UpdateExtensionRecordRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: int = Field(ge=1, le=2147483646, strict=True)
    expected_manifest_id: UUID | None = None
    payload: dict[str, Any]
    expected_link_context: dict[str, Any] | None = None
    operation_id: UUID | None = None


class TransitionExtensionRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_manifest_id: UUID
    target_manifest_id: UUID


class PortableImportRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_manifest_id: UUID
    bundle: dict[str, Any]
    sha256: str = Field(pattern=r"^[0-9a-f]{64}$")


@router.post(f"{_ORG}/extensions/manifests", status_code=201)
def register_extension_manifest(
    organization_id: UUID,
    body: RegisterManifestRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    if official_manifest(body.extension_key) is not None:
        raise PlatformConflictError(
            "official runtime keys must use the official registration endpoint",
            code="OFFICIAL_MANIFEST_REQUIRED",
        )
    ctx = context_for(auth, organization_id=organization_id)
    rec = SqlExtensionsRepository(session).register_manifest(
        context=ctx,
        extension_key=body.extension_key,
        version=body.version,
        api_compatibility=body.api_compatibility,
        display_name=body.display_name,
        description=body.description,
        permissions=body.permissions,
        commands=body.commands,
        capabilities=body.capabilities,
        entity_schemas=body.entity_schemas,
        ui_contributions=body.ui_contributions,
    )
    return _manifest_wire(rec)


@router.get(f"{_ORG}/extensions/official")
def list_official_extension_catalog(
    organization_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id)
    SqlExtensionsRepository(session).list_manifests(context=ctx)
    items = []
    for runtime in discover_official_module_runtimes():
        manifest = runtime.manifest
        items.append(
            {
                "extension_key": runtime.extension_key,
                "version": manifest.get("version"),
                "display_name": manifest.get("display_name"),
                "description": manifest.get("description", ""),
                "capabilities": list(manifest.get("capabilities", [])),
                "ui_contributions": list(manifest.get("ui_contributions", [])),
            }
        )
    return {"items": items}


@router.post(f"{_ORG}/extensions/official/{{extension_key}}/register", status_code=201)
def register_official_extension_manifest(
    organization_id: UUID,
    extension_key: str,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    manifest = official_manifest(extension_key)
    if manifest is None:
        raise PlatformNotFoundError("official extension not found")
    ctx = context_for(auth, organization_id=organization_id)
    repo = SqlExtensionsRepository(session)
    for item in repo.list_manifests(context=ctx):
        if item.extension_key == extension_key and item.version == str(manifest["version"]):
            if not matches_official_manifest(_manifest_wire(item), manifest):
                raise PlatformConflictError(
                    "stored official manifest differs from shipped runtime",
                    code="OFFICIAL_MANIFEST_MISMATCH",
                )
            return _manifest_wire(item)
    rec = repo.register_manifest(
        context=ctx,
        extension_key=str(manifest["extension_key"]),
        version=str(manifest["version"]),
        api_compatibility=str(manifest.get("api_compatibility", "1.x")),
        display_name=str(manifest["display_name"]),
        description=str(manifest.get("description", "")),
        permissions=list(manifest.get("permissions", [])),
        commands=list(manifest.get("commands", [])),
        capabilities=list(manifest.get("capabilities", [])),
        entity_schemas=dict(manifest["entity_schemas"]),
        ui_contributions=list(manifest.get("ui_contributions", [])),
    )
    return _manifest_wire(rec)


@router.get(f"{_ORG}/extensions/manifests")
def list_extension_manifests(
    organization_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id)
    items = SqlExtensionsRepository(session).list_manifests(context=ctx)
    return {"items": [_manifest_wire(m) for m in items]}


@router.post(f"{_PROJ}/extensions/activations", status_code=201)
def activate_project_extension(
    organization_id: UUID,
    project_id: UUID,
    body: ActivateExtensionRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    return _act_wire(
        SqlExtensionsRepository(session).activate_for_project(
            context=ctx, manifest_id=body.manifest_id, validate_runtime=_validate_official_runtime
        )
    )


@router.post(f"{_PROJ}/extensions/{{extension_key}}/disable")
def disable_project_extension(
    organization_id: UUID,
    project_id: UUID,
    extension_key: str,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    return _act_wire(
        SqlExtensionsRepository(session).disable_for_project(
            context=ctx, extension_key=extension_key
        )
    )


@router.post(f"{_PROJ}/extensions/activations/{{activation_id}}/deactivate")
def deactivate_project_extension(
    organization_id: UUID,
    project_id: UUID,
    activation_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    return _act_wire(
        SqlExtensionsRepository(session).deactivate_for_project(
            context=ctx, activation_id=activation_id
        )
    )


@router.get(f"{_PROJ}/extensions/activations")
def list_project_activations(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    repo = SqlExtensionsRepository(session)
    items = repo.list_activations(context=ctx)
    if not items:
        return {"items": []}
    shipped = {r.extension_key: r.manifest for r in discover_official_module_runtimes()}
    manifests = {m.id: _manifest_wire(m) for m in repo.list_manifests(context=ctx)}
    result = []
    for a in items:
        wire = _act_wire(a)
        registered = manifests.get(a.manifest_id, {})
        wire["manifest_version"] = registered.get("version")
        if a.extension_key in shipped:
            wire["runtime_compatible"] = matches_official_manifest(
                registered, shipped[a.extension_key]
            )
        result.append(wire)
    return {"items": result}


@router.post(f"{_PROJ}/extensions/{{extension_key}}/records", status_code=201)
def create_extension_record(
    organization_id: UUID,
    project_id: UUID,
    extension_key: str,
    body: CreateExtensionRecordRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    return _rec_wire(
        SqlExtensionsRepository(session).create_record(
            context=ctx,
            extension_key=extension_key,
            entity_name=body.entity_name,
            payload=body.payload,
            linked_layer_id=body.linked_layer_id,
            linked_feature_id=body.linked_feature_id,
            expected_link_context=body.expected_link_context,
            operation_id=body.operation_id,
        )
    )


@router.get(f"{_PROJ}/extensions/{{extension_key}}/records")
def list_extension_records(
    organization_id: UUID,
    project_id: UUID,
    extension_key: str,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    entity_name: Annotated[str | None, Query()] = None,
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    items = SqlExtensionsRepository(session).list_records(
        context=ctx, extension_key=extension_key, entity_name=entity_name
    )
    return {"items": [_rec_wire(r) for r in items]}


@router.post(f"{_PROJ}/extensions/activations/{{activation_id}}/transition")
def transition_project_extension(
    organization_id: UUID,
    project_id: UUID,
    activation_id: UUID,
    body: TransitionExtensionRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    return _act_wire(
        SqlExtensionsRepository(session).transition_version(
            context=ctx,
            activation_id=activation_id,
            expected_manifest_id=body.expected_manifest_id,
            target_manifest_id=body.target_manifest_id,
            validate_runtime=_validate_official_runtime,
        )
    )


@router.get(f"{_PROJ}/extensions/activations/{{activation_id}}/history")
def list_extension_activation_history(
    organization_id: UUID,
    project_id: UUID,
    activation_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    return {
        "items": SqlExtensionsRepository(session).activation_history(
            context=ctx,
            activation_id=activation_id,
        )
    }


@router.put(f"{_PROJ}/extensions/{{extension_key}}/records/{{record_id}}")
def update_extension_record(
    organization_id: UUID,
    project_id: UUID,
    extension_key: str,
    record_id: UUID,
    body: UpdateExtensionRecordRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    return _rec_wire(
        SqlExtensionsRepository(session).update_record(
            context=ctx,
            extension_key=extension_key,
            record_id=record_id,
            expected_revision=body.expected_revision,
            expected_manifest_id=body.expected_manifest_id,
            payload=body.payload,
            expected_link_context=body.expected_link_context,
            operation_id=body.operation_id,
        )
    )


@router.get(f"{_PROJ}/extensions/{{extension_key}}/records/{{record_id}}/history")
def list_extension_record_history(
    organization_id: UUID,
    project_id: UUID,
    extension_key: str,
    record_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    return {
        "items": SqlExtensionsRepository(session).record_history(
            context=ctx,
            extension_key=extension_key,
            record_id=record_id,
        )
    }


@router.get(f"{_PROJ}/extensions/{{extension_key}}/portable-export")
def export_extension_bundle(
    organization_id: UUID,
    project_id: UUID,
    extension_key: str,
    response: Response,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    result = SqlExtensionsRepository(session).export_bundle(
        context=ctx, extension_key=extension_key
    )
    response.headers["Cache-Control"] = "no-store"
    return result


@router.post(f"{_PROJ}/extensions/{{extension_key}}/portable-import")
def import_extension_bundle(
    organization_id: UUID,
    project_id: UUID,
    extension_key: str,
    body: PortableImportRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    return SqlExtensionsRepository(session).import_bundle(
        context=ctx,
        extension_key=extension_key,
        expected_manifest_id=body.expected_manifest_id,
        bundle=body.bundle,
        sha256=body.sha256,
    )
