import asyncio
import sys
from uuid import UUID

import httpx
import pytest
from mcp import Client
from mcp.client.stdio import StdioServerParameters

import server
from gis_client import GisClient, Settings

ORG = UUID("00000000-0000-4000-8000-000000000001")
PROJECT = UUID("00000000-0000-4000-8000-000000000002")
LAYER = UUID("00000000-0000-4000-8000-000000000003")
SETTINGS = Settings("https://gis.example/api/v1", ORG, PROJECT, "test-credential")
TOOLS = {"gis_get_project", "gis_get_saved_map", "gis_list_layers", "gis_get_layer", "gis_list_features"}


def _client(handler):
    return GisClient(SETTINGS, httpx.MockTransport(handler))


def test_project_scope_and_token():
    def handle(request):
        assert request.method == "GET"
        assert request.url.path == f"/api/v1/organizations/{ORG}/projects/{PROJECT}/layers"
        assert request.headers["Authorization"] == "Bearer test-credential"
        assert request.url.params["limit"] == "5"
        return httpx.Response(200, json={"items": []})
    assert _client(handle).layers(5) == {"items": []}
    assert "test-credential" not in repr(SETTINGS)


def test_feature_pins_and_crs():
    def handle(request):
        assert request.url.path.endswith(f"/layers/{LAYER}/features")
        assert request.url.params["content_revision"] == "a"*64
        assert request.url.params["schema_revision"] == "b"*64
        assert request.url.params["response_crs"] == "4326"
        assert request.url.params["include_geometry"] == "false"
        return httpx.Response(200, json={"items": []})
    assert _client(handle).features(LAYER, "a"*64, "b"*64) == {"items": []}


@pytest.mark.parametrize("status", [302, 401, 403, 404, 409, 500])
def test_api_failure_closed(status):
    calls = []
    def handle(request):
        calls.append(request)
        return httpx.Response(status, headers={"location": "https://evil.example/"}, json={"secret":"test-credential"})
    with pytest.raises(ValueError, match=f"HTTP {status}") as error:
        _client(handle).project()
    assert len(calls) == 1
    assert "test-credential" not in str(error.value)


def test_invalid_arguments_and_config():
    def handle(_request):
        pytest.fail("invalid argument hit network")
    client = _client(handle)
    with pytest.raises(ValueError):
        client.layers(101)
    with pytest.raises(ValueError):
        client.layer("../other")
    with pytest.raises(ValueError):
        client.features(LAYER, "bad", "b"*64)
    with pytest.raises(ValueError):
        client.features(LAYER, "a"*64, "b"*64, after_feature_id="../other")
    with pytest.raises(ValueError):
        Settings("http://gis.example/api/v1", ORG, PROJECT, "test")
    with pytest.raises(ValueError):
        Settings("https://user:pass@gis.example/api/v1", ORG, PROJECT, "test")


def test_response_bound():
    client = _client(lambda _: httpx.Response(200, content=b"x"*2_000_001))
    with pytest.raises(ValueError, match="too large"):
        client.project()


def test_protocol_discovery_and_call(monkeypatch):
    monkeypatch.setattr(server, "gis", lambda: _client(
        lambda _: httpx.Response(200, json={"items": [{"id": str(LAYER)}]})))
    async def run():
        async with Client(server.mcp) as client:
            result = await client.list_tools()
            assert {tool.name for tool in result.tools} == TOOLS
            output = await client.call_tool("gis_list_layers", {"limit": 5})
            assert not output.is_error
            assert str(LAYER) in str(output.structured_content)
    asyncio.run(run())


def test_stdio_discovery():
    async def run():
        params = StdioServerParameters(command=sys.executable, args=[server.__file__])
        async with Client(params) as client:
            result = await client.list_tools()
            assert {tool.name for tool in result.tools} == TOOLS
    asyncio.run(run())
