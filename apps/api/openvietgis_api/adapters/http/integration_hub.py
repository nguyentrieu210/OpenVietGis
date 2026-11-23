"""Authenticated project-scoped interoperability catalog and read-only adapter profiles.

No connection secrets, remote requests, file uploads, or writes are exposed here.
"""

from __future__ import annotations

import hashlib
import json
import math
import re
from collections.abc import Mapping
from datetime import UTC, datetime
from typing import Annotated, Any, Final, Literal
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, HTTPException, Response
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.formats.geojson import write_geojson
from openvietgis_api.adapters.formats.geopackage import write_geopackage
from openvietgis_api.adapters.formats.postgis_sql import write_postgis_sql
from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
)
from openvietgis_api.adapters.persistence import gis_access
from openvietgis_api.adapters.persistence.authorization_repository import (
    SqlEffectivePermissionProvider,
)
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.service import AuthorizationService
from openvietgis_api.domains.integration_hub.catalog import get_adapter, list_adapters
from openvietgis_api.domains.integration_hub.profiles import (
    PROFILE_ENABLED_ADAPTER_KEYS,
    LayerAccessSnapshot,
    build_adapter_profile,
    is_explicit_readable_crs,
)
from openvietgis_api.domains.spatial.domain import (
    FieldType,
    NativeGeometry,
    SpatialValidationError,
)
from openvietgis_api.domains.transfers.domain import ExportFieldMapping

router = APIRouter(tags=["integration-hub"])

EXPORT_ENABLED_ADAPTER_KEYS: Final[frozenset[str]] = frozenset({"qgis", "postgis"})
MAX_HUB_EXPORT_FEATURES: Final[int] = 25_000
_SUPPORTED_EXPORT_GEOMETRIES: Final[frozenset[str]] = frozenset(
    {"Point", "LineString", "Polygon"}
)


def _set_no_store_headers(response: Response) -> None:
    response.headers["Cache-Control"] = "no-store"


_ACTIVE_LAYERS_SQL = text(
    """
    SELECT
        l.id AS layer_id,
        l.source_id AS source_id,
        l.name AS name,
        s.geometry_family AS geometry_family,
        s.dimensions AS dimensions,
        s.validation_state AS validation_state,
        s.source_crs AS source_crs_code,
        s.stored_crs AS stored_crs_code,
        l.layer_revision AS layer_revision,
        s.content_revision AS content_revision,
        s.schema_revision AS schema_revision
    FROM spatial.layers l
    JOIN spatial.data_sources s
      ON s.organization_id = l.organization_id
     AND s.project_id = l.project_id
     AND s.id = l.source_id
    WHERE l.organization_id = :org
      AND l.project_id = :proj
      AND l.lifecycle = 'active'
      AND s.lifecycle = 'active'
    ORDER BY l.name ASC, l.id ASC
    LIMIT 1000
    """
)

_ACTIVE_LAYER_BY_ID_SQL = text(
    """
    SELECT
        l.id AS layer_id,
        l.source_id AS source_id,
        l.name AS name,
        s.geometry_family AS geometry_family,
        s.dimensions AS dimensions,
        s.validation_state AS validation_state,
        s.source_crs AS source_crs_code,
        s.stored_crs AS stored_crs_code,
        l.layer_revision AS layer_revision,
        s.content_revision AS content_revision,
        s.schema_revision AS schema_revision
    FROM spatial.layers l
    JOIN spatial.data_sources s
      ON s.organization_id = l.organization_id
     AND s.project_id = l.project_id
     AND s.id = l.source_id
    WHERE l.organization_id = :org
      AND l.project_id = :proj
      AND l.id = :lid
      AND l.lifecycle = 'active'
      AND s.lifecycle = 'active'
    LIMIT 1
    """
)

_SOURCE_FIELDS_SQL = text(
    """
    SELECT field_id, name, type
    FROM spatial.source_fields
    WHERE organization_id = :org
      AND project_id = :proj
      AND source_id = :sid
    ORDER BY ordinal ASC
    """
)

_EXPORT_FEATURES_SQL = text(
    """
    SELECT feature_id, attributes, ST_AsGeoJSON(geometry) AS geom_json
    FROM spatial.reference_features
    WHERE organization_id = :org
      AND project_id = :proj
      AND source_id = :sid
      AND geometry_state = 'present'
      AND geometry IS NOT NULL
    ORDER BY feature_id ASC
    LIMIT :limit
    """
)

