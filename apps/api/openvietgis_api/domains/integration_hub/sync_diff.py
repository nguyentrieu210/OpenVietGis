"""Pure domain diff planner for two-step revision-pinned GeoPackage sync push.

Computes deterministic create/update operations against a server snapshot while
blocking automatic feature deletions by default.
"""

from __future__ import annotations

import hashlib
import json
import math
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Any, Final, Literal

MAX_SYNC_OPERATIONS: Final[int] = 1000
_SUPPORTED_SYNC_GEOMETRIES: Final[frozenset[str]] = frozenset(
    {"Point", "LineString", "Polygon"}
)
_IGNORED_UPLOAD_COLUMNS: Final[frozenset[str]] = frozenset({"ovg_feature_id", "fid"})


@dataclass(frozen=True, slots=True)
class ServerFeatureSnapshot:
    feature_id: str
    attributes: dict[str, Any]
    geometry_geojson: dict[str, Any]


@dataclass(frozen=True, slots=True)
class UploadedFeatureSnapshot:
    feature_id: str
    attributes: dict[str, Any]
    geometry_geojson: dict[str, Any]


@dataclass(frozen=True, slots=True)
class SyncOperationPreview:
    op_type: Literal["create", "update", "delete"]
    feature_id: str
    geometry: dict[str, Any]
    properties: dict[str, Any]
    changed_fields: tuple[str, ...]
    geometry_changed: bool


@dataclass(frozen=True, slots=True)
class SyncDiffPlan:
    operations: tuple[SyncOperationPreview, ...]
    create_count: int
    update_count: int
    unchanged_count: int
    omitted_server_feature_ids: tuple[str, ...]
    diff_sha256: str
    delete_count: int = 0


def _normalize_coords(coords: Any, depth: int = 0) -> Any:
    if depth > 4:
        raise ValueError("Geometry coordinate nesting exceeds supported 2D depth.")
    if isinstance(coords, list | tuple):
        return [_normalize_coords(item, depth + 1) for item in coords]
    if isinstance(coords, bool) or not isinstance(coords, int | float):
        raise ValueError("Geometry coordinates must be finite numbers.")
    val = float(coords)
    if not math.isfinite(val):
        raise ValueError("Geometry coordinates must be finite numbers.")
    return 0.0 if val == 0.0 else val


def _normalize_geometry(geometry: Mapping[str, Any]) -> dict[str, Any]:
    if not isinstance(geometry, Mapping):
        raise ValueError("Feature geometry must be a GeoJSON object.")
    geom_type = geometry.get("type")
    if geom_type not in _SUPPORTED_SYNC_GEOMETRIES:
        raise ValueError(f"Unsupported geometry type for sync: {geom_type!r}")
    coords = _normalize_coords(geometry.get("coordinates"))
    return {"type": str(geom_type), "coordinates": coords}


def _values_equal(left: Any, right: Any) -> bool:
    if left is None and right is None:
        return True
    if left is None or right is None:
        return False
    if isinstance(left, bool) or isinstance(right, bool):
        return type(left) is type(right) and left == right
    if isinstance(left, int | float) and isinstance(right, int | float):
        return float(left) == float(right)
    return bool(left == right)


def _project_attributes(
    raw_attributes: Mapping[str, Any],
    sorted_field_ids: tuple[str, ...],
) -> dict[str, Any]:
    return {
        field_id: raw_attributes.get(field_id)
        for field_id in sorted_field_ids
        if field_id not in _IGNORED_UPLOAD_COLUMNS
    }


def _compute_diff_sha256(
    *,
    base_content_revision: str,
    base_schema_revision: str,
    operations: Sequence[SyncOperationPreview],
) -> str:
    canonical = json.dumps(
        {
            "base_content_revision": base_content_revision,
            "base_schema_revision": base_schema_revision,
            "operations": [
                {
                    "op_type": op.op_type,
                    "feature_id": op.feature_id,
                    "geometry": op.geometry,
                    "properties": op.properties,
                }
                for op in operations
            ],
        },
        sort_keys=True,
        separators=(",", ":"),
        allow_nan=False,
    ).encode("utf-8")
    return hashlib.sha256(canonical).hexdigest()


