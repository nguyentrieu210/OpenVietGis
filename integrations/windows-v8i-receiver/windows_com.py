"""Read-only MicroStation V8i COM adapter.

Only attaches to an already-running application. Does not create/save/edit CAD
entities and does not make network calls. The scan is a pilot, not certified
for cadastral use until unit scale and V8i behavior are tested on Windows.
"""

from __future__ import annotations

import os
from collections import Counter
from pathlib import PureWindowsPath
from typing import Any

from bridge_contract import ContractError, validate_snapshot


class V8iProbeError(RuntimeError):
    """The running CAD application could not be safely inspected."""


# MicroStation V8 VBA msdElementType constants, *not* GIS parcel types.
_LINE = 3
_LINE_STRING = 4
_SHAPE = 6
MAX_SCAN = 5000


def _running_app() -> Any:
    if os.name != "nt":
        raise V8iProbeError("V8i COM requires Windows and a licensed running instance")
    try:
        import win32com.client  # type: ignore[import-not-found]
    except ImportError as exc:
        raise V8iProbeError("pywin32 not installed; use py -m pip install pywin32") from exc
    try:
        # GetActiveObject is attach-only. Dispatch may launch V8i: never use it.
        return win32com.client.GetActiveObject("MicroStationDGN.Application")
    except Exception as exc:
        raise V8iProbeError("No running MicroStationDGN.Application COM object") from exc


def probe_running_v8i() -> dict[str, str | bool]:
    app = _running_app()
    try:
        drawing = PureWindowsPath(str(app.ActiveDesignFile.FullName)).name
        model = str(app.ActiveModelReference.Name)
    except Exception as exc:
        raise V8iProbeError("Running V8i has no readable active drawing/model") from exc
    return {
        "bridge": "OpenVietGIS MicroStation V8i",
        "connection": "attached",
        "drawing_name": drawing,
        "model_name": model,
        "read_only": True,
        "geometry_extraction_verified": False,
        "coordinate_units_verified": False,
        "write_back_enabled": False,
    }


def _element_id(raw: Any) -> str:
    # In V8i VBA, Element.ID is commonly a DLong with High/Low DWORDs.
    identity = raw.ID
    if hasattr(identity, "High") and hasattr(identity, "Low"):
        high, low = int(identity.High) & 0xFFFFFFFF, int(identity.Low) & 0xFFFFFFFF
        return f"{high:08x}:{low:08x}"
    if isinstance(identity, int) and identity >= 0:
        return str(identity)
    raise V8iProbeError("CAD element does not expose a stable numeric/DLong ID")


def _coordinate(point: Any) -> list[float]:
    try:
        return [float(point.X), float(point.Y)]
    except (TypeError, ValueError, AttributeError) as exc:
        raise V8iProbeError("CAD Point3d coordinate cannot be interpreted") from exc


def _geometry(raw: Any, kind: int) -> dict[str, Any]:
    if kind == _LINE:
        line = raw.AsLineElement
        coordinates = [_coordinate(line.StartPoint), _coordinate(line.EndPoint)]
        return {"type": "LineString", "coordinates": coordinates}
    if kind in (_LINE_STRING, _SHAPE):
        vertices = [_coordinate(p) for p in raw.AsVertexList.GetVertices()]
        if kind == _SHAPE:
            if vertices and vertices[0] != vertices[-1]:
                vertices.append(vertices[0])
            return {"type": "Polygon", "coordinates": [vertices]}
        return {"type": "LineString", "coordinates": vertices}
    raise V8iProbeError("unsupported CAD geometry")


def scan_model_preview(
    model: Any, *, drawing_id: str, model_id: str, source_crs: str, max_elements: int = MAX_SCAN
) -> dict[str, Any]:
    """Scan only the active model; no reference attachment traversal.

    All CAD geometry is still in *V8i COM coordinate units*. The caller must
    verify master units and model origin against source control points before
    interpreting coordinates geospatially. Unsupported types remain visible.
    """
    if not 1 <= max_elements <= MAX_SCAN:
        raise V8iProbeError("max_elements outside 1..5000")
    try:
        enumerator = model.Scan()
    except Exception as exc:
        raise V8iProbeError("Active model does not support read-only Scan") from exc
    elements: list[dict[str, Any]] = []
    kinds: Counter[str] = Counter()
    seen = set()
    scanned = 0
    unsupported = 0
    try:
        while enumerator.MoveNext():
            scanned += 1
            if scanned > max_elements:
                raise V8iProbeError("Scan limit reached; no partial snapshot returned")
            raw = enumerator.Current
            cad_type = int(raw.Type)
            kinds[str(cad_type)] += 1
            if cad_type not in (_LINE, _LINE_STRING, _SHAPE):
                unsupported += 1
                continue
            try:
                eid = _element_id(raw)
                if eid in seen:
                    raise V8iProbeError("Duplicate CAD element ID; scan aborted")
                seen.add(eid)
                geom = _geometry(raw, cad_type)
                elements.append({
                    "element_id": eid,
                    "geometry": geom,
                    "properties": {"cad_type": cad_type},
                })
            except (AttributeError, TypeError, ValueError, ContractError) as exc:
                # Never convert a known but malformed geometry to an apparently
                # complete snapshot. A type may vary between V8i COM builds.
                raise V8iProbeError(
                    "Known CAD element could not be safely extracted; no snapshot returned"
                ) from exc
    except V8iProbeError:
        raise
    except Exception as exc:
        raise V8iProbeError("COM model enumeration failed; no partial snapshot returned") from exc

    try:
        snapshot = validate_snapshot({
            "schema_version": "ovg.v8i.snapshot.v1",
            "drawing_id": drawing_id,
            "model_id": model_id,
            "source_crs": source_crs,
            "unsupported_count": unsupported,
            "elements": elements,
        })
    except ContractError as exc:
        raise V8iProbeError("Snapshot failed contract validation; no output returned") from exc
    return {
        "snapshot": snapshot,
        "scan_stats": {
            "total_elements": scanned,
            "converted_elements": len(elements),
            "unsupported_elements": unsupported,
            "cad_type_counts": dict(sorted(kinds.items())),
            "reference_attachments_scanned": False,
            "coordinate_units_verified": False,
            "cadastral_attributes_verified": False,
            "geometry_extraction_verified_on_v8i": False,
        },
    }


def scan_running_v8i(
    *, drawing_id: str, model_id: str, source_crs: str, max_elements: int = MAX_SCAN
) -> dict[str, Any]:
    app = _running_app()
    try:
        model = app.ActiveModelReference
    except Exception as exc:
        raise V8iProbeError("Running MicroStation has no active model") from exc
    return scan_model_preview(
        model, drawing_id=drawing_id, model_id=model_id,
        source_crs=source_crs, max_elements=max_elements,
    )