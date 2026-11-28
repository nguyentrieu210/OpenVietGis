"""Verify E09 raster dispatch on the isolated hybrid Nginx gateway.

Requires Python 3 stdlib and an installed nginx binary. No app, database,
GDAL, credentials or production service is contacted.
"""

import http.client
import json
import socket
import subprocess
import tempfile
import time
from contextlib import ExitStack, contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread

CONFIG = Path(__file__).resolve().parents[2] / "infra/rust-platform/hybrid-gateway.conf"
ORG = "27ac050e-e443-480e-bdc5-b36bcab9403d"
PROJECT = "c498bd02-fdd3-47b1-a897-ca2b1d59f60a"
LAYER = "859cc005-3e22-4707-8763-707d08183407"
BASE = f"/api/v1/organizations/{ORG}/projects/{PROJECT}/raster"
LAYERS = f"{BASE}/layers"
BOUNDARY = "ovg-e09-gateway-fixture"


def multipart(fields, files=()):
    parts = []
    for name, value in fields:
        parts.append(
            f"--{BOUNDARY}\r\n"
            f'Content-Disposition: form-data; name="{name}"\r\n\r\n'
            f"{value}\r\n"
        )
    for name, filename, content_type, value in files:
        parts.append(
            f"--{BOUNDARY}\r\n"
            f'Content-Disposition: form-data; name="{name}"; filename="{filename}"\r\n'
            f"Content-Type: {content_type}\r\n\r\n"
            f"{value}\r\n"
        )
    parts.append(f"--{BOUNDARY}--\r\n")
    return "".join(parts), f"multipart/form-data; boundary={BOUNDARY}"


def e09_cases():
    analyze_body, analyze_type = multipart(
        [("method", "band"), ("band_a", "1"), ("band_b", "2")],
        [("file", "fixture.tif", "image/tiff", "II*\\x00fixture")],
    )
    store_body, store_type = multipart(
        [("method", "band"), ("name", "E09 fixture"), ("band_a", "1")],
        [("file", "fixture.tif", "image/tiff", "II*\\x00fixture")],
    )
    report_body, report_type = multipart(
        [("method", "contour"), ("source_id", LAYER), ("interval", "10")],
    )
    terrain_body = json.dumps(
        {
            "elevation_unit": "metre",
            "vertical_reference": "local",
            "reference_label": "Gateway fixture",
            "acknowledge_dem": True,
        },
        separators=(",", ":"),
    )
    return [
        ("GET", LAYERS + "?limit=25", "rust", None, None),
        ("POST", BASE + "/analyze", "rust", analyze_body, analyze_type),
        ("POST", LAYERS, "rust", store_body, store_type),
        ("POST", BASE + "/reports", "rust", report_body, report_type),
        (
            "POST",
            f"{LAYERS}/{LAYER}/terrain-preview",
            "rust",
            terrain_body,
            "application/json",
        ),
        ("GET", f"{LAYERS}/{LAYER}/tiles/2/1/2.png", "rust", None, None),
        ("GET", f"{LAYERS}/{LAYER}/file", "rust", None, None),
        ("DELETE", f"{LAYERS}/{LAYER}", "rust", None, None),
    ]


@contextmanager
def upstream(name):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def respond(self):
            body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            data = json.dumps(
                {
                    "backend": name,
                    "method": self.command,
                    "target": self.path,
                    "cookie": self.headers.get("Cookie"),
                    "authorization": self.headers.get("Authorization"),
                    "csrf": self.headers.get("X-OpenVietGIS-CSRF"),
                    "content_type": self.headers.get("Content-Type"),
                    "body": body.decode("ascii"),
                }
            ).encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Connection", "close")
            self.send_header("X-Test-Upstream", name)
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(data)

        do_GET = do_HEAD = do_POST = do_DELETE = respond

        def log_message(self, *_args):
            pass

    with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
        worker = Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            yield server.server_port
        finally:
            server.shutdown()
            worker.join()


