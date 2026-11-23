"""OpenVietGIS Read-Only Connector plugin package for QGIS 3.28+."""

from __future__ import annotations

from typing import Any


def classFactory(iface: Any) -> Any:
    """QGIS plugin entry point returning the read-only connector plugin instance."""
    from .plugin import OpenVietGisConnectorPlugin

    return OpenVietGisConnectorPlugin(iface)
