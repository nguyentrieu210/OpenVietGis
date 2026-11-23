"""Versioned adapter inventory and fail-closed capability authorization.

This catalog intentionally does not execute, pair, or claim to synchronize any
desktop software. Execution is an independent, reviewed API contract.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
from typing import Final

from openvietgis_api.domains.authorization.permissions import Permission


class ConnectorAction(StrEnum):
    DISCOVER = "catalog.discover"
    INSPECT = "connection.inspect"
    CONFIGURE = "connection.configure"
    IMPORT = "data.import"
    EXPORT = "data.export"
    SYNC_PULL = "sync.pull"
    SYNC_PUSH = "sync.push"
    APPROVE = "sync.approve"


class AdapterLifecycle(StrEnum):
    EXISTING_READ_ONLY = "existing_read_only"
    CANDIDATE = "candidate"
    PLANNED = "planned"


# A project right is only a *precondition*: layer, field, feature, connection
# credential and revision ACLs must also pass at the actual execution endpoint.
ACTION_PERMISSIONS: Final[dict[ConnectorAction, Permission]] = {
    ConnectorAction.DISCOVER: Permission.PROJECT_READ,
    ConnectorAction.INSPECT: Permission.PROJECT_READ,
    ConnectorAction.CONFIGURE: Permission.PROJECT_MANAGE,
    ConnectorAction.IMPORT: Permission.PROJECT_EDIT,
    ConnectorAction.EXPORT: Permission.PROJECT_READ,
    ConnectorAction.SYNC_PULL: Permission.PROJECT_READ,
    ConnectorAction.SYNC_PUSH: Permission.PROJECT_EDIT,
    ConnectorAction.APPROVE: Permission.PROJECT_MANAGE,
}


@dataclass(frozen=True, slots=True)
class AdapterManifest:
    key: str
    name: str
    category: str
    mode: str
    lifecycle: AdapterLifecycle
    protocols: tuple[str, ...]
    note: str
    # Advertised *potential* operations; this does NOT mean API execution exists.
    proposed_actions: tuple[ConnectorAction, ...]
    existing_api: str | None = None
    # Operations implemented by this new hub; must be explicit, never inferred.
    hub_executable_actions: tuple[ConnectorAction, ...] = (ConnectorAction.DISCOVER,)


ADAPTERS: Final[tuple[AdapterManifest, ...]] = (
    AdapterManifest("ogc_services", "OGC WMS/WFS", "gis", "remote_service",
                    AdapterLifecycle.EXISTING_READ_ONLY, ("WMS", "WFS", "COG", "XYZ"),
                    "R010 provides allowlisted read-only external sources; no hub sync.",
                    (ConnectorAction.INSPECT,), "external-sources"),
    AdapterManifest("qgis", "QGIS", "gis", "open_protocols",
                    AdapterLifecycle.PLANNED, ("GeoPackage", "PostGIS", "WMS", "WFS"),
                    "QGIS plugin and push/pull synchronization not integrated.",
                    (ConnectorAction.INSPECT, ConnectorAction.EXPORT, ConnectorAction.SYNC_PULL)),
    AdapterManifest("arcgis", "ArcGIS Pro / Online", "gis", "remote_service",
                    AdapterLifecycle.PLANNED, ("ArcGIS Feature Service", "FileGDB", "SHP"),
                    "Vendor API authentication, feature ACL and versioning require tests.",
                    (ConnectorAction.INSPECT, ConnectorAction.SYNC_PULL)),
    AdapterManifest("autocad", "AutoCAD / Map 3D", "cad", "file_or_desktop_bridge",
                    AdapterLifecycle.PLANNED, ("DXF", "DWG"),
                    "Native DWG fidelity and licensing require verified adapter.",
                    (ConnectorAction.IMPORT, ConnectorAction.EXPORT)),
    AdapterManifest("microstation_v8i", "MicroStation V8i", "cad", "windows_bridge",
                    AdapterLifecycle.CANDIDATE, ("DGN V8", "COM"),
                    "R025 separate branch: licensed Windows instance, read-only preview first.",
                    (ConnectorAction.INSPECT, ConnectorAction.IMPORT)),
    AdapterManifest("mapinfo", "MapInfo Pro", "gis", "file_exchange",
                    AdapterLifecycle.PLANNED, ("TAB", "MIF", "MID"),
                    "Text, CRS and styling preservation not yet accepted.",
                    (ConnectorAction.IMPORT, ConnectorAction.EXPORT)),
    AdapterManifest("cadastral_vn", "gCadas / FAMIS / TMV.Map", "cadastral", "format_adapter",
                    AdapterLifecycle.PLANNED, ("DGN", "cadastral_attributes"),
                    "Requires lawful schema access and tested parcel/attribute binding.",
                    (ConnectorAction.IMPORT, ConnectorAction.EXPORT)),
    AdapterManifest("postgis", "PostgreSQL / PostGIS", "database", "database_bridge",
                    AdapterLifecycle.PLANNED, ("PostGIS",),
                    "Existing OpenVietGIS internal PostGIS is not an external DB connector.",
                    (ConnectorAction.INSPECT, ConnectorAction.SYNC_PULL)),
    AdapterManifest("ogc_publisher", "OGC publishing servers", "gis", "remote_service",
                    AdapterLifecycle.PLANNED, ("WMS", "WFS", "OGC API"),
                    "Generic WMS/WFS may be registered in R010; no dedicated adapter.",
                    (ConnectorAction.INSPECT, ConnectorAction.SYNC_PULL)),
    AdapterManifest("google_earth", "Google Earth Pro", "gis", "file_exchange",
                    AdapterLifecycle.PLANNED, ("KML", "KMZ"),
                    "Dedicated exchange adapter not verified.",
                    (ConnectorAction.IMPORT, ConnectorAction.EXPORT)),
    AdapterManifest("excel", "Excel / CSV", "tabular", "file_exchange",
                    AdapterLifecycle.PLANNED, ("XLSX", "CSV"),
                    "Dedicated coordinate/field mapping profile not verified.",
                    (ConnectorAction.IMPORT, ConnectorAction.EXPORT)),
    AdapterManifest("survey_3d", "RTK / UAV / BIM", "survey_3d", "file_exchange",
                    AdapterLifecycle.PLANNED, ("LandXML", "GeoTIFF", "LAS", "3D Tiles"),
                    "Acquisition device and model adapters require separate acceptance.",
                    (ConnectorAction.IMPORT, ConnectorAction.EXPORT)),
)


def evaluate_action(
    adapter: AdapterManifest,
    action: ConnectorAction,
    effective_permissions: frozenset[Permission],
) -> dict[str, str | bool]:
    """Policy preflight is *not* an authorization token or execution grant."""
    permission = ACTION_PERMISSIONS[action]
    eligible = permission in effective_permissions
    declared = action is ConnectorAction.DISCOVER or action in adapter.proposed_actions
    executable = action in adapter.hub_executable_actions
    allowed = eligible and declared and executable
    if not eligible:
        reason = "permission_denied"
    elif not declared:
        reason = "unsupported_by_adapter"
    elif not executable:
        reason = "not_implemented"
    else:
        reason = "catalog_only"
    return {
        "action": action.value,
        "required_permission": permission.value,
        "eligible_by_role": eligible,
        "executable_in_hub": executable,
        "allowed": allowed,
        "reason": reason,
    }


def list_adapters(effective_permissions: frozenset[Permission]) -> list[dict[str, object]]:
    return [
        {
            "key": adapter.key,
            "name": adapter.name,
            "category": adapter.category,
            "mode": adapter.mode,
            "lifecycle": adapter.lifecycle.value,
            "protocols": list(adapter.protocols),
            "note": adapter.note,
            "existing_api": adapter.existing_api,
            "actions": [
                evaluate_action(adapter, action, effective_permissions)
                for action in ConnectorAction
            ],
        }
        for adapter in ADAPTERS
    ]