def request(
    port,
    method,
    target,
    body=None,
    *,
    cookie="session=fixture",
    csrf="fixture",
    authorization=None,
    content_type=None,
):
    headers = {}
    if cookie is not None:
        headers["Cookie"] = cookie
    if csrf is not None:
        headers["X-OpenVietGIS-CSRF"] = csrf
    if authorization is not None:
        headers["Authorization"] = authorization
    if content_type is not None:
        headers["Content-Type"] = content_type
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=4)
    try:
        connection.request(method, target, body, headers)
        response = connection.getresponse()
        return response.status, response.getheader("X-Test-Upstream"), response.read()
    finally:
        connection.close()


def main():
    cases = e09_cases()
    with ExitStack() as stack:
        temp = Path(stack.enter_context(tempfile.TemporaryDirectory()))
        legacy = stack.enter_context(upstream("python"))
        rust_stack = stack.enter_context(ExitStack())
        rust = rust_stack.enter_context(upstream("rust"))
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]

        config = CONFIG.read_text().replace("api-python:8000", f"127.0.0.1:{legacy}")
        config = config.replace("api-rust:8080", f"127.0.0.1:{rust}")
        config = config.replace("listen 8080;", f"listen 127.0.0.1:{port};")
        client_body = temp / "body"
        client_body.mkdir(mode=0o777)
        path = temp / "nginx.conf"
        path.write_text(
            f"pid {temp}/nginx.pid; error_log stderr crit;\n"
            f"events {{ worker_connections 64; }} http {{ access_log off; "
            f"client_body_temp_path {client_body};\n{config}\n}}\n"
        )
        command = ["nginx", "-p", str(temp), "-c", str(path)]
        subprocess.run([*command, "-t"], check=True)
        nginx = subprocess.Popen([*command, "-g", "daemon off;"])
        try:
            for _ in range(40):
                try:
                    status, _, _ = request(port, "GET", "/healthz")
                    if status == 200:
                        break
                except ConnectionRefusedError:
                    time.sleep(0.05)
            else:
                raise RuntimeError("Nginx did not start")

            for method, target, expected, body, content_type in cases:
                status, backend, raw = request(
                    port, method, target, body, content_type=content_type
                )
                assert (status, backend) == (200, expected), (
                    method,
                    target,
                    status,
                    backend,
                )
                echoed = json.loads(raw)
                assert echoed == {
                    "backend": expected,
                    "method": method,
                    "target": target,
                    "cookie": "session=fixture",
                    "authorization": None,
                    "csrf": "fixture",
                    "content_type": content_type,
                    "body": body or "",
                }, (method, target, echoed)

                # Legacy bearer callers stay on FastAPI until that auth path is
                # separately certified. The route must not be selected by the
                # presence of a cookie from a different credential type.
                status, backend, raw = request(
                    port,
                    method,
                    target,
                    body,
                    cookie=None,
                    csrf=None,
                    authorization="Bearer fixture",
                    content_type=content_type,
                )
                assert (status, backend) == (200, "python"), (
                    "bearer fallback",
                    method,
                    target,
                    status,
                    backend,
                )
                assert json.loads(raw)["authorization"] == "Bearer fixture"

            malformed = f"{LAYERS}/{LAYER}/tiles/not-a-zoom/1/2.png"
            assert request(port, "GET", malformed)[:2] == (200, "python")

            rust_stack.close()
            # Cookie-session traffic is pinned to Rust and fails closed if its
            # upstream is unavailable; bearer requests still reach FastAPI.
            assert request(port, "GET", LAYERS)[:2] == (502, None)
            assert request(
                port,
                "GET",
                LAYERS,
                cookie=None,
                csrf=None,
                authorization="Bearer fixture",
            )[:2] == (200, "python")
            assert request(port, "GET", "/api/v1/health/live")[:2] == (200, "python")
        finally:
            nginx.terminate()
            nginx.wait(timeout=5)

    print(
        f"{len(cases)} E09 Rust dispatches, {len(cases)} bearer fallbacks, "
        "malformed-route fallback and fail-closed upstream checks passed"
    )


if __name__ == "__main__":
    main()
