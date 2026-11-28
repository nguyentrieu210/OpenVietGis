"""Smoke the E12 hybrid gateway dispatch with local recording upstreams."""

from __future__ import annotations

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
CONNECTION = "859cc005-3e22-4707-8763-707d08183407"
LAYER = "859cc005-3e22-4707-8763-707d08183407"
ORG_PATH = f"/api/v1/organizations/{ORG}"
PROJECT_PATH = f"{ORG_PATH}/projects/{PROJECT}"
E12 = f"{PROJECT_PATH}/integration-hub/adapters"
SOURCES = f"{PROJECT_PATH}/external-sources"

# Every Rust-enabled E12 operation is tested for cookie dispatch and bearer fallback.
CASES = [
    ("GET", f"{ORG_PATH}/external-sources/allowlist"),
    ("POST", f"{ORG_PATH}/external-sources/allowlist"),
    ("GET", SOURCES),
    ("POST", SOURCES),
    ("POST", f"{SOURCES}/{CONNECTION}/invalidate-cache"),
    ("POST", f"{SOURCES}/{CONNECTION}/revoke"),
    ("POST", f"{SOURCES}/{CONNECTION}/preview"),
    ("POST", f"{SOURCES}/{CONNECTION}/viewport"),
    ("GET", f"{E12}/qgis/layers/{LAYER}/export?format=geojson"),
    ("GET", f"{E12}/postgis/layers/{LAYER}/export?format=postgis_sql"),
    ("GET", f"{E12}/autocad/layers/{LAYER}/export?format=dxf"),
    ("POST", f"{E12}/qgis/layers/{LAYER}/sync-preview"),
    ("POST", f"{E12}/qgis/layers/{LAYER}/sync-push"),
]

# These routes are accepted by the legacy API but are not enabled in the Rust gateway.
FALLBACK_CASES = [
    ("GET", f"{E12}/ogc_services/layers/{LAYER}/export"),
    ("POST", f"{E12}/autocad/layers/{LAYER}/sync-preview"),
    ("POST", f"{E12}/postgis/layers/{LAYER}/sync-push"),
    ("GET", f"{E12}/qgis/layers/not-a-uuid/export"),
    ("GET", f"{SOURCES}/{CONNECTION}/preview/extra"),
    ("DELETE", SOURCES),
]


@contextmanager
def upstream(name: str):
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
                    "body": body.decode(),
                }
            ).encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Connection", "close")
            self.send_header("X-Test-Upstream", name)
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(data)

        do_GET = do_POST = do_PUT = do_DELETE = do_OPTIONS = respond

        def log_message(self, *_args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    worker = Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        yield server, worker
    finally:
        server.shutdown()
        server.server_close()
        worker.join()


def body_for(method: str, target: str) -> tuple[str | None, str | None]:
    if method != "POST":
        return None, None
    if target.endswith("/sync-preview") or target.endswith("/sync-push"):
        boundary = "ovg-e12-fixture"
        body = (
            f"--{boundary}\r\n"
            'Content-Disposition: form-data; name="file"; filename="layer.gpkg"\r\n'
            "Content-Type: application/octet-stream\r\n\r\n"
            "synthetic GeoPackage dispatch fixture\r\n"
            f"--{boundary}--\r\n"
        )
        return body, f"multipart/form-data; boundary={boundary}"
    return '{"dispatch_probe":"e12"}', "application/json"


def request(
    port: int,
    method: str,
    target: str,
    body: str | None = None,
    *,
    authorization: str | None = None,
    content_type: str | None = None,
) -> tuple[int, str | None, bytes]:
    headers = {"Cookie": "session=e12-fixture", "X-OpenVietGIS-CSRF": "e12-csrf"}
    if authorization is not None:
        headers["Authorization"] = authorization
    if content_type is not None:
        headers["Content-Type"] = content_type
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    try:
        connection.request(method, target, body, headers)
        response = connection.getresponse()
        return response.status, response.getheader("X-Test-Upstream"), response.read()
    finally:
        connection.close()


def main() -> None:
    with ExitStack() as stack:
        temp = Path(stack.enter_context(tempfile.TemporaryDirectory()))
        python_server, _ = stack.enter_context(upstream("python"))
        rust_server, rust_worker = stack.enter_context(upstream("rust"))
        python_port = python_server.server_port
        rust_port = rust_server.server_port
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]

        config = CONFIG.read_text()
        config = config.replace("api-python:8000", f"127.0.0.1:{python_port}")
        config = config.replace("api-rust:8080", f"127.0.0.1:{rust_port}")
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

            cookie_dispatches = 0
            bearer_fallbacks = 0
            for method, target in CASES:
                body, content_type = body_for(method, target)
                status, backend, raw = request(
                    port, method, target, body, content_type=content_type
                )
                assert (status, backend) == (200, "rust"), (
                    "cookie route dispatch",
                    method,
                    target,
                    status,
                    backend,
                )
                expected = {
                    "backend": "rust",
                    "method": method,
                    "target": target,
                    "cookie": "session=e12-fixture",
                    "authorization": None,
                    "csrf": "e12-csrf",
                    "content_type": content_type,
                    "body": body or "",
                }
                assert json.loads(raw) == expected, (method, target, raw)
                cookie_dispatches += 1

                status, backend, raw = request(
                    port,
                    method,
                    target,
                    body,
                    authorization="Bearer e12-fixture",
                    content_type=content_type,
                )
                assert (status, backend) == (200, "python"), (
                    "bearer fallback",
                    method,
                    target,
                    status,
                    backend,
                )
                expected["backend"] = "python"
                expected["authorization"] = "Bearer e12-fixture"
                assert json.loads(raw) == expected, (method, target, raw)
                bearer_fallbacks += 1

            unsupported_fallbacks = 0
            for method, target in FALLBACK_CASES:
                body, content_type = body_for(method, target)
                status, backend, raw = request(
                    port, method, target, body, content_type=content_type
                )
                assert (status, backend) == (200, "python"), (
                    "unsupported/malformed fallback",
                    method,
                    target,
                    status,
                    backend,
                )
                assert json.loads(raw)["target"] == target
                unsupported_fallbacks += 1

            # Requests pinned to Rust never silently fail over to Python.
            rust_server.shutdown()
            rust_server.server_close()
            rust_worker.join()
            status, backend, _ = request(port, "GET", f"{ORG_PATH}/external-sources/allowlist")
            assert status == 502 and backend is None, ("Rust outage must fail closed", status, backend)

            print(
                json.dumps(
                    {
                        "cookie_rust_dispatches": cookie_dispatches,
                        "bearer_python_fallbacks": bearer_fallbacks,
                        "unsupported_or_malformed_python_fallbacks": unsupported_fallbacks,
                        "rust_outage_fail_closed": True,
                    },
                    sort_keys=True,
                )
            )
        finally:
            nginx.terminate()
            nginx.wait(timeout=5)


if __name__ == "__main__":
    main()
