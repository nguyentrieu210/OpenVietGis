"""MCP server for a paired live GIS workspace; no GIS account secret."""
from typing import Any
from mcp.server import MCPServer
from mcp.types import ToolAnnotations
from bridge_client import BridgeClient, BridgeSettings

mcp = MCPServer("OpenVietGIS Live Workspace")


def bridge():
    return BridgeClient(BridgeSettings.from_env())


@mcp.tool(annotations=ToolAnnotations(read_only_hint=True, destructive_hint=False), structured_output=True)
def gis_live_view() -> dict[str, Any]:
    """Read the current map viewport after explicit approval in GIS."""
    return bridge().run("map_view", {})


@mcp.tool(annotations=ToolAnnotations(read_only_hint=True, destructive_hint=False), structured_output=True)
def gis_live_selection() -> dict[str, Any]:
    """Read only the currently selected geometry, WGS84, after explicit GIS approval."""
    return bridge().run("map_selection", {})


@mcp.tool(annotations=ToolAnnotations(read_only_hint=False, destructive_hint=False), structured_output=True)
def gis_live_navigate(longitude: float, latitude: float, zoom: float) -> dict[str, Any]:
    """Move only the live map camera. Requires GIS user approval and does not persist data."""
    return bridge().run("map_navigate", {"longitude": longitude, "latitude": latitude, "zoom": zoom})


if __name__ == "__main__":
    mcp.run(transport="stdio")
