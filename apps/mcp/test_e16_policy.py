"""Shared native/legacy golden and negative contract fixtures."""
from pathlib import Path
import time

import pytest
from starlette.testclient import TestClient

from bridge_http import create_app
from live_bridge import BridgeError, command_params
from test_remote import VERIFIER, signed


def test_navigation_golden():
    for row in (Path(__file__).parent / "fixtures/e16-navigation.tsv").read_text().splitlines():
        lng, lat, zoom, expected = row.split("\t")
        params = dict(longitude=float(lng), latitude=float(lat), zoom=float(zoom))
        if expected == "ok":
            assert command_params("map_navigate", params) == params
        else:
            with pytest.raises(BridgeError):
                command_params("map_navigate", params)


@pytest.mark.parametrize("payload", [
    '{"x":NaN}',
    '{"x":1e999}',
    '{"x":1,"x":2}',
    '{"x":' + '[' * 1100 + '0' + ']' * 1100 + '}',
])
def test_invalid_bridge_json(payload):
    client = TestClient(create_app())
    response = client.post("/v1/sessions", content=payload, headers={
        "Origin": "null", "Content-Type": "application/json"
    })
    assert response.status_code == 422


@pytest.mark.parametrize("claims", [
    {"exp": str(int(time.time()) + 300)},
    {"exp": time.time() + 300},
    {"iat": True, "exp": 2},
    {"iat": int(time.time()), "exp": int(time.time())},
    {"exp": float("inf")},
])
def test_invalid_token_dates(claims):
    assert VERIFIER._verify(signed(**claims)) is None