_EXPORT_FEATURES_SCOPED_SQL = text(
    """
    SELECT feature_id, attributes, ST_AsGeoJSON(geometry) AS geom_json
    FROM spatial.reference_features
    WHERE organization_id = :org
      AND project_id = :proj
      AND source_id = :sid
      AND geometry_state = 'present'
      AND geometry IS NOT NULL
      AND gis_access.feature_allowed(
          :org, :proj, :sid, feature_id, attributes, geometry, 'export'
      )
    ORDER BY feature_id ASC
    LIMIT :limit
    """
)


def _is_scoped(session: Session, context: AuthorizationContext) -> bool:
    return bool(gis_access.scoped(session, context))


def _source_allowed(
    session: Session,
    context: AuthorizationContext,
    source_id: object,
    action: str,
) -> bool:
    return bool(gis_access.source_allowed(session, context, source_id, action))


def _safe_filename_slug(name: str) -> str:
    slug = re.sub(r"[^a-z0-9_-]+", "-", name.strip().lower()).strip("-_")
    if not slug:
        return "layer"
    return slug[:64]


def _to_tuple_coords(coords: Any) -> Any:
    if isinstance(coords, list):
        return tuple(_to_tuple_coords(item) for item in coords)
    if isinstance(coords, tuple):
        return tuple(_to_tuple_coords(item) for item in coords)
    if isinstance(coords, bool) or not isinstance(coords, int | float):
        raise ValueError("Coordinate values must be numeric.")
    val = float(coords)
    if not math.isfinite(val):
        raise ValueError("Coordinate values must be finite.")
    return val


def _load_accessible_layers(
    session: Session,
    context: AuthorizationContext,
    *,
    organization_id: UUID,
    project_id: UUID,
    is_scoped: bool,
    permissions: frozenset[Permission],
) -> list[LayerAccessSnapshot]:
    rows = (
        session.execute(
            _ACTIVE_LAYERS_SQL,
            {"org": organization_id, "proj": project_id},
        )
        .mappings()
        .all()
    )
    snapshots: list[LayerAccessSnapshot] = []
    for row in rows:
        source_id = row["source_id"]
        validation_state = str(row["validation_state"])
        source_crs_raw = row["source_crs_code"]
        stored_crs_raw = row["stored_crs_code"]
        stored_crs_code = int(stored_crs_raw) if stored_crs_raw is not None else None
        crs_explicit = is_explicit_readable_crs(stored_crs_code, validation_state)
        if is_scoped:
            if not _source_allowed(session, context, source_id, "read"):
                continue
            export_permitted = (
                _source_allowed(session, context, source_id, "export") and crs_explicit
            )
        else:
            export_permitted = Permission.PROJECT_READ in permissions and crs_explicit
        snapshots.append(
            LayerAccessSnapshot(
                layer_id=str(row["layer_id"]),
                source_id=str(source_id),
                name=str(row["name"]),
                geometry_family=str(row["geometry_family"]),
                dimensions=str(row["dimensions"]),
                validation_state=validation_state,
                source_crs_code=int(source_crs_raw) if source_crs_raw is not None else None,
                stored_crs_code=stored_crs_code,
                layer_revision=int(row["layer_revision"]),
                content_revision=str(row["content_revision"]),
                schema_revision=str(row["schema_revision"]),
                export_permitted=export_permitted,
            )
        )
    return snapshots


def _load_export_features(
    session: Session,
    context: AuthorizationContext,
    *,
    organization_id: UUID,
    project_id: UUID,
    source_id: object,
    is_scoped: bool,
    stored_crs_code: int,
    geometry_family: str | None = None,
) -> list[tuple[str, dict[str, Any], NativeGeometry]]:
    del context  # Session RLS actor context is already bound by prepare_project_scope.
    rows = (
        session.execute(
            _EXPORT_FEATURES_SCOPED_SQL if is_scoped else _EXPORT_FEATURES_SQL,
            {
                "org": organization_id,
                "proj": project_id,
                "sid": source_id,
                "limit": MAX_HUB_EXPORT_FEATURES + 1,
            },
        )
        .mappings()
        .all()
    )
    if len(rows) > MAX_HUB_EXPORT_FEATURES:
        raise HTTPException(
            status_code=422,
            detail="Export limit of 25,000 features exceeded.",
        )

    features: list[tuple[str, dict[str, Any], NativeGeometry]] = []
    for row in rows:
        raw_geom = row["geom_json"]
        if raw_geom is None:
            continue
        try:
            geom_obj = json.loads(raw_geom) if isinstance(raw_geom, str) else raw_geom
            if not isinstance(geom_obj, dict):
                raise ValueError("Feature geometry must be a GeoJSON object.")
            geom_type = str(geom_obj.get("type", ""))
            if geom_type not in _SUPPORTED_EXPORT_GEOMETRIES:
                raise ValueError(f"Unsupported feature geometry type: {geom_type}")
            if geometry_family is not None and geom_type != geometry_family:
                raise ValueError("Feature geometry type does not match layer geometry family.")
            tuple_coords = _to_tuple_coords(geom_obj.get("coordinates"))
            native_geom = NativeGeometry(geom_type, tuple_coords, stored_crs_code)
        except (ValueError, TypeError, SpatialValidationError) as exc:
            raise HTTPException(
                status_code=422,
                detail="Layer contains invalid or unsupported feature geometry.",
            ) from exc
        raw_attrs = row["attributes"]
        attrs = dict(raw_attrs) if isinstance(raw_attrs, Mapping) else {}
        features.append((str(row["feature_id"]), attrs, native_geom))
    return features


