"""Read-only Windows V8i compatibility diagnostics; no snapshots/geometry persisted.

Safe to run with real cadastral DGN: attaches to an already running V8i
session, enumerates at most 5000 elements, and reports counts only. No edits,
network calls, credentials, or path collection in the saved output.
"""
from __future__ import annotations

import os
import platform
from collections import Counter
from typing import Any


class DiagnosticError(RuntimeError):
    """Native V8i compatibility has not been established."""


def inspect_v8i(get_active: Any = None) -> dict[str, Any]:
    if get_active is None:
        if os.name != "nt":
            raise DiagnosticError("V8i COM can only be inspected on Windows.")
        try:
            from win32com.client import GetActiveObject  # type: ignore[import-not-found]
        except ImportError as exc:
            raise DiagnosticError("Install pywin32 or use the packaged Windows executable.") from exc
        get_active = GetActiveObject
    try:
        app = get_active("MicroStationDGN.Application")
        model = app.ActiveModelReference
        application_version = str(getattr(app, "Version", "unavailable"))[:80]
        model_name = str(model.Name)[:80]
        e = model.Scan()
        counts: Counter[str] = Counter()
        count = 0
        while e.MoveNext():
            count += 1
            if count > 5000:
                raise DiagnosticError("Scan exceeded 5000 elements; cannot certify completeness.")
            counts[str(int(e.Current.Type))] += 1
    except DiagnosticError:
        raise
    except Exception as exc:
        raise DiagnosticError("Cannot inspect running MicroStation V8i COM model.") from exc
    supported = sum(counts.get(str(kind), 0) for kind in (3, 4, 6))
    return {
        "connector": "openvietgis-windows-v8i",
        "status": "read_only_probe_only",
        "application_version": application_version,
        "model_name": model_name,
        "os": platform.system(),
        "python_bitness": platform.architecture()[0],
        "scanned": count,
        "candidate_simple_entities": supported,
        "unsupported_or_unhandled": count - supported,
        "entity_type_counts": dict(sorted(counts.items())),
        "geometry_and_units_verified": False,
        "cad_write_enabled": False,
        "network_writes": False,
    }