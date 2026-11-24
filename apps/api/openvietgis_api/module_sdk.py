"""Stable internal SDK surface for trusted official modules.

Vertical modules depend on this file rather than persistence/http internals. Third-party
code is not auto-loaded by the official-module runtime.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from hashlib import sha256
from pathlib import Path
from typing import Literal
from uuid import UUID, uuid4

from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.config import get_settings
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.extensions.domain import matches_official_manifest
from openvietgis_api.domains.platform_errors import PlatformNotFoundError

__all__ = [
    "AuthenticatedRequest",
    "AuthorizationContext",
    "append_project_audit",
    "authenticate_request",
    "context_for",
    "get_db_session",
    "ModuleArtifact",
    "put_module_artifact",
    "read_module_artifact",
    "prepare_module_scope",
    "require_official_activation",
    "require_csrf",
    "set_extension_private_access",
]


def prepare_module_scope(
    session: Session,
    context: AuthorizationContext,
    *,
    access: Literal["read", "edit", "manage"],
) -> None:
    permission = {
        "read": Permission.PROJECT_READ,
        "edit": Permission.PROJECT_EDIT,
        "manage": Permission.PROJECT_MANAGE,
    }[access]
    prepare_project_scope(session, context, permission=permission)


def require_official_activation(
    session: Session,
    context: AuthorizationContext,
    manifest: dict,
) -> None:
    """Called after prepare_module_scope; never treat metadata as an executable grant."""
    row = (
        session.execute(
            text("""
            SELECT m.* FROM extensions.project_activations a
            JOIN extensions.manifests m ON m.organization_id=a.organization_id
                AND m.id=a.manifest_id AND m.extension_key=a.extension_key
            WHERE a.organization_id=:org AND a.project_id=:project
                AND a.extension_key=:key AND a.status='active'
        """),
            {
                "org": context.organization_id,
                "project": context.project_id,
                "key": manifest["extension_key"],
            },
        )
        .mappings()
        .first()
    )
    if row is None or not matches_official_manifest(dict(row), manifest):
        raise PlatformNotFoundError("official module activation does not match shipped runtime")


def set_extension_private_access(session: Session, *, extension_key: str, enabled: bool) -> None:
    """Bind a transaction-local private-data namespace for one trusted extension."""
    allowed = "abcdefghijklmnopqrstuvwxyz0123456789_-"
    if not extension_key or any(ch not in allowed for ch in extension_key):
        raise ValueError("invalid extension key")
    session.execute(
        text("SELECT set_config('openvietgis.extension_private_access', :value, true)"),
        {"value": extension_key if enabled else ""},
    )


_ARTIFACT_NAME = re.compile(r"[^A-Za-z0-9._-]+")


@dataclass(frozen=True, slots=True)
class ModuleArtifact:
    id: UUID
    filename: str
    content_type: str
    byte_size: int
    sha256: str
    storage_key: str


def put_module_artifact(
    context: AuthorizationContext,
    *,
    namespace: str,
    filename: str,
    content_type: str,
    data: bytes,
) -> ModuleArtifact:
    if context.organization_id is None or context.project_id is None:
        raise ValueError("project context required for module artifact")
    safe_namespace = _ARTIFACT_NAME.sub("-", namespace.strip()).strip("-._")
    safe_filename = _ARTIFACT_NAME.sub("-", Path(filename).name).strip("-._") or "artifact.bin"
    if not safe_namespace:
        raise ValueError("artifact namespace required")
    artifact_id = uuid4()
    storage_key = (
        f"extensions/{context.organization_id}/{context.project_id}/"
        f"{safe_namespace}/{artifact_id}/{safe_filename}"
    )
    root = Path(get_settings().artifact_store_dir).resolve()
    target = (root / storage_key).resolve()
    if root not in target.parents:
        raise ValueError("invalid artifact path")
    target.parent.mkdir(parents=True, exist_ok=True)
    digest = sha256(data).hexdigest()
    target.write_bytes(data)
    meta = {
        "id": str(artifact_id),
        "filename": safe_filename,
        "content_type": content_type,
        "byte_size": len(data),
        "sha256": digest,
        "storage_key": storage_key,
    }
    target.with_suffix(target.suffix + ".json").write_text(
        json.dumps(meta, ensure_ascii=False, sort_keys=True), encoding="utf-8"
    )
    return ModuleArtifact(
        id=artifact_id,
        filename=safe_filename,
        content_type=content_type,
        byte_size=len(data),
        sha256=digest,
        storage_key=storage_key,
    )


def read_module_artifact(
    context: AuthorizationContext, *, namespace: str, artifact_id: UUID
) -> tuple[ModuleArtifact, bytes]:
    if context.organization_id is None or context.project_id is None:
        raise ValueError("project context required for module artifact")
    safe_namespace = _ARTIFACT_NAME.sub("-", namespace.strip()).strip("-._")
    root = Path(get_settings().artifact_store_dir).resolve()
    parent = (
        root
        / "extensions"
        / str(context.organization_id)
        / str(context.project_id)
        / safe_namespace
        / str(artifact_id)
    ).resolve()
    if root not in parent.parents or not parent.is_dir():
        raise FileNotFoundError("module artifact not found")
    metas = list(parent.glob("*.json"))
    if len(metas) != 1:
        raise FileNotFoundError("module artifact metadata not found")
    meta = json.loads(metas[0].read_text(encoding="utf-8"))
    file_path = parent / str(meta["filename"])
    data = file_path.read_bytes()
    digest = sha256(data).hexdigest()
    if digest != meta["sha256"] or len(data) != int(meta["byte_size"]):
        raise RuntimeError("module artifact integrity mismatch")
    return (
        ModuleArtifact(
            id=artifact_id,
            filename=str(meta["filename"]),
            content_type=str(meta["content_type"]),
            byte_size=int(meta["byte_size"]),
            sha256=str(meta["sha256"]),
            storage_key=str(meta["storage_key"]),
        ),
        data,
    )
