"""Local candidate HTTP relay / partner UI. Do not expose without release review."""
from __future__ import annotations
import json
import math
import os
from pathlib import Path
from urllib.parse import urlsplit

from starlette.applications import Starlette
from starlette.middleware.cors import CORSMiddleware
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
from starlette.responses import JSONResponse, FileResponse, Response
from starlette.routing import Route

from live_bridge import BridgeStore, BridgeError

HERE = Path(__file__).parent
MAX_BODY = 128_000


def _json_object(pairs):
    obj = {}
    for key, value in pairs:
        if key in obj:
            raise ValueError("Duplicate JSON member")
        obj[key] = value
    return obj


def _reject_constant(value):
    raise ValueError("Non-finite JSON number")


def _finite_float(value):
    parsed = float(value)
    if not math.isfinite(parsed):
        raise ValueError("Non-finite JSON number")
    return parsed


def _bounded_depth(obj):
    pending = [(obj, 0)]
    while pending:
        value, depth = pending.pop()
        if depth > 64:
            raise ValueError("JSON nesting exceeds limit")
        if isinstance(value, dict):
            pending.extend((item, depth + 1) for item in value.values())
        elif isinstance(value, list):
            pending.extend((item, depth + 1) for item in value)


async def body(request):
    if request.headers.get("content-type", "").split(";")[0] != "application/json":
        raise BridgeError(415, "JSON required")
    data = bytearray()
    async for chunk in request.stream():
        data.extend(chunk)
        if len(data) > MAX_BODY:
            raise BridgeError(413, "Request too large")
    try:
        result = json.loads(data, object_pairs_hook=_json_object, parse_constant=_reject_constant, parse_float=_finite_float)
        _bounded_depth(result)
    except (ValueError, UnicodeError, RecursionError):
        raise BridgeError(422, "Invalid JSON") from None
    if not isinstance(result, dict):
        raise BridgeError(422, "JSON object required")
    return result


def token(request):
    header = request.headers.get("authorization", "")
    if not header.startswith("Bearer ") or not 20 <= len(header[7:]) <= 128:
        raise BridgeError(404, "Session unavailable")
    return header[7:]


def create_app(store=None):
    store = store or BridgeStore()
    origin = os.getenv("OVG_HOST_ORIGIN", "https://openvietgis.cloud")
    parsed = urlsplit(origin)
    if parsed.scheme not in ("https", "http") or origin != parsed.scheme + "://" + parsed.netloc or parsed.username:
        raise ValueError("Invalid GIS host origin")

    async def plugin(request):
        return FileResponse(HERE / "plugin" / "index.html", headers={
            "Content-Security-Policy": f"default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors {origin}; base-uri 'none'; form-action 'none'",
            "Referrer-Policy": "no-referrer", "Cache-Control": "no-store",
        })

    async def script(request):
        return FileResponse(HERE / "plugin" / "plugin.js")
    async def style(request):
        return FileResponse(HERE / "plugin" / "style.css")
    async def sdk(request):
        path = os.getenv("OVG_PARTNER_SDK_PATH")
        if not path or not Path(path).is_file():
            return Response("Approved partner SDK not configured", status_code=503)
        return FileResponse(path, media_type="text/javascript")
    async def config(request):
        return JSONResponse({"host_origin": origin})
    async def create(request):
        # Public candidate only creates empty, bounded sessions. It cannot access GIS.
        if request.headers.get("origin") != "null":
            raise BridgeError(403, "Open from the sandboxed plugin")
        await body(request)
        return JSONResponse(store.create(), status_code=201, headers={"Cache-Control": "no-store"})
    async def poll(request):
        await body(request)
        return JSONResponse(store.poll(request.path_params["sid"], token(request)))
    async def ping(request):
        await body(request)
        return JSONResponse(store.ping(request.path_params["sid"], token(request)))
    async def submit(request):
        data = await body(request)
        method = data.get("method")
        if not isinstance(method, str):
            raise BridgeError(422, "Invalid method")
        return JSONResponse(store.submit(request.path_params["sid"], token(request), method, data.get("params")), status_code=201)
    async def ack(request):
        data = await body(request)
        return JSONResponse(store.ack(request.path_params["sid"], token(request), request.path_params["cid"], data.get("ok"), data.get("result")))
    async def result(request):
        return JSONResponse(store.result(request.path_params["sid"], token(request), request.path_params["cid"]))
    async def close(request):
        return JSONResponse(store.close(request.path_params["sid"], token(request)))
    async def error(request, exc):
        return JSONResponse({"error": str(exc)}, status_code=exc.status, headers={"Cache-Control": "no-store"})

    app = Starlette(routes=[
        Route("/v1/index.html", plugin), Route("/v1/plugin.js", script), Route("/v1/style.css", style),
        Route("/v1/sdk.js", sdk), Route("/v1/config", config),
        Route("/v1/sessions", create, methods=["POST"]),
        Route("/v1/sessions/{sid}/poll", poll, methods=["POST"]),
        Route("/v1/sessions/{sid}/ping", ping, methods=["POST"]),
        Route("/v1/sessions/{sid}/commands", submit, methods=["POST"]),
        Route("/v1/sessions/{sid}/commands/{cid}/ack", ack, methods=["POST"]),
        Route("/v1/sessions/{sid}/commands/{cid}/result", result),
        Route("/v1/sessions/{sid}/close", close, methods=["POST"]),
    ], exception_handlers={BridgeError: error})
    async def no_cache(request, call_next):
        response = await call_next(request)
        response.headers["Cache-Control"] = "no-store"
        response.headers["X-Content-Type-Options"] = "nosniff"
        return response
    app.add_middleware(BaseHTTPMiddleware, dispatch=no_cache)
    return CORSMiddleware(app, allow_origins=["null"], allow_methods=["GET", "POST"], allow_headers=["Authorization", "Content-Type"])


app = create_app()
