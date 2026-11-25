"""Authenticated project-scoped interoperability catalog, read-only exports, and controlled sync.

No database credentials or arbitrary outbound network/database connections are exposed.
Writes are limited to two-step revision-pinned QGIS GeoPackage sync via R006 changesets
with deletions blocked by default.
"""

from __future__ import annotations

import hashlib
import json
import math
import os
import re
import sqlite3
import tempfile
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Annotated, Any, Final, Literal
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, File, Form, HTTPException, Query, Response, UploadFile
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.cad.dxf_codec import write_dxf_2d
from openvietgis_api.adapters.formats.geojson import write_geojson
from openvietgis_api.adapters.formats.geopackage import (
    inspect_geopackage,
    parse_geopackage_member,
    write_geopackage,
)
from openvietgis_api.adapters.formats.postgis_sql import ascii_fold, write_postgis_sql
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
from openvietgis_api.adapters.persistence.editing_repository import SqlEditingRepository
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.service import AuthorizationService
from openvietgis_api.domains.cad.domain import CadUnit
from openvietgis_api.domains.editing.domain import (
    ChangesetRecord,
    EditOperationInput,
    EditOpType,
)
from openvietgis_api.domains.integration_hub.catalog import get_adapter, list_adapters
from openvietgis_api.domains.integration_hub.profiles import (
    PROFILE_ENABLED_ADAPTER_KEYS,
    LayerAccessSnapshot,
    build_adapter_profile,
    is_explicit_readable_crs,
)
from openvietgis_api.domains.integration_hub.sync_diff import (
    ServerFeatureSnapshot,
    SyncDiffPlan,
    UploadedFeatureSnapshot,
    compute_sync_diff,
)
from openvietgis_api.domains.platform_errors import PlatformValidationError
from openvietgis_api.domains.spatial.crs import get_readable_crs_profile
from openvietgis_api.domains.spatial.domain import (
    FieldType,
    GeometryDimensions,
    NativeGeometry,
    SpatialValidationError,
)
from openvietgis_api.domains.transfers.domain import ExportFieldMapping
from openvietgis_api.domains.transfers.ports import TransferFormatError

router = APIRouter(tags=["integration-hub"])

EXPORT_ENABLED_ADAPTER_KEYS: Final[frozenset[str]] = frozenset({"qgis", "postgis", "autocad"})
SYNC_PUSH_ENABLED_ADAPTER_KEYS: Final[frozenset[str]] = frozenset({"qgis"})
MAX_HUB_EXPORT_FEATURES: Final[int] = 25_000
MAX_DXF_EXPORT_FEATURES: Final[int] = 10_000
MAX_SYNC_UPLOAD_BYTES: Final[int] = 16 * 1024 * 1024
MAX_SYNC_PREVIEW_ITEMS: Final[int] = 100
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

_EXPORT_SELECT = """
    SELECT feature_id, attributes, ST_AsGeoJSON(ST_Transform(geometry, :out_srid)) AS geom_json
    FROM spatial.reference_features
    WHERE organization_id = :org
      AND project_id = :proj
      AND source_id = :sid
      AND geometry_state = 'present'
      AND geometry IS NOT NULL
"""
_EXPORT_ACL = """
      AND gis_access.feature_allowed(
          :org, :proj, :sid, feature_id, attributes, geometry, 'export'
      )
"""
# Spec 054 export scope (xã/ấp area, tuyến route). Fixed SQL fragments; every value is bound.
_ROUTE_ID = "coalesce(attributes->>'id', feature_id)"
_SCOPE_GEOMETRIES = """
    SELECT ST_Transform(geometry, 4326) AS geometry FROM spatial.reference_features
    WHERE organization_id = :org AND project_id = :proj AND geometry IS NOT NULL
      AND source_id = ANY(:{kind}_sids) AND {match}
"""
# Layers that record their hamlet (like the survey's "ap") match it by name, as the Bình Hưng app
# did; drawn hamlet boundaries and recorded hamlets disagree for ~13% of survey points.
_AREA_FILTERS = {
    "attribute": "\n      AND attributes->>'ap' = ANY(:area_names)",
    # Exporting the hamlet boundary layer itself: the chosen hamlets, not their neighbours.
    "own_name": "\n      AND attributes->>'name' = ANY(:area_names)",
    "spatial": (
        "\n      AND ST_Intersects(geometry, (SELECT ST_Transform(ST_Union(geometry),"
        " :native_srid) FROM ("
        + _SCOPE_GEOMETRIES.format(kind="area", match="feature_id = ANY(:area_fids)")
        + ") area))"
    ),
}
_ROUTE_FILTERS = {
    "self": f"\n      AND {_ROUTE_ID} = ANY(:route_ids)",
    "attribute": "\n      AND attributes->>'tuyen_id' = ANY(:route_ids)",
    "distance": (
        "\n      AND ST_DWithin(ST_Transform(geometry, 4326)::geography,"
        " (SELECT ST_Union(geometry)::geography FROM ("
        + _SCOPE_GEOMETRIES.format(kind="route", match=f"{_ROUTE_ID} = ANY(:route_ids)")
        + ") route), :route_distance_m)"
    ),
}


