"""Fail-closed Windows COM probe. Attaches only to an already running V8i session."""
from __future__ import annotations

import os
from pathlib import PureWindowsPath
from typing import Any


class ProbeError(RuntimeError):
    pass


def probe(get_active: Any = None) -> dict[str, str]:
    if get_active is None:
        if os.name != "nt":
            raise ProbeError("MicroStation V8i COM requires Windows.")
        try:
            from win32com.client import GetActiveObject  # type: ignore[import-not-found]
        except ImportError as exc:
            raise ProbeError("Install pywin32 for V8i COM detection.") from exc
        get_active = GetActiveObject
    try:
        application = get_active("MicroStationDGN.Application")
        drawing = PureWindowsPath(str(application.ActiveDesignFile.FullName)).name
        model = str(application.ActiveModelReference.Name)
        if not drawing or not model:
            raise ProbeError("No active V8i DGN/model.")
    except ProbeError:
        raise
    except Exception as exc:
        raise ProbeError("Cannot attach to an already-running V8i session.") from exc
    return {
        "status": "attached_read_only",
        "drawing": drawing,
        "model": model,
        "cad_write_back_enabled": "false",
    }