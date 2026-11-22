from __future__ import annotations

import httpx
import pytest

from tools.check_self_host_fixture import check, check_origin


@pytest.mark.parametrize(
    "origin",
    [
        "http://localhost:8000",
        "https://public.example:443",
        "https://localhost",
        "https://user:password@localhost:8000",
        "https://localhost:8000/path",
        "https://localhost:8000?query=1",
    ],
)
def test_fixture_origin_rejects_nonisolated_targets(origin: str) -> None:
    with pytest.raises(ValueError, match="loopback HTTPS"):
        check_origin(origin)


def test_fixture_origin_normalizes_loopback() -> None:
    assert check_origin("https://localhost:8000/") == "https://localhost:8000"


def test_fixture_smoke_rejects_octet_stream_module_worker() -> None:
    def handle(request: httpx.Request) -> httpx.Response:
        if request.url.path == "/assets/index-fixture.js":
            return httpx.Response(
                200,
                headers={"content-type": "application/javascript"},
                text='const worker="/assets/maplibre-gl-worker-fixture.mjs";',
            )
        if request.url.path == "/assets/maplibre-gl-worker-fixture.mjs":
            return httpx.Response(200, headers={"content-type": "application/octet-stream"})
        return httpx.Response(
            200, text='OpenVietGIS<script src="/assets/index-fixture.js"></script>'
        )

    with httpx.Client(
        base_url="https://localhost:8000", transport=httpx.MockTransport(handle)
    ) as client:
        with pytest.raises(RuntimeError, match="worker must be served as JavaScript"):
            check(client, "https://localhost:8000")
