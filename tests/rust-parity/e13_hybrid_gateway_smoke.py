"""Standalone E13 hybrid Nginx dispatch, auth-header, and outage smoke."""

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
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[2]
CONFIG = ROOT / "infra/rust-platform/hybrid-gateway.conf"
ORG = "27ac050e-e443-480e-bdc5-b36bcab9403d"
PROJECT = "c498bd02-fdd3-47b1-a897-ca2b1d59f60a"
PACKAGE = "859cc005-3e22-4707-8763-707d08183407"
PROJECT_PATH = f"/api/v1/organizations/{ORG}/projects/{PROJECT}"
PACKAGE_PATH = f"{PROJECT_PATH}/field/packages"
SNAPSHOT_PATH = f"{PACKAGE_PATH}/{PACKAGE}/snapshot"
SYNC_PATH = f"{PACKAGE_PATH}/{PACKAGE}/sync"
NATIVE_PATHS = {PACKAGE_PATH, SNAPSHOT_PATH, SYNC_PATH}


@contextmanager
def upstream(name: str):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def respond(self) -> None:
            body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            payload = json.dumps(
                {
                    "backend": name,
                    "method": self.command,
                    "target": self.path,
                    "cookie": self.headers.get("Cookie"),
                    "authorization": self.headers.get("Authorization"),
                    "csrf": self.headers.get("X-OpenVietGIS-CSRF"),
                    "body": body.decode(),
                }
            ).encode()
            path = urlsplit(self.path).path
            status = 200
            if path in NATIVE_PATHS and not self.headers.get("Authorization"):
                if not self.headers.get("Cookie"):
                    status = 401
                elif self.command == "POST" and not self.headers.get("X-OpenVietGIS-CSRF"):
                    status = 403
            self.send_response(status)
            self.send_header("Content-Length", str(len(payload)))
            self.send_header("Connection", "close")
            self.send_header("X-Test-Upstream", name)
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(payload)

        do_GET = do_HEAD = do_POST = do_PUT = do_DELETE = do_OPTIONS = respond

        def log_message(self, *_args) -> None:
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
    port: int,
    method: str,
    target: str,
    body: str | None = None,
    *,
    cookie: str | None = "session=fixture",
    csrf: str | None = "csrf-fixture",
    authorization: str | None = None,
) -> tuple[int, str | None, dict[str, object] | None]:
    headers: dict[str, str] = {}
    if cookie is not None:
        headers["Cookie"] = cookie
    if csrf is not None:
        headers["X-OpenVietGIS-CSRF"] = csrf
    if authorization is not None:
        headers["Authorization"] = authorization
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=4)
    try:
        connection.request(method, target, body, headers)
        response = connection.getresponse()
        raw = response.read()
        try:
            payload = json.loads(raw) if raw else None
        except json.JSONDecodeError:
            payload = None
        return response.status, response.getheader("X-Test-Upstream"), payload
    finally:
        connection.close()


def main() -> None:
    with ExitStack() as stack:
        temp = Path(stack.enter_context(tempfile.TemporaryDirectory()))
        python_port = stack.enter_context(upstream("python"))
        rust_stack = stack.enter_context(ExitStack())
        rust_port = rust_stack.enter_context(upstream("rust"))
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
                    if request(port, "GET", "/healthz")[0] == 200:
                        break
                except ConnectionRefusedError:
                    time.sleep(0.05)
            else:
                raise RuntimeError("Nginx did not start")

            native = [
                ("POST", PACKAGE_PATH, "{}"),
                ("GET", SNAPSHOT_PATH + "?view=browser", None),
                ("POST", SYNC_PATH, "{}"),
            ]
            for method, target, body in native:
                status, backend, payload = request(port, method, target, body)
                assert (status, backend) == (200, "rust"), (method, target, status, backend)
                assert payload == {
                    "backend": "rust",
                    "method": method,
                    "target": target,
                    "cookie": "session=fixture",
                    "authorization": None,
                    "csrf": "csrf-fixture",
                    "body": body or "",
                }, (method, target, payload)

            # Unselected routes, methods, and malformed identifiers stay on Python.
            python_cases = [
                ("GET", PACKAGE_PATH, None),
                ("GET", f"{PACKAGE_PATH}/{PACKAGE}/download", None),
                ("POST", f"{PACKAGE_PATH}/{PACKAGE}/revoke", ""),
                ("POST", PACKAGE_PATH + "/", "{}"),
                ("GET", f"{PACKAGE_PATH}/not-a-uuid/snapshot", None),
                ("POST", f"{PACKAGE_PATH}/not-a-uuid/sync", "{}"),
                ("GET", f"{PACKAGE_PATH}/{PACKAGE}/%73napshot", None),
                ("OPTIONS", PACKAGE_PATH, None),
                ("HEAD", SNAPSHOT_PATH, None),
            ]
            for method, target, body in python_cases:
                status, backend, _ = request(port, method, target, body)
                assert (status, backend) == (200, "python"), (method, target, status, backend)

            # Bearer requests retain the FastAPI path; its bearer auth does not need CSRF.
            for method, target, body in native:
                status, backend, payload = request(
                    port,
                    method,
                    target,
                    body,
                    cookie=None,
                    csrf=None,
                    authorization="Bearer fixture",
                )
                assert (status, backend) == (200, "python"), (method, target, status, backend)
                assert payload["authorization"] == "Bearer fixture"
                assert payload["cookie"] is None and payload["csrf"] is None

            status, backend, _ = request(
                port, "GET", SNAPSHOT_PATH, cookie=None, csrf=None
            )
            assert (status, backend) == (401, "rust"), (status, backend)
            for target in (PACKAGE_PATH, SYNC_PATH):
                status, backend, _ = request(port, "POST", target, "{}", csrf=None)
                assert (status, backend) == (403, "rust"), (target, status, backend)

            # proxy_next_upstream is disabled: native outage fails closed without Python fallback.
            rust_stack.close()
            for method, target, body in native:
                status, backend, _ = request(port, method, target, body)
                assert (status, backend) == (502, None), (method, target, status, backend)
            assert request(port, "GET", PACKAGE_PATH)[0:2] == (200, "python")
        finally:
            nginx.terminate()
            nginx.wait(timeout=5)

    print("E13 gateway: native routing, bearer fallback, auth/CSRF, malformed paths, and outage checks PASS")


if __name__ == "__main__":
    main()
