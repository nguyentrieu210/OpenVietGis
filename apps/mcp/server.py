"""Local MCP server: read-only, fixed organization and project."""
from typing import Any
from mcp.server import MCPServer
from gis_engineering_policy import GIS_INSTRUCTIONS
from mcp.types import ToolAnnotations
from gis_client import GisClient, Settings

mcp = MCPServer("OpenVietGIS", instructions=GIS_INSTRUCTIONS)


def gis():
    return GisClient(Settings.from_env())


@mcp.tool(annotations=ToolAnnotations(read_only_hint=True, destructive_hint=False, open_world_hint=True), structured_output=True)
def gis_get_project() -> dict[str, Any]:
    """Read metadata for the configured OpenVietGIS project."""
    return gis().project()


@mcp.tool(annotations=ToolAnnotations(read_only_hint=True, destructive_hint=False, open_world_hint=True), structured_output=True)
def gis_get_saved_map() -> dict[str, Any]:
    """Read the project's saved map configuration; not the live browser viewport."""
    return gis().map_state()


@mcp.tool(annotations=ToolAnnotations(read_only_hint=True, destructive_hint=False, open_world_hint=True), structured_output=True)
def gis_list_layers(limit: int = 50, after_id: str | None = None) -> dict[str, Any]:
    """List readable layers in the configured project, with pagination."""
    return gis().layers(limit, after_id)


@mcp.tool(annotations=ToolAnnotations(read_only_hint=True, destructive_hint=False, open_world_hint=True), structured_output=True)
def gis_get_layer(layer_id: str) -> dict[str, Any]:
    """Get layer metadata, source revision and capabilities for a readable layer."""
    return gis().layer(layer_id)


@mcp.tool(annotations=ToolAnnotations(read_only_hint=True, destructive_hint=False, open_world_hint=True), structured_output=True)
def gis_list_features(
    layer_id: str, content_revision: str, schema_revision: str,
    limit: int = 20, after_feature_id: str | None = None, include_geometry: bool = False,
) -> dict[str, Any]:
    """Read a pinned page of features. Get current revision pins with gis_get_layer first.
    Geometry is opt-in and requested in WGS84 (EPSG:4326)."""
    return gis().features(layer_id, content_revision, schema_revision, limit,
                          after_feature_id, include_geometry)


if __name__ == "__main__":
    mcp.run(transport="stdio")