def _export_features_sql(*, scoped: bool, area_mode: str | None, route_mode: str | None) -> Any:
    sql = _EXPORT_SELECT + (_EXPORT_ACL if scoped else "")
    if area_mode is not None:
        sql += _AREA_FILTERS[area_mode]
    if route_mode is not None:
        sql += _ROUTE_FILTERS[route_mode]
    return text(sql + "\n    ORDER BY feature_id ASC\n    LIMIT :limit\n")


@dataclass(frozen=True)
class _ExportScope:
    area_sids: list[Any]
    hamlet_sids: list[Any]
    area_fids: list[str]
    area_names: list[str]
    route_sids: list[Any]
    route_ids: list[str]
    route_distance_m: float


_SCOPE_LAYERS_SQL = text(
    """
    SELECT l.id AS layer_id, l.name AS name, l.source_id AS source_id,
           s.geometry_family AS geometry_family
    FROM spatial.layers l
    JOIN spatial.data_sources s
      ON s.organization_id = l.organization_id AND s.project_id = l.project_id
     AND s.id = l.source_id
    WHERE l.organization_id = :org AND l.project_id = :proj AND l.lifecycle = 'active'
    """
)


def _scope_layers(
    session: Session, organization_id: UUID, project_id: UUID
) -> dict[str, list[Any]]:
    """Find commune, hamlet and route layers by name and geometry.

    shortcut: name-based detection (xã/phường, ấp, tuyến/đường/hẻm); add an explicit per-project
    setting if a project names its boundary layers differently.
    """
    found: dict[str, list[Any]] = {"commune": [], "hamlet": [], "route": []}
    rows = session.execute(_SCOPE_LAYERS_SQL, {"org": organization_id, "proj": project_id})
    for row in rows.mappings().all():
        words = set(re.findall(r"[a-z0-9]+", ascii_fold(str(row["name"])).lower()))
        family = str(row["geometry_family"])
        if family in ("Polygon", "MultiPolygon") and words & {"xa", "phuong"}:
            found["commune"].append(row["source_id"])
        elif family in ("Polygon", "MultiPolygon") and "ap" in words:
            found["hamlet"].append(row["source_id"])
        elif family in ("LineString", "MultiLineString") and words & {"tuyen", "duong", "hem"}:
            found["route"].append(row["source_id"])
    return found


