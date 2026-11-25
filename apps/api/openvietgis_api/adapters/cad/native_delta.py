"""Bounded, side-effect-free R048 native DXF geometry exchange.

This is a qualification seam over the existing DXF codec and R006 operations.
It deliberately has no HTTP route, write authority, or automatic conflict merge.
"""

from __future__ import annotations

import hashlib
import io
import re
from copy import deepcopy
from typing import Any, NoReturn

from ezdxf.entities.lwpolyline import LWPolyline
from ezdxf.filemanagement import read

from openvietgis_api.adapters.cad.dxf_codec import parse_dxf_2d
from openvietgis_api.domains.cad.domain import UNIT_TO_METERS
from openvietgis_api.domains.cad.georeference import CadPlacement, place_features, restore_features
from openvietgis_api.domains.editing.domain import (
    EditOperationInput,
    EditOpType,
    validate_feature_id,
    validate_geometry_payload,
)
from openvietgis_api.domains.platform_errors import PlatformConflictError, PlatformValidationError
from openvietgis_api.domains.spatial.domain import GeometryFamily

LIMIT = 25 * 1024 * 1024
HANDLE = re.compile(r"^[0-9A-F]{1,32}$")


def _fail(message: str) -> NoReturn:
    raise PlatformValidationError(message, code="CAD_DELTA_UNSUPPORTED")


def _check(payload: bytes, expected: str) -> None:
    if not 0 < len(payload) <= LIMIT:
        _fail("Native DXF byte limit")
    if hashlib.sha256(payload).hexdigest() != expected:
        raise PlatformConflictError("CAD blob digest changed", code="STALE_CAD_REVISION")


def _index(
    payload: bytes, placement: CadPlacement
) -> tuple[dict[str, dict[str, Any]], list[dict[str, Any]]]:
    _, features, summary, loss = parse_dxf_2d(payload, units=placement.units)
    if loss["requires_acknowledgement"]:
        _fail("Native delta requires a homogeneous, exact straight XY drawing")
    rows = summary["entity_mapping"]
    by_id = {f["feature_id"]: f for f in features}
    result = {}
    for row in rows:
        handle = row["handle"].upper()
        if not HANDLE.fullmatch(handle) or handle in result or row["disposition"] != "mapped":
            _fail("Ambiguous or unsupported native handle")
        result[handle] = {"row": row, "feature": by_id[row["feature_id"]]}
    return result, rows


def _linked(base: dict[str, dict[str, Any]], mapping: list[dict[str, Any]]) -> dict[str, str]:
    """Validate immutable conversion provenance, not caller-supplied feature attributes."""
    result = {}
    for row in mapping:
        handle = str(row.get("handle", "")).upper()
        if handle in result or handle not in base or row.get("disposition") != "mapped":
            _fail("Native link is missing, duplicate, or not exact")
        raw_fid = row.get("feature_id")
        if not isinstance(raw_fid, str):
            _fail("Native link has invalid feature identity")
        fid = validate_feature_id(raw_fid)
        original = base[handle]["row"]
        if any(row.get(k) != original.get(k) for k in ("entity", "layer", "family")):
            _fail("Native link mapping disagrees with base blob")
        result[handle] = fid
    if set(result) != set(base) or len(set(result.values())) != len(result):
        _fail("Native delta requires complete one-to-one mapping")
    return result


def _shape_equal(a: dict[str, Any], b: dict[str, Any]) -> bool:
    # Exact coordinate ordering is intentional; uncertain ring/line reordering
    # produces a proposed edit rather than a silently discarded change.
    return a == b


def cad_to_gis_preview(
    base_payload: bytes,
    candidate_payload: bytes,
    *,
    base_sha256: str,
    candidate_sha256: str,
    placement: CadPlacement,
    mapping: list[dict[str, Any]],
    imported_content_revision: str,
    current_content_revision: str,
    imported_schema_revision: str,
    current_schema_revision: str,
) -> list[EditOperationInput]:
    """Prepare geometry-only R006 updates; require unchanged imported GIS base.

    The caller must retrieve placement/mapping/revisions under current scope.
    A real commit still requires server authorization, PostGIS topology checks,
    reviewed operation digest and the existing R006 transactional commit.
    """
    _check(base_payload, base_sha256)
    _check(candidate_payload, candidate_sha256)
    if (imported_content_revision, imported_schema_revision) != (
        current_content_revision,
        current_schema_revision,
    ):
        raise PlatformConflictError(
            "GIS base diverged; review conflict", code="STALE_SOURCE_REVISION"
        )
    base, _ = _index(base_payload, placement)
    candidate, _ = _index(candidate_payload, placement)
    linked = _linked(base, mapping)
    if set(base) != set(candidate):
        _fail("Create/delete/handle regeneration requires a separate impact review")
    changes = []
    for handle in sorted(base):
        old, new = base[handle], candidate[handle]
        if any(old["row"].get(k) != new["row"].get(k) for k in ("entity", "layer", "family")):
            _fail("Entity type or layer change requires mapping review")
        before, after = old["feature"]["geometry"], new["feature"]["geometry"]
        if _shape_equal(before, after):
            continue
        feature = deepcopy(new["feature"])
        if placement.native_reference is not None:
            place_features([deepcopy(feature)], placement.native_reference)
        place_features([feature], placement)
        validate_geometry_payload(feature["geometry"], GeometryFamily(after["type"]))
        changes.append(EditOperationInput(EditOpType.UPDATE, linked[handle], feature["geometry"]))
    if len(changes) > 1000:
        _fail("Native delta exceeds 1000 updates")
    return changes


