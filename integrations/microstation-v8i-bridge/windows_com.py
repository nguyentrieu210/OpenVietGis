"""Optional Windows-only COM attach. Does not scan, edit or write a DGN."""

from __future__ import annotations

import os
from pathlib import Path


class V8iProbeError(RuntimeError):
    pass


def probe_running_v8i() -> dict[str, str | bool]:
    """Attach to a running MicroStation COM session; do not instantiate the application."""
    if os.name != "nt":
        raise V8iProbeError("COM probe requires Windows with a licensed, running MicroStation V8i")
    try:
        import win32com.client  # type: ignore[import-not-found]
    except ImportError as exc:
        raise V8iProbeError("pywin32 unavailable; run 'py -m pip install pywin32'") from exc
    try:
        application = win32com.client.GetActiveObject("MicroStationDGN.Application")
    except Exception as exc:
        raise V8iProbeError("Cannot attach to running MicroStationDGN.Application; check V8i and COM registration") from exc
    try:
        drawing = application.ActiveDesignFile
        model = application.ActiveModelReference
        filename = Path(str(drawing.FullName)).name  # Never publish local paths.
        model_name = str(model.Name)
    except Exception as exc:
        raise V8iProbeError("COM attached, but active drawing/model metadata is unavailable") from exc
    return {
        "bridge": "OpenVietGIS MicroStation V8i",
        "connection": "attached",
        "drawing_name": filename,
        "model_name": model_name,
        "read_only": True,
        "geometry_extraction_verified": False,
        "write_back_enabled": False,
    }
