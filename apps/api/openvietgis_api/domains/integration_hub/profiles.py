"""Pure domain read-only exchange profiles for integration hub adapters.

Profiles describe verified read-only exchange capabilities, explicit CRS rules,
and existing project-scoped API endpoints without exposing database credentials,
initiating outbound network connections, or enabling writeback.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from typing import Final

from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.integration_hub.catalog import (
    AdapterLifecycle,
    AdapterManifest,
    serialize_adapter,
)
from openvietgis_api.domains.spatial.crs import get_readable_crs_profile

PROFILE_ENABLED_ADAPTER_KEYS: Final[frozenset[str]] = frozenset(
    {"qgis", "postgis", "ogc_services"}
)


@dataclass(frozen=True, slots=True)
class LayerAccessSnapshot:
    layer_id: str
    source_id: str
    name: str
    geometry_family: str
    dimensions: str
    validation_state: str
    source_crs_code: int | None
    stored_crs_code: int | None
    layer_revision: int
    content_revision: str
    schema_revision: str
    export_permitted: bool


def execution_status_for(adapter: AdapterManifest) -> str:
    if adapter.key == "ogc_services":
        return "existing_read_only_api"
    if adapter.key in ("qgis", "postgis"):
        return "read_only_profile_ready"
    if adapter.key == "microstation_v8i" or adapter.lifecycle is AdapterLifecycle.CANDIDATE:
        return "candidate_external_branch"
    return "planned_not_implemented"


def _build_connection_profile(
    adapter_key: str,
    *,
    organization_id: str,
    project_id: str,
) -> dict[str, object] | None:
    base = f"/api/v1/organizations/{organization_id}/projects/{project_id}"
    if adapter_key == "qgis":
        return {
            "adapter_key": "qgis",
            "access_mode": "read_only_exchange",
            "supports_direct_db_credentials": False,
            "requires_per_layer_acl": True,
            "default_write_policy": "deny",
            "supported_protocols": [
                "GeoPackage",
                "GeoJSON",
                "WMS",
                "WFS",
                "PostGIS_read_only_export",
            ],
            "export_formats": ["geopackage", "geojson", "postgis_sql"],
            "existing_endpoints": [
                {
                    "name": "project_layers",
                    "method": "GET",
                    "path_template": f"{base}/layers",
                    "required_permission": "project.read",
                },
                {
                    "name": "scoped_geojson_export",
                    "method": "GET",
                    "path_template": f"{base}/access/export/{{layer_id}}",
                    "required_permission": "project.read",
                },
                {
                    "name": "hub_layer_export",
                    "method": "GET",
                    "path_template": (
                        f"{base}/integration-hub/adapters/{adapter_key}/layers/{{layer_id}}/export"
                    ),
                    "required_permission": "project.read",
                },
            ],
            "crs_policy": "explicit_only_never_inferred",
            "restrictions": [
                "No direct internal PostgreSQL/PostGIS credentials are exposed.",
                "Only active layers visible under RLS and per-layer/feature GIS ACL are listed.",
                "CRS is reported explicitly from stored_crs; missing CRS is never inferred.",
                "All QGIS writeback and sync.push operations remain denied by default.",
            ],
        }
    if adapter_key == "postgis":
        return {
            "adapter_key": "postgis",
            "access_mode": "read_only_exchange",
            "supports_direct_db_credentials": False,
            "requires_per_layer_acl": True,
            "default_write_policy": "deny",
            "supported_protocols": ["PostGIS_read_only_export", "GeoPackage"],
            "export_formats": ["postgis_sql", "geopackage"],
            "existing_endpoints": [
                {
                    "name": "project_layers",
                    "method": "GET",
                    "path_template": f"{base}/layers",
                    "required_permission": "project.read",
                },
                {
                    "name": "scoped_geojson_export",
                    "method": "GET",
                    "path_template": f"{base}/access/export/{{layer_id}}",
                    "required_permission": "project.read",
                },
                {
                    "name": "hub_layer_export",
                    "method": "GET",
                    "path_template": (
                        f"{base}/integration-hub/adapters/{adapter_key}/layers/{{layer_id}}/export"
                    ),
                    "required_permission": "project.read",
                },
            ],
            "crs_policy": "explicit_only_never_inferred",
            "restrictions": [
                "Internal OpenVietGIS PostGIS is never exposed directly as an external DB bridge.",
                "No direct internal PostgreSQL/PostGIS credentials are exposed.",
                "External DB connectors require host allowlist, credential vault, and review.",
                "Only active layers visible under RLS and per-layer/feature GIS ACL are listed.",
                "CRS is reported explicitly from stored_crs; missing CRS is never inferred.",
                "All PostGIS writeback and sync.push operations remain denied by default.",
            ],
        }
    if adapter_key == "ogc_services":
        return {
            "adapter_key": "ogc_services",
            "access_mode": "read_only_exchange",
            "supports_direct_db_credentials": False,
            "requires_per_layer_acl": True,
            "default_write_policy": "deny",
            "supported_protocols": ["WMS", "WFS", "COG", "XYZ"],
            "existing_endpoints": [
                {
                    "name": "external_sources",
                    "method": "GET",
                    "path_template": f"{base}/external-sources",
                    "required_permission": "project.read",
                },
            ],
            "crs_policy": "explicit_only_never_inferred",
            "restrictions": [
                "R010 SSRF allowlist and read-only enforcement remain unchanged.",
                "No direct internal PostgreSQL/PostGIS credentials are exposed.",
                "CRS is reported explicitly from stored_crs; missing CRS is never inferred.",
                "No hub mutation or remote writeback is permitted.",
            ],
        }
    return None


def is_explicit_readable_crs(stored_crs_code: int | None, validation_state: str) -> bool:
    return bool(
        stored_crs_code is not None
        and validation_state == "ready"
        and get_readable_crs_profile(stored_crs_code) is not None
    )


def _serialize_layer(layer: LayerAccessSnapshot) -> dict[str, object]:
    crs_explicit = is_explicit_readable_crs(layer.stored_crs_code, layer.validation_state)
    return {
        "layer_id": layer.layer_id,
        "source_id": layer.source_id,
        "name": layer.name,
        "geometry_family": layer.geometry_family,
        "dimensions": layer.dimensions,
        "validation_state": layer.validation_state,
        "source_crs_code": layer.source_crs_code,
        "stored_crs_code": layer.stored_crs_code,
        "crs_explicit": crs_explicit,
        "layer_revision": layer.layer_revision,
        "content_revision": layer.content_revision,
        "schema_revision": layer.schema_revision,
        "export_permitted": bool(layer.export_permitted and crs_explicit),
    }


def build_adapter_profile(
    adapter: AdapterManifest,
    effective_permissions: frozenset[Permission],
    *,
    organization_id: str,
    project_id: str,
    scoped_access: bool,
    layers: Sequence[LayerAccessSnapshot],
) -> dict[str, object]:
    connection_profile = _build_connection_profile(
        adapter.key,
        organization_id=organization_id,
        project_id=project_id,
    )
    serialized_layers = (
        [_serialize_layer(layer) for layer in layers]
        if connection_profile is not None
        else []
    )
    return {
        "schema_version": 1,
        "organization_id": organization_id,
        "project_id": project_id,
        "scoped_access": scoped_access,
        "default_write_policy": "deny",
        "adapter": serialize_adapter(adapter, effective_permissions),
        "execution_status": execution_status_for(adapter),
        "connection_profile": connection_profile,
        "layers": serialized_layers,
    }