def compute_sync_diff(
    *,
    server_features: Sequence[ServerFeatureSnapshot],
    uploaded_features: Sequence[UploadedFeatureSnapshot],
    allowed_field_ids: frozenset[str],
    base_content_revision: str,
    base_schema_revision: str,
    max_operations: int = MAX_SYNC_OPERATIONS,
    allow_deletes: bool = False,
) -> SyncDiffPlan:
    """Compute a deterministic, deletion-free sync diff plan.

    Server features missing from the upload are reported in
    ``omitted_server_feature_ids`` and are never auto-deleted.
    """
    if max_operations < 1:
        raise ValueError("max_operations must be at least 1.")

    sorted_field_ids = tuple(sorted(allowed_field_ids - _IGNORED_UPLOAD_COLUMNS))
    server_by_id: dict[str, ServerFeatureSnapshot] = {
        feat.feature_id: feat for feat in server_features
    }

    seen_uploaded_ids: set[str] = set()
    operations: list[SyncOperationPreview] = []
    create_count = 0
    update_count = 0
    unchanged_count = 0

    for uploaded in uploaded_features:
        feature_id = uploaded.feature_id.strip()
        if not feature_id:
            raise ValueError("Uploaded feature_id must be a non-empty string.")
        if feature_id in seen_uploaded_ids:
            raise ValueError(f"Duplicate feature_id in uploaded features: {feature_id}")
        seen_uploaded_ids.add(feature_id)

        norm_uploaded_geom = _normalize_geometry(uploaded.geometry_geojson)
        uploaded_props = _project_attributes(uploaded.attributes, sorted_field_ids)

        existing = server_by_id.get(feature_id)
        if existing is None:
            changed_fields = tuple(
                fid for fid in sorted_field_ids if uploaded_props.get(fid) is not None
            )
            operations.append(
                SyncOperationPreview(
                    op_type="create",
                    feature_id=feature_id,
                    geometry=norm_uploaded_geom,
                    properties=uploaded_props,
                    changed_fields=changed_fields,
                    geometry_changed=True,
                )
            )
            create_count += 1
            continue

        norm_server_geom = _normalize_geometry(existing.geometry_geojson)
        server_props = _project_attributes(existing.attributes, sorted_field_ids)
        geometry_changed = norm_server_geom != norm_uploaded_geom
        changed_fields = tuple(
            fid
            for fid in sorted_field_ids
            if not _values_equal(server_props.get(fid), uploaded_props.get(fid))
        )
        if not geometry_changed and not changed_fields:
            unchanged_count += 1
            continue

        operations.append(
            SyncOperationPreview(
                op_type="update",
                feature_id=feature_id,
                geometry=norm_uploaded_geom,
                properties=uploaded_props,
                changed_fields=changed_fields,
                geometry_changed=geometry_changed,
            )
        )
        update_count += 1

    missing_server_ids = sorted(fid for fid in server_by_id if fid not in seen_uploaded_ids)
    delete_count = 0
    if allow_deletes:
        for fid in missing_server_ids:
            operations.append(
                SyncOperationPreview(
                    op_type="delete",
                    feature_id=fid,
                    geometry={},
                    properties={},
                    changed_fields=(),
                    geometry_changed=False,
                )
            )
            delete_count += 1
        omitted_server_feature_ids: tuple[str, ...] = ()
    else:
        omitted_server_feature_ids = tuple(missing_server_ids)

    if len(operations) > max_operations:
        raise ValueError(f"Sync push exceeds maximum {max_operations:,} changed operations")

    sorted_operations = tuple(
        sorted(operations, key=lambda op: (op.feature_id, op.op_type))
    )
    diff_sha256 = _compute_diff_sha256(
        base_content_revision=base_content_revision,
        base_schema_revision=base_schema_revision,
        operations=sorted_operations,
    )
    return SyncDiffPlan(
        operations=sorted_operations,
        create_count=create_count,
        update_count=update_count,
        unchanged_count=unchanged_count,
        omitted_server_feature_ids=omitted_server_feature_ids,
        diff_sha256=diff_sha256,
        delete_count=delete_count,
    )