_SYNC_FEATURES_SQL = text(
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

_SYNC_FEATURES_SCOPED_SQL = text(
    """
    SELECT feature_id, attributes, ST_AsGeoJSON(geometry) AS geom_json
    FROM spatial.reference_features
    WHERE organization_id = :org
      AND project_id = :proj
      AND source_id = :sid
      AND geometry_state = 'present'
      AND geometry IS NOT NULL
      AND gis_access.feature_allowed(
          :org, :proj, :sid, feature_id, attributes, geometry, 'read'
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
    slug = re.sub(r"[^a-z0-9_-]+", "-", ascii_fold(name).strip().lower()).strip("-_")
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


def _to_list_coords(coords: Any) -> Any:
    if isinstance(coords, tuple | list):
        return [_to_list_coords(item) for item in coords]
    return float(coords)


def _normalize_uploaded_decimal(val: Any) -> Any:
    if val is None or isinstance(val, bool):
        return val
    try:
        dec = Decimal(str(val))
    except InvalidOperation:
        return val
    if not dec.is_finite():
        return val
    if dec.is_zero():
        return "0"
    rendered = format(dec, "f")
    if "." in rendered:
        rendered = rendered.rstrip("0").rstrip(".")
    return rendered or "0"


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
    native_crs_code: int | None = None,
    scope: _ExportScope | None = None,
    area_mode: str | None = None,
    route_mode: str | None = None,
) -> list[tuple[str, dict[str, Any], NativeGeometry]]:
    del context  # Session RLS actor context is already bound by prepare_project_scope.
    rows = (
        session.execute(
            _export_features_sql(scoped=is_scoped, area_mode=area_mode, route_mode=route_mode),
            {
                "org": organization_id,
                "proj": project_id,
                "sid": source_id,
                "limit": MAX_HUB_EXPORT_FEATURES + 1,
                # Geometry is returned in this CRS; ST_Transform is a no-op when it equals storage.
                "out_srid": stored_crs_code,
                "native_srid": native_crs_code or stored_crs_code,
                "area_sids": scope.area_sids if scope else [],
                "area_fids": scope.area_fids if scope else [],
                "area_names": scope.area_names if scope else [],
                "route_sids": scope.route_sids if scope else [],
                "route_ids": scope.route_ids if scope else [],
                "route_distance_m": scope.route_distance_m if scope else 0.0,
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


@dataclass(frozen=True, slots=True)
class _PreparedSyncContext:
    adapter_key: str
    source_id: UUID
    stored_crs_code: int
    geometry_family: str
    current_content_revision: str
    current_schema_revision: str
    stale_revision: bool
    plan: SyncDiffPlan


def _load_sync_server_features(
    session: Session,
    context: AuthorizationContext,
    *,
    organization_id: UUID,
    project_id: UUID,
    source_id: object,
    is_scoped: bool,
) -> list[ServerFeatureSnapshot]:
    del context  # Session RLS actor context is already bound by prepare_project_scope.
    rows = (
        session.execute(
            _SYNC_FEATURES_SCOPED_SQL if is_scoped else _SYNC_FEATURES_SQL,
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
            detail="Sync feature limit of 25,000 server features exceeded.",
        )

    snapshots: list[ServerFeatureSnapshot] = []
    for row in rows:
        raw_geom = row["geom_json"]
        if raw_geom is None:
            continue
        try:
            geom_obj = json.loads(raw_geom) if isinstance(raw_geom, str) else raw_geom
            if not isinstance(geom_obj, dict):
                raise ValueError("Server feature geometry must be a GeoJSON object.")
        except (ValueError, TypeError) as exc:
            raise HTTPException(
                status_code=422,
                detail="Layer contains invalid or unsupported feature geometry.",
            ) from exc
        raw_attrs = row["attributes"]
        attrs = dict(raw_attrs) if isinstance(raw_attrs, Mapping) else {}
        snapshots.append(
            ServerFeatureSnapshot(
                feature_id=str(row["feature_id"]),
                attributes=attrs,
                geometry_geojson=geom_obj,
            )
        )
    return snapshots


def _read_bounded_sync_upload(file: UploadFile) -> bytes:
    raw = file.file.read(MAX_SYNC_UPLOAD_BYTES + 1)
    if len(raw) > MAX_SYNC_UPLOAD_BYTES:
        raise HTTPException(
            status_code=413,
            detail="Uploaded GeoPackage exceeds maximum size of 16 MiB.",
        )
    if not raw:
        raise HTTPException(
            status_code=422,
            detail="Uploaded GeoPackage file is empty.",
        )
    return raw


def _check_duplicate_gpkg_feature_ids(raw: bytes, table_name: str) -> None:
    if not raw.startswith(b"SQLite format 3\x00"):
        return
    quoted = '"' + table_name.replace('"', '""') + '"'
    fd, tmp_name = tempfile.mkstemp(suffix=".gpkg")
    tmp_path = Path(tmp_name)
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(raw)
        conn = sqlite3.connect(f"file:{tmp_path}?mode=ro", uri=True)
        try:
            cols = {
                str(row[1])
                for row in conn.execute(f"PRAGMA table_info({quoted})").fetchall()
            }
            if "ovg_feature_id" not in cols:
                return
            dup = conn.execute(
                f"SELECT ovg_feature_id FROM {quoted} "
                "WHERE ovg_feature_id IS NOT NULL "
                "GROUP BY ovg_feature_id HAVING COUNT(*) > 1 LIMIT 1"
            ).fetchone()
            if dup is not None:
                raise HTTPException(
                    status_code=422,
                    detail=f"Duplicate ovg_feature_id in uploaded GeoPackage: {dup[0]}",
                )
        finally:
            conn.close()
    finally:
        tmp_path.unlink(missing_ok=True)


def _parse_sync_geopackage(
    raw: bytes,
    *,
    expected_crs_code: int,
    expected_geometry_family: str,
    field_types_by_id: Mapping[str, FieldType],
) -> list[UploadedFeatureSnapshot]:
    try:
        members = inspect_geopackage(raw)
    except TransferFormatError as exc:
        raise HTTPException(status_code=422, detail=exc.message) from exc

    vector_members = [m for m in members if m.geometry_family != "unknown"]
    if not vector_members:
        raise HTTPException(
            status_code=422,
            detail="Uploaded GeoPackage contains no vector feature layers.",
        )
    if len(vector_members) > 1:
        raise HTTPException(
            status_code=422,
            detail="Uploaded GeoPackage must contain exactly one vector layer for sync.",
        )
    member = vector_members[0]
    if not member.eligible:
        raise HTTPException(
            status_code=422,
            detail=member.reason or "Uploaded GeoPackage layer is not eligible for sync.",
        )
    if member.detected_crs is None:
        raise HTTPException(
            status_code=422,
            detail="Uploaded GeoPackage is missing an explicit supported EPSG CRS.",
        )
    if member.detected_crs != expected_crs_code:
        raise HTTPException(
            status_code=422,
            detail=(
                f"Uploaded GeoPackage CRS EPSG:{member.detected_crs} does not match "
                f"layer CRS EPSG:{expected_crs_code}."
            ),
        )

    _check_duplicate_gpkg_feature_ids(raw, member.name)

    try:
        parsed = parse_geopackage_member(
            raw,
            member_name=member.name,
            declared_crs=expected_crs_code,
        )
    except TransferFormatError as exc:
        raise HTTPException(status_code=422, detail=exc.message) from exc

    error_findings = [f for f in parsed.findings if f.severity == "error"]
    if error_findings:
        first = error_findings[0]
        raise HTTPException(
            status_code=422,
            detail=f"Uploaded GeoPackage validation failed ({first.code}): {first.message}",
        )
    if parsed.detected_crs != expected_crs_code:
        raise HTTPException(
            status_code=422,
            detail=(
                f"Uploaded GeoPackage CRS does not match layer CRS EPSG:{expected_crs_code}."
            ),
        )
    if parsed.dimensions != GeometryDimensions.XY:
        raise HTTPException(
            status_code=422,
            detail="Only 2D XY GeoPackage layers are supported for sync.",
        )
    if parsed.geometry_family.value != expected_geometry_family:
        raise HTTPException(
            status_code=422,
            detail=(
                f"Uploaded GeoPackage geometry family '{parsed.geometry_family.value}' "
                f"does not match layer geometry family '{expected_geometry_family}'."
            ),
        )

    uploaded: list[UploadedFeatureSnapshot] = []
    for rec in parsed.features:
        attrs: dict[str, Any] = {}
        for key, value in rec.attributes.items():
            if field_types_by_id.get(key) is FieldType.DECIMAL:
                attrs[key] = _normalize_uploaded_decimal(value)
            else:
                attrs[key] = value
        uploaded.append(
            UploadedFeatureSnapshot(
                feature_id=rec.feature_id,
                attributes=attrs,
                geometry_geojson={
                    "type": rec.geometry.type,
                    "coordinates": _to_list_coords(rec.geometry.coordinates),
                },
            )
        )
    return uploaded


def _prepare_sync_plan(
    session: Session,
    *,
    context: AuthorizationContext,
    organization_id: UUID,
    project_id: UUID,
    adapter_key: str,
    layer_id: UUID,
    file: UploadFile,
    base_content_revision: str,
    base_schema_revision: str,
    allow_deletes: bool = False,
) -> _PreparedSyncContext:
    adapter = get_adapter(adapter_key)
    if adapter is None:
        raise HTTPException(status_code=404, detail="Integration adapter not found.")
    if adapter.key not in SYNC_PUSH_ENABLED_ADAPTER_KEYS:
        raise HTTPException(
            status_code=409,
            detail="GeoPackage sync push is not enabled for this adapter.",
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

    raw_source_id = layer_row["source_id"]
    is_scoped = _is_scoped(session, context)
    if is_scoped and not _source_allowed(session, context, raw_source_id, "read"):
        raise HTTPException(status_code=404, detail="Layer not found.")

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
                "are supported for sync."
            ),
        )

    field_rows = (
        session.execute(
            _SOURCE_FIELDS_SQL,
            {"org": organization_id, "proj": project_id, "sid": raw_source_id},
        )
        .mappings()
        .all()
    )
    field_types_by_id = {
        str(row["field_id"]): FieldType(str(row["type"])) for row in field_rows
    }
    allowed_field_ids = frozenset(field_types_by_id.keys())

    raw_payload = _read_bounded_sync_upload(file)
    uploaded_features = _parse_sync_geopackage(
        raw_payload,
        expected_crs_code=stored_crs_code,
        expected_geometry_family=geometry_family,
        field_types_by_id=field_types_by_id,
    )
    server_features = _load_sync_server_features(
        session,
        context,
        organization_id=organization_id,
        project_id=project_id,
        source_id=raw_source_id,
        is_scoped=is_scoped,
    )
    try:
        plan = compute_sync_diff(
            server_features=server_features,
            uploaded_features=uploaded_features,
            allowed_field_ids=allowed_field_ids,
            base_content_revision=base_content_revision,
            base_schema_revision=base_schema_revision,
            allow_deletes=allow_deletes,
        )
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=str(exc)) from exc

    current_content_revision = str(layer_row["content_revision"])
    current_schema_revision = str(layer_row["schema_revision"])
    stale_revision = (
        base_content_revision != current_content_revision
        or base_schema_revision != current_schema_revision
    )
    source_uuid = (
        raw_source_id if isinstance(raw_source_id, UUID) else UUID(str(raw_source_id))
    )
    return _PreparedSyncContext(
        adapter_key=adapter.key,
        source_id=source_uuid,
        stored_crs_code=stored_crs_code,
        geometry_family=geometry_family,
        current_content_revision=current_content_revision,
        current_schema_revision=current_schema_revision,
        stale_revision=stale_revision,
        plan=plan,
    )


def _commit_sync_changeset(
    session: Session,
    *,
    context: AuthorizationContext,
    source_id: UUID,
    base_content_revision: str,
    base_schema_revision: str,
    operation_id: str,
    summary: str,
    operations: Sequence[EditOperationInput],
) -> ChangesetRecord:
    repo = SqlEditingRepository(session)
    return repo.commit_changeset(
        context=context,
        source_id=source_id,
        base_content_revision=base_content_revision,
        base_schema_revision=base_schema_revision,
        operation_id=operation_id,
        summary=summary,
        operations=operations,
    )


_SCOPE_AREAS_SQL = text(
    """
    SELECT feature_id, attributes FROM spatial.reference_features
    WHERE organization_id = :org AND project_id = :proj AND source_id = ANY(:sids)
      AND geometry IS NOT NULL
    """
)
_SCOPE_ROUTES_SQL = text(
    """
    SELECT r.attributes->>'ten' AS name, coalesce(r.attributes->>'id', r.feature_id) AS route_id,
           array_remove(array_agg(DISTINCT h.feature_id), NULL) AS hamlet_ids
    FROM spatial.reference_features r
    LEFT JOIN spatial.reference_features h
      ON h.organization_id = r.organization_id AND h.project_id = r.project_id
     AND h.source_id = ANY(:hamlet_sids) AND h.geometry IS NOT NULL
     AND ST_Intersects(h.geometry, ST_Transform(r.geometry, ST_SRID(h.geometry)))
    WHERE r.organization_id = :org AND r.project_id = :proj AND r.source_id = ANY(:route_sids)
      AND r.geometry IS NOT NULL AND coalesce(r.attributes->>'ten', '') <> ''
    GROUP BY r.feature_id, r.attributes
    """
)


def _scope_hamlet_names(
    session: Session,
    organization_id: UUID,
    project_id: UUID,
    sources: dict[str, list[Any]],
    area_fids: list[str],
) -> list[str]:
    """Hamlet names covered by the chosen communes/hamlets (a commune covers its ma_xa hamlets)."""
    if not area_fids or not sources["hamlet"]:
        return []
    params = {"org": organization_id, "proj": project_id}
    hamlets = session.execute(_SCOPE_AREAS_SQL, params | {"sids": sources["hamlet"]})
    hamlet_rows = list(hamlets.mappings().all())
    chosen = set(area_fids)
    codes: set[str] = set()
    if sources["commune"]:
        communes = session.execute(_SCOPE_AREAS_SQL, params | {"sids": sources["commune"]})
        codes = {
            str(row["attributes"].get("ma_xa"))
            for row in communes.mappings().all()
            if str(row["feature_id"]) in chosen and row["attributes"].get("ma_xa")
        }
    return [
        str(row["attributes"]["name"])
        for row in hamlet_rows
        if row["attributes"].get("name")
        and (str(row["feature_id"]) in chosen or str(row["attributes"].get("ma_xa")) in codes)
    ]


def _natural_key(name: str) -> tuple[str, int, str]:
    match = re.search(r"\d+", name)
    return (name[: match.start()] if match else name, int(match.group()) if match else 0, name)


@router.get(
    "/organizations/{organization_id}/projects/{project_id}/integration-hub/export-scopes",
    dependencies=[Depends(_set_no_store_headers)],
)
def get_export_scopes(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    """Commune -> hamlet -> route choices for scoped export (Spec 054), under map read scope."""
    context = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(session, context, permission=Permission.PROJECT_READ)
    sources = _scope_layers(session, organization_id, project_id)
    params = {"org": organization_id, "proj": project_id}

    def areas(kind: str) -> list[Any]:
        if not sources[kind]:
            return []
        rows = session.execute(_SCOPE_AREAS_SQL, params | {"sids": sources[kind]})
        return list(rows.mappings().all())

    communes = [
        {
            "id": str(row["feature_id"]),
            "name": str(row["attributes"].get("name") or row["attributes"].get("ten") or "Xã"),
            "code": str(row["attributes"].get("ma_xa") or ""),
        }
        for row in areas("commune")
    ]
    hamlets = [
        {
            "id": str(row["feature_id"]),
            "name": str(row["attributes"].get("name") or row["attributes"].get("ten") or "Ấp"),
            "commune_code": str(row["attributes"].get("ma_xa") or ""),
        }
        for row in areas("hamlet")
    ]
    routes: dict[str, dict[str, Any]] = {}
    if sources["route"]:
        rows = session.execute(
            _SCOPE_ROUTES_SQL,
            params | {"route_sids": sources["route"], "hamlet_sids": sources["hamlet"]},
        )
        for row in rows.mappings().all():
            name = " ".join(str(row["name"]).split())
            entry = routes.setdefault(
                ascii_fold(name).lower(), {"name": name, "route_ids": [], "hamlet_ids": set()}
            )
            entry["route_ids"].append(str(row["route_id"]))
            entry["hamlet_ids"].update(str(h) for h in row["hamlet_ids"] or [])
    return {
        "communes": sorted(communes, key=lambda c: _natural_key(c["name"])),
        "hamlets": sorted(hamlets, key=lambda h: _natural_key(h["name"])),
        "routes": sorted(
            (
                {
                    "name": r["name"],
                    "route_ids": r["route_ids"],
                    "hamlet_ids": sorted(r["hamlet_ids"]),
                }
                for r in routes.values()
            ),
            key=lambda r: _natural_key(r["name"]),
        ),
    }


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
    format: Literal["geopackage", "geojson", "postgis_sql", "dxf"] = "geopackage",
    scope_area: Annotated[list[str] | None, Query(max_length=200)] = None,
    scope_route: Annotated[list[str] | None, Query(max_length=500)] = None,
    route_distance_m: Annotated[float, Query(ge=1, le=200)] = 30.0,
) -> Response:
    context = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(session, context, permission=Permission.PROJECT_READ)
    scope: _ExportScope | None = None
    if scope_area or scope_route:
        if any(not v or len(v) > 128 for v in [*(scope_area or []), *(scope_route or [])]):
            raise HTTPException(status_code=422, detail="Invalid export scope identifier.")
        scope_sources = _scope_layers(session, organization_id, project_id)
        scope = _ExportScope(
            area_sids=scope_sources["commune"] + scope_sources["hamlet"],
            hamlet_sids=scope_sources["hamlet"],
            area_fids=list(scope_area or []),
            area_names=_scope_hamlet_names(
                session, organization_id, project_id, scope_sources, list(scope_area or [])
            ),
            route_sids=scope_sources["route"],
            route_ids=list(scope_route or []),
            route_distance_m=route_distance_m,
        )

    adapter = get_adapter(adapter_key)
    if adapter is None:
        raise HTTPException(status_code=404, detail="Integration adapter not found.")
    if adapter.key not in EXPORT_ENABLED_ADAPTER_KEYS:
        raise HTTPException(
            status_code=409,
            detail="Read-only hub export is not enabled for this adapter.",
        )
    if adapter.key == "autocad" and format != "dxf":
        raise HTTPException(
            status_code=422,
            detail="AutoCAD adapter supports format=dxf only.",
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

    # Only the exact, datum-free EPSG:3857 <-> EPSG:4326 conversion is applied; every other CRS
    # still has to match the format natively.
    output_crs_code = stored_crs_code
    if format == "geojson" and stored_crs_code == 3857:
        output_crs_code = 4326
    if format == "dxf" and stored_crs_code == 4326:
        output_crs_code = 3857

    if format == "geojson" and output_crs_code != 4326:
        raise HTTPException(
            status_code=422,
            detail=(
                "GeoJSON export requires EPSG:4326 or EPSG:3857; use geopackage or postgis_sql "
                "to preserve native EPSG coordinates without unverified reprojection."
            ),
        )

    if format == "dxf":
        crs_profile = get_readable_crs_profile(output_crs_code)
        if crs_profile is None or crs_profile.unit != "metre":
            raise HTTPException(
                status_code=422,
                detail=(
                    "DXF export requires a projected metre CRS (EPSG:5899 or EPSG:3857); "
                    "geographic degree CRS is not supported."
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

    def has_field(name: str) -> bool:
        return any(name in (row["name"], row["field_id"]) for row in field_rows)

    area_mode: str | None = None
    if scope is not None and scope.area_fids:
        # Hamlet layer: the chosen hamlets by name; layers recording "ap": by that name;
        # the commune layer and everything else: the drawn boundary.
        if source_id in scope.hamlet_sids and scope.area_names:
            area_mode = "own_name"
        elif source_id not in scope.area_sids and has_field("ap") and scope.area_names:
            area_mode = "attribute"
        else:
            area_mode = "spatial"
    route_mode: str | None = None
    if scope is not None and scope.route_ids:
        # Route layers match their own ids, layers carrying tuyen_id match it, others use distance.
        if source_id in scope.route_sids:
            route_mode = "self"
        elif has_field("tuyen_id"):
            route_mode = "attribute"
        else:
            route_mode = "distance"

    features = _load_export_features(
        session,
        context,
        organization_id=organization_id,
        project_id=project_id,
        source_id=source_id,
        is_scoped=is_scoped,
        stored_crs_code=output_crs_code,
        native_crs_code=stored_crs_code,
        scope=scope,
        area_mode=area_mode,
        route_mode=route_mode,
    )
    if scope is not None and not features:
        raise HTTPException(status_code=422, detail="No features in the selected scope.")
    if any(geom.type != geometry_family for _, _, geom in features):
        raise HTTPException(
            status_code=422,
            detail="Feature geometry type does not match layer geometry family.",
        )

    if format == "dxf" and not (1 <= len(features) <= MAX_DXF_EXPORT_FEATURES):
        raise HTTPException(
            status_code=422,
            detail="DXF export requires between 1 and 10,000 features.",
        )

    layer_name = str(layer_row["name"])
    try:
        if format == "geopackage":
            payload, ext, media_type = write_geopackage(
                layer_name=layer_name,
                crs_code=output_crs_code,
                geometry_family=geometry_family,
                field_mappings=field_mappings,
                features=features,
            )
        elif format == "geojson":
            payload, ext, media_type = write_geojson(
                field_mappings=field_mappings,
                features=features,
            )
        elif format == "dxf":
            dxf_features = [
                {
                    "feature_id": fid,
                    "geometry": {
                        "type": geom.type,
                        "coordinates": _to_list_coords(geom.coordinates),
                    },
                }
                for fid, _, geom in features
            ]
            payload = write_dxf_2d(
                dxf_features,
                layer_name=layer_name,
                units=CadUnit.METER,
            )
            ext = "dxf"
            media_type = "application/dxf"
        else:
            payload, ext, media_type = write_postgis_sql(
                layer_name=layer_name,
                crs_code=output_crs_code,
                geometry_family=geometry_family,
                field_mappings=field_mappings,
                features=features,
            )
    except PlatformValidationError as exc:
        raise HTTPException(status_code=422, detail=exc.message) from exc
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
        stored_crs_code=output_crs_code,
        feature_count=feature_count,
        content_revision=content_revision,
        schema_revision=schema_revision,
        sha256=sha256,
        scoped_access=is_scoped,
    )

    filename = f"{_safe_filename_slug(layer_name)}-{adapter.key}.{ext}"
    headers = {
        "Cache-Control": "no-store",
        "Content-Disposition": f'attachment; filename="{filename}"',
        "X-Content-SHA256": sha256,
        "X-OpenVietGIS-CRS": f"EPSG:{output_crs_code}",
        "X-OpenVietGIS-Content-Revision": content_revision,
        "X-OpenVietGIS-Schema-Revision": schema_revision,
        "X-OpenVietGIS-Feature-Count": str(feature_count),
    }
    if format == "dxf":
        headers["X-OpenVietGIS-Attribute-Loss"] = "true"
    if scope is not None:
        headers["X-OpenVietGIS-Scope"] = ",".join(
            part
            for part in (
                f"area-{area_mode}" if area_mode else "",
                f"route-distance-{scope.route_distance_m:g}m" if route_mode == "distance" else "",
                f"route-{route_mode}" if route_mode in ("self", "attribute") else "",
            )
            if part
        )
    return Response(
        content=payload,
        media_type=media_type,
        headers=headers,
    )


@router.post(
    "/organizations/{organization_id}/projects/{project_id}"
    "/integration-hub/adapters/{adapter_key}/layers/{layer_id}/sync-preview",
    dependencies=[Depends(_set_no_store_headers)],
)
def preview_integration_layer_sync(
    organization_id: UUID,
    project_id: UUID,
    adapter_key: str,
    layer_id: UUID,
    file: Annotated[UploadFile, File()],
    base_content_revision: Annotated[str, Form(pattern=r"^[a-f0-9]{64}$")],
    base_schema_revision: Annotated[str, Form(pattern=r"^[a-f0-9]{64}$")],
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    allow_deletes: Annotated[bool, Form()] = False,
) -> dict[str, object]:
    context = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(session, context, permission=Permission.PROJECT_EDIT)
    prepared = _prepare_sync_plan(
        session,
        context=context,
        organization_id=organization_id,
        project_id=project_id,
        adapter_key=adapter_key,
        layer_id=layer_id,
        file=file,
        base_content_revision=base_content_revision,
        base_schema_revision=base_schema_revision,
        allow_deletes=allow_deletes,
    )
    plan = prepared.plan
    return {
        "schema_version": 1,
        "adapter_key": prepared.adapter_key,
        "layer_id": str(layer_id),
        "source_id": str(prepared.source_id),
        "stored_crs_code": prepared.stored_crs_code,
        "geometry_family": prepared.geometry_family,
        "base_content_revision": base_content_revision,
        "current_content_revision": prepared.current_content_revision,
        "base_schema_revision": base_schema_revision,
        "current_schema_revision": prepared.current_schema_revision,
        "stale_revision": prepared.stale_revision,
        "deletions_blocked": not allow_deletes,
        "create_count": plan.create_count,
        "update_count": plan.update_count,
        "delete_count": plan.delete_count,
        "unchanged_count": plan.unchanged_count,
        "omitted_server_feature_count": len(plan.omitted_server_feature_ids),
        "omitted_server_feature_ids": list(
            plan.omitted_server_feature_ids[:MAX_SYNC_PREVIEW_ITEMS]
        ),
        "diff_sha256": plan.diff_sha256,
        "can_commit": not prepared.stale_revision
        and (plan.create_count + plan.update_count + plan.delete_count) > 0,
        "operations": [
            {
                "op_type": op.op_type,
                "feature_id": op.feature_id,
                "changed_fields": list(op.changed_fields),
                "geometry_changed": op.geometry_changed,
            }
            for op in plan.operations[:MAX_SYNC_PREVIEW_ITEMS]
        ],
    }


@router.post(
    "/organizations/{organization_id}/projects/{project_id}"
    "/integration-hub/adapters/{adapter_key}/layers/{layer_id}/sync-push",
    status_code=201,
    dependencies=[Depends(_set_no_store_headers)],
)
def push_integration_layer_sync(
    organization_id: UUID,
    project_id: UUID,
    adapter_key: str,
    layer_id: UUID,
    file: Annotated[UploadFile, File()],
    base_content_revision: Annotated[str, Form(pattern=r"^[a-f0-9]{64}$")],
    base_schema_revision: Annotated[str, Form(pattern=r"^[a-f0-9]{64}$")],
    operation_id: Annotated[str, Form(min_length=1, max_length=128)],
    expected_diff_sha256: Annotated[str, Form(pattern=r"^[a-f0-9]{64}$")],
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    summary: Annotated[str, Form(max_length=500)] = "QGIS GeoPackage sync push",
    allow_deletes: Annotated[bool, Form()] = False,
) -> dict[str, object]:
    context = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(session, context, permission=Permission.PROJECT_EDIT)

    if operation_id.strip().startswith("field-sync-"):
        raise HTTPException(
            status_code=422,
            detail="Reserved internal changeset operation_id prefix 'field-sync-' is not allowed.",
        )

    prepared = _prepare_sync_plan(
        session,
        context=context,
        organization_id=organization_id,
        project_id=project_id,
        adapter_key=adapter_key,
        layer_id=layer_id,
        file=file,
        base_content_revision=base_content_revision,
        base_schema_revision=base_schema_revision,
        allow_deletes=allow_deletes,
    )
    if prepared.stale_revision:
        raise HTTPException(
            status_code=409,
            detail="Layer revision changed since export; re-export and preview before syncing.",
        )

    plan = prepared.plan
    if plan.diff_sha256 != expected_diff_sha256:
        raise HTTPException(
            status_code=409,
            detail=(
                "Uploaded GeoPackage diff does not match expected_diff_sha256; "
                "run sync-preview again."
            ),
        )
    if len(plan.operations) == 0:
        raise HTTPException(
            status_code=409,
            detail="No material creates, updates, or deletes to sync.",
        )

    edit_operations = [
        EditOperationInput(
            op_type=(
                EditOpType.CREATE
                if op.op_type == "create"
                else EditOpType.DELETE
                if op.op_type == "delete"
                else EditOpType.UPDATE
            ),
            feature_id=op.feature_id,
            geometry=None if op.op_type == "delete" else op.geometry,
            properties=None if op.op_type == "delete" else op.properties,
        )
        for op in plan.operations
    ]
    changeset = _commit_sync_changeset(
        session,
        context=context,
        source_id=prepared.source_id,
        base_content_revision=base_content_revision,
        base_schema_revision=base_schema_revision,
        operation_id=operation_id,
        summary=summary,
        operations=edit_operations,
    )
    return {
        "changeset_id": str(changeset.id),
        "source_id": str(changeset.source_id),
        "layer_id": str(layer_id),
        "operation_id": changeset.operation_id,
        "resulting_content_revision": changeset.resulting_content_revision,
        "resulting_catalog_revision": changeset.resulting_catalog_revision,
        "operation_count": changeset.operation_count,
        "create_count": plan.create_count,
        "update_count": plan.update_count,
        "delete_count": plan.delete_count,
        "omitted_server_feature_count": len(plan.omitted_server_feature_ids),
        "deletions_blocked": not allow_deletes,
        "diff_sha256": plan.diff_sha256,
        "audit_event_id": str(changeset.audit_event_id),
    }