def gis_to_cad_preview(
    base_payload: bytes,
    *,
    base_sha256: str,
    placement: CadPlacement,
    mapping: list[dict[str, Any]],
    features: list[dict[str, Any]],
) -> tuple[bytes, tuple[str, ...]]:
    """Create an isolated native DXF proposal, preserving qualified handles.

    Caller pins latest document/source revisions and validates topology before
    publication. This profile rejects additions/deletions, mixed/unsupported
    entities and semantic CAD geometry conversions. No GIS attributes are copied.
    """
    _check(base_payload, base_sha256)
    base, _ = _index(base_payload, placement)
    links = _linked(base, mapping)
    if len(features) != len(links) or {f.get("feature_id") for f in features} != set(
        links.values()
    ):
        _fail("GIS feature additions/deletions require separate review")
    projected = {f["feature_id"]: f for f in features}
    local = restore_features(
        features,
        placement,
        document_id=placement.document_id,
        version=placement.version,
        source_crs_code=placement.crs_code,
    )
    local_by_id = {f["feature_id"]: f for f in local}
    doc = read(io.StringIO(base_payload.decode("utf-8")))
    original_props = {}
    changed = []
    scale = UNIT_TO_METERS[placement.units]
    for handle, fid in sorted(links.items()):
        entry = base[handle]
        ent = doc.entitydb[handle]
        old_geom = entry["feature"]["geometry"]
        old_projected = deepcopy(entry["feature"])
        place_features([old_projected], placement)
        geom = local_by_id[fid]["geometry"]
        if geom["type"] != old_geom["type"]:
            _fail("Geometry family change requires mapping review")
        if geom["type"] == "Polygon" and len(geom["coordinates"]) != 1:
            _fail("Polygon holes cannot be represented by native simple polyline")
        etype = entry["row"]["entity"]
        if etype not in ("POINT", "LINE", "LWPOLYLINE"):
            _fail("Unsupported native entity")
        original_props[handle] = {
            k: v
            for k, v in ent.dxf.all_existing_dxf_attribs().items()
            if k not in {"location", "start", "end", "count"}
        }
        if _shape_equal(old_projected["geometry"], projected[fid]["geometry"]):
            continue
        coords = geom["coordinates"]

        def xy(p: list[float]) -> tuple[float, float]:
            return (p[0] / scale, p[1] / scale)

        if etype == "POINT":
            ent.dxf.location = (*xy(coords), 0)
        elif etype == "LINE":
            if len(coords) != 2 or coords[0] == coords[1]:
                _fail("Native LINE requires two distinct endpoints")
            ent.dxf.start, ent.dxf.end = (*xy(coords[0]), 0), (*xy(coords[1]), 0)
        else:
            if not isinstance(ent, LWPolyline):
                _fail("Expected native LWPOLYLINE")
            if geom["type"] == "Polygon":
                vertices = coords[0][:-1]
                if len(vertices) < 3:
                    _fail("Degenerate native polygon")
                # Preserve the original closure representation.
                if not ent.closed or ent.get_points("xy")[0] == ent.get_points("xy")[-1]:
                    vertices = coords[0]
            else:
                vertices = coords
            # set_points("xy") drops per-vertex bulges/widths, which the dxf-attrib
            # roundtrip check below cannot see. Import already rejects wide polylines.
            if any(s or e or b for _, _, s, e, b in ent.get_points("xyseb")):
                _fail("Native LWPOLYLINE arcs (bulge) require CAD review")
            ent.set_points([xy(p) for p in vertices], format="xy")
        changed.append(handle)
    if len(changed) > 1000:
        _fail("Native delta exceeds 1000 updates")
    if not changed:
        return base_payload, ()
    out = io.StringIO()
    doc.write(out)
    payload = out.getvalue().encode("utf-8")
    if len(payload) > LIMIT:
        _fail("Native proposal exceeds byte limit")
    reopened = read(io.StringIO(payload.decode("utf-8")))
    indexed, _ = _index(payload, placement)
    if set(indexed) != set(base):
        _fail("Native roundtrip changed handle set")
    for handle, fid in links.items():
        ent = reopened.entitydb[handle]
        props = {
            k: v
            for k, v in ent.dxf.all_existing_dxf_attribs().items()
            if k not in {"location", "start", "end", "count"}
        }
        if props != original_props[handle]:
            _fail("Native roundtrip changed qualified entity properties")
        before = base[handle]["feature"]["geometry"]
        after = indexed[handle]["feature"]["geometry"]
        if handle not in changed and not _shape_equal(before, after):
            _fail("Native roundtrip changed unrelated geometry")
        # Validate the resulting feature against the proposed local geometry.
        # Floating division by unit scale may differ at sub-nanometre levels;
        # compare the actual grid roundtrip through the existing inverse helper.
        if handle in changed:
            expected = deepcopy(local_by_id[fid])
            got = deepcopy(indexed[handle]["feature"])
            place_features([expected], placement)
            place_features([got], placement)

            def close(a: Any, b: Any) -> bool:
                if isinstance(a, list) and isinstance(b, list):
                    return len(a) == len(b) and all(close(x, y) for x, y in zip(a, b, strict=True))
                return (
                    isinstance(a, (float, int))
                    and isinstance(b, (float, int))
                    and abs(a - b) <= min(placement.tolerance_m, 1e-7)
                )

            if got["geometry"]["type"] != expected["geometry"]["type"] or not close(
                got["geometry"]["coordinates"], expected["geometry"]["coordinates"]
            ):
                _fail("Native roundtrip differs from proposed geometry")
    return payload, tuple(changed)
