"""Legacy ingress compatibility only. E08 authorization and business logic live in Rust."""

from __future__ import annotations

import os
import re
from collections.abc import Awaitable, Callable
from urllib.parse import urlsplit

import httpx
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse, Response

_SCOPE = re.compile(r"^/api/v1/organizations/[^/]+/projects/[^/]+/(?:processing|pro)(?:/|$)")
_REQUEST_HEADERS = {
    "accept",
    "authorization",
    "content-type",
    "cookie",
    "if-none-match",
    "if-modified-since",
    "origin",
    "x-openvietgis-csrf",
    "x-request-id",
}
_RESPONSE_HEADERS = {
    "allow",
    "cache-control",
    "content-disposition",
    "content-type",
    "etag",
    "last-modified",
    "vary",
    "x-request-id",
    "access-control-allow-origin",
    "access-control-allow-credentials",
    "access-control-expose-headers",
}
_MAX_BODY = 2 * 1024 * 1024


def _origin() -> str | None:
    value = os.environ.get("OPENVIETGIS_E08_RUST_URL", "").rstrip("/")
    if not value:
        return None
    parsed = urlsplit(value)
    if (
        parsed.scheme != "http"
        or parsed.hostname not in {"127.0.0.1", "localhost", "api-rust"}
        or parsed.port is None
        or parsed.username is not None
        or parsed.password is not None
        or parsed.path
        or parsed.query
        or parsed.fragment
    ):
        raise ValueError("OPENVIETGIS_E08_RUST_URL must be an internal Rust HTTP origin")
    return value


def install_native_processing(app: FastAPI) -> None:
    upstream = _origin()

    @app.middleware("http")
    async def forward_native_processing(
        request: Request, call_next: Callable[[Request], Awaitable[Response]]
    ) -> Response:
        if not _SCOPE.match(request.url.path) or request.method == "OPTIONS":
            return await call_next(request)
        if upstream is None:
            return JSONResponse(
                {"code": "NATIVE_BACKEND_UNAVAILABLE", "message": "Rust E08 is not configured."},
                status_code=503,
            )
        body = bytearray()
        async for chunk in request.stream():
            body.extend(chunk)
            if len(body) > _MAX_BODY:
                return JSONResponse({"code": "REQUEST_TOO_LARGE"}, status_code=413)
        raw_path = request.scope.get("raw_path") or request.url.path.encode()
        query = request.scope.get("query_string", b"")
        if query:
            raw_path += b"?" + query
        url = httpx.URL(upstream).copy_with(raw_path=raw_path)
        headers = {k: v for k, v in request.headers.items() if k in _REQUEST_HEADERS}
        try:
            async with httpx.AsyncClient(
                trust_env=False, follow_redirects=False, timeout=120.0
            ) as client:
                result = await client.request(
                    request.method, url, headers=headers, content=bytes(body)
                )
        except httpx.RequestError:
            return JSONResponse(
                {"code": "NATIVE_BACKEND_UNAVAILABLE", "message": "Rust E08 is unavailable."},
                status_code=503,
            )
        return Response(
            result.content,
            status_code=result.status_code,
            headers={k: v for k, v in result.headers.items() if k in _RESPONSE_HEADERS},
        )