def _record_export_audit(
    session: Session,
    context: AuthorizationContext,
    *,
    layer_id: UUID,
    adapter_key: str,
    export_format: str,
    stored_crs_code: int,
    feature_count: int,
    content_revision: str,
    schema_revision: str,
    sha256: str,
    scoped_access: bool,
) -> None:
    append_project_audit(
        session,
        context,
        audit_id=uuid4(),
        action="integration_hub.layer_export",
        resource_type="layer",
        resource_id=layer_id,
        metadata={
            "project_id": str(context.project_id),
            "adapter_key": adapter_key,
            "format": export_format,
            "crs": f"EPSG:{stored_crs_code}",
            "feature_count": feature_count,
            "content_revision": content_revision,
            "schema_revision": schema_revision,
            "sha256": sha256,
            "scoped_access": scoped_access,
        },
        created_at=datetime.now(UTC),
    )


@router.get(
    "/organizations/{organization_id}/projects/{project_id}/integration-hub/catalog",
    dependencies=[Depends(_set_no_store_headers)],
)
def read_integration_catalog(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    context = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(session, context, permission=Permission.PROJECT_READ)
    permissions = AuthorizationService(
        SqlEffectivePermissionProvider(session)
    ).effective_permissions(context)
    is_scoped = _is_scoped(session, context)
    return {
        "schema_version": 1,
        "organization_id": str(organization_id),
        "project_id": str(project_id),
        "mode": "catalog_read_only",
        "scoped_access": is_scoped,
        "default_write_policy": "deny",
        "adapters": list_adapters(permissions),
    }


@router.get(
    "/organizations/{organization_id}/projects/{project_id}/integration-hub/adapters/{adapter_key}",
    dependencies=[Depends(_set_no_store_headers)],
)
def read_integration_adapter_profile(
    organization_id: UUID,
    project_id: UUID,
    adapter_key: str,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    context = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(session, context, permission=Permission.PROJECT_READ)
    adapter = get_adapter(adapter_key)
    if adapter is None:
        raise HTTPException(status_code=404, detail="Integration adapter not found.")
    permissions = AuthorizationService(
        SqlEffectivePermissionProvider(session)
    ).effective_permissions(context)
    is_scoped = _is_scoped(session, context)
    layers = (
        _load_accessible_layers(
            session,
            context,
            organization_id=organization_id,
            project_id=project_id,
            is_scoped=is_scoped,
            permissions=permissions,
        )
        if adapter.key in PROFILE_ENABLED_ADAPTER_KEYS
        else []
    )
    return build_adapter_profile(
        adapter,
        permissions,
        organization_id=str(organization_id),
        project_id=str(project_id),
        scoped_access=is_scoped,
        layers=layers,
    )


@router.get(
    "/organizations/{organization_id}/projects/{project_id}"
    "/integration-hub/adapters/{adapter_key}/layers/{layer_id}/export",
    dependencies=[Depends(_set_no_store_headers)],
)
def export_integration_layer(
    organization_id: UUID,
    project_id: UUID,
    adapter_key: str,
    layer_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    format: Literal["geopackage", "geojson", "postgis_sql"] = "geopackage",
) -> Response:
    context = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(session, context, permission=Permission.PROJECT_READ)

    adapter = get_adapter(adapter_key)
    if adapter is None:
        raise HTTPException(status_code=404, detail="Integration adapter not found.")
    if adapter.key not in EXPORT_ENABLED_ADAPTER_KEYS:
        raise HTTPException(
            status_code=409,
            detail="Read-only hub export is not enabled for this adapter.",
        )

    layer_row = (
        session.execute(
            _ACTIVE_LAYER_BY_ID_SQL,
            {"org": organization_id, "proj": project_id, "lid": layer_id},
        )
        .mappings()
        .first()
    )
    if layer_row is None:
        raise HTTPException(status_code=404, detail="Layer not found.")

    source_id = layer_row["source_id"]
    is_scoped = _is_scoped(session, context)
    if is_scoped:
        if not _source_allowed(session, context, source_id, "read"):
            raise HTTPException(status_code=404, detail="Layer not found.")
        if not _source_allowed(session, context, source_id, "export"):
            raise HTTPException(status_code=403, detail="Export not permitted for this layer.")

    validation_state = str(layer_row["validation_state"])
    stored_crs_raw = layer_row["stored_crs_code"]
    stored_crs_code = int(stored_crs_raw) if stored_crs_raw is not None else None
    if not is_explicit_readable_crs(stored_crs_code, validation_state):
        raise HTTPException(
            status_code=422,
            detail=(
                "Layer CRS is missing, unverified, or definition-only; "
                "explicit readable EPSG CRS is required."
            ),
        )
    assert stored_crs_code is not None

    geometry_family = str(layer_row["geometry_family"])
    if geometry_family not in _SUPPORTED_EXPORT_GEOMETRIES:
        raise HTTPException(
            status_code=422,
            detail=(
                "Only single-part 2D Point, LineString, and Polygon layers "
                "are supported for export."
            ),
        )

    if format == "geojson" and stored_crs_code != 4326:
        raise HTTPException(
            status_code=422,
            detail=(
                "GeoJSON export requires EPSG:4326; use geopackage or postgis_sql "
                "to preserve native EPSG coordinates without unverified reprojection."
            ),
        )

    field_rows = (
        session.execute(
            _SOURCE_FIELDS_SQL,
            {"org": organization_id, "proj": project_id, "sid": source_id},
        )
        .mappings()
        .all()
    )
    field_mappings = [
        ExportFieldMapping(
            field_id=str(row["field_id"]),
            source_name=str(row["name"]),
            output_name=str(row["field_id"]),
            type=FieldType(str(row["type"])),
        )
        for row in field_rows
    ]

    features = _load_export_features(
        session,
        context,
        organization_id=organization_id,
        project_id=project_id,
        source_id=source_id,
        is_scoped=is_scoped,
        stored_crs_code=stored_crs_code,
    )
    if any(geom.type != geometry_family for _, _, geom in features):
        raise HTTPException(
            status_code=422,
            detail="Feature geometry type does not match layer geometry family.",
        )

    layer_name = str(layer_row["name"])
    try:
        if format == "geopackage":
            payload, ext, media_type = write_geopackage(
                layer_name=layer_name,
                crs_code=stored_crs_code,
                geometry_family=geometry_family,
                field_mappings=field_mappings,
                features=features,
            )
        elif format == "geojson":
            payload, ext, media_type = write_geojson(
                field_mappings=field_mappings,
                features=features,
            )
        else:
            payload, ext, media_type = write_postgis_sql(
                layer_name=layer_name,
                crs_code=stored_crs_code,
                geometry_family=geometry_family,
                field_mappings=field_mappings,
                features=features,
            )
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=str(exc)) from exc

    sha256 = hashlib.sha256(payload).hexdigest()
    content_revision = str(layer_row["content_revision"])
    schema_revision = str(layer_row["schema_revision"])
    feature_count = len(features)

    _record_export_audit(
        session,
        context,
        layer_id=layer_id,
        adapter_key=adapter.key,
        export_format=format,
        stored_crs_code=stored_crs_code,
        feature_count=feature_count,
        content_revision=content_revision,
        schema_revision=schema_revision,
        sha256=sha256,
        scoped_access=is_scoped,
    )

    filename = f"{_safe_filename_slug(layer_name)}-{adapter.key}.{ext}"
    return Response(
        content=payload,
        media_type=media_type,
        headers={
            "Cache-Control": "no-store",
            "Content-Disposition": f'attachment; filename="{filename}"',
            "X-Content-SHA256": sha256,
            "X-OpenVietGIS-CRS": f"EPSG:{stored_crs_code}",
            "X-OpenVietGIS-Content-Revision": content_revision,
            "X-OpenVietGIS-Schema-Revision": schema_revision,
            "X-OpenVietGIS-Feature-Count": str(feature_count),
        },
    )
