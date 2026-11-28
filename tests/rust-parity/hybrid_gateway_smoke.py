"""Exercise the hybrid Nginx dispatch against two local recording upstreams."""

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
P = f"/api/v1/organizations/{ORG}/projects/{PROJECT}"
L = f"{P}/layers/{LAYER}"
# E07 is still FastAPI-owned; protect the whole transfer family at the gateway.
E07_CASES = [
    ("POST", f"{P}/imports/upload", "python", 200),
    ("POST", f"{P}/imports/inspect", "python", 200),
    ("POST", f"{P}/imports/confirm", "python", 200),
    ("POST", f"{P}/exports/preview", "python", 200),
    ("POST", f"{P}/exports/confirm", "python", 200),
    ("GET", f"{P}/jobs?limit=50", "python", 200),
    ("GET", f"{P}/jobs/{LAYER}", "python", 200),
    ("POST", f"{P}/jobs/{LAYER}/cancel", "python", 200),
    ("POST", f"{P}/jobs/{LAYER}/retry", "python", 200),
    ("GET", f"{P}/artifacts/{LAYER}", "python", 200),
    ("GET", f"{P}/artifacts/{LAYER}/download", "python", 200),
]


@contextmanager
def upstream(name):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def respond(self):
            body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            data = json.dumps({
                "backend": name,
                "method": self.command,
                "target": self.path,
                "cookie": self.headers.get("Cookie"),
                "authorization": self.headers.get("Authorization"),
                "csrf": self.headers.get("X-OpenVietGIS-CSRF"),
                "content_type": self.headers.get("Content-Type"),
                "body": body.decode(),
            }).encode()
            self.send_response(418 if self.path.endswith("?status=418") else 200)
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Connection", "close")
            self.send_header("X-Test-Upstream", name)
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(data)

        do_GET = do_HEAD = do_POST = do_PUT = do_DELETE = do_OPTIONS = respond

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


def request(port, method, target, body=None, *, authorization=None, content_type=None):
    headers = {
        "Cookie": "session=fixture",
        "X-OpenVietGIS-CSRF": "fixture",
    }
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


def e07_request_payload(method, target):
    if target.endswith("/imports/upload"):
        boundary = "ovg-e07-fixture"
        body = (
            f"--{boundary}\r\n"
            'Content-Disposition: form-data; name="file"; filename="roads.gpkg"\r\n'
            "Content-Type: application/octet-stream\r\n\r\n"
            "SQLite format 3\x00synthetic fixture\r\n"
            f"--{boundary}--\r\n"
        )
        return body, f"multipart/form-data; boundary={boundary}"
    if target.startswith((f"{P}/imports/", f"{P}/exports/")):
        return "{}", "application/json"
    if method in ("POST", "PUT"):
        return "{}", None
    return None, None


def main():
    with ExitStack() as stack:
        temp = Path(stack.enter_context(tempfile.TemporaryDirectory()))
        legacy = stack.enter_context(upstream("python"))
        upstream_stack = stack.enter_context(ExitStack())
        rust = upstream_stack.enter_context(upstream("rust"))
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

            cases = [
                ("GET", "/api/v1/me", "rust", 200),
                ("GET", f"{L}/features?cursor=a%2Fb", "rust", 200),
                ("GET", f"{L}/features/feature_7-A?include_geometry=false", "rust", 200),
                ("GET", f"{L}/features/{'g' * 128}", "rust", 200),
                ("GET", f"{L}/features/{'g' * 129}", "python", 200),
                ("GET", f"{L}/features/bad.id", "python", 200),
                ("GET", f"{L}/viewport?status=418", "rust", 418),
                ("HEAD", f"{L}/display-extent", "python", 200),
                ("HEAD", f"{L}/tiles/9/42/10.mvt", "rust", 200),
                ("GET", f"{L}/tiles/9/42/10.mvt", "rust", 200),
                ("POST", f"{L}/query", "rust", 200),
                ("DELETE", f"{P}/saved-queries/{LAYER}", "rust", 200),
                ("POST", f"{P}/editing/snap", "rust", 200),
                ("POST", f"{P}/sources/{LAYER}/changesets", "python", 200),
                ("POST", f"{P}/changesets/{LAYER}/revert", "python", 200),
                ("OPTIONS", f"{L}/query", "python", 200),
                ("POST", f"{L}/viewport", "python", 200),
                ("GET", f"{L}/viewport/unmapped", "python", 200),
                ("GET", f"{L}/%76iewport", "python", 200),
                ("GET", "/api/v1/auth/oidc/keycloak/start", "python", 200),
                *E07_CASES,
                ("GET", f"{P}/cartography/layouts", "python", 200),
                ("GET", f"{P}/processing/algorithms", "rust", 200),
                ("POST", f"{P}/processing/jobs", "rust", 200),
                ("PUT", f"{P}/processing/models/{LAYER}", "rust", 200),
                ("DELETE", f"{P}/processing/models/{LAYER}", "rust", 200),
                ("POST", f"{P}/pro/geometry", "rust", 200),
                ("GET", f"{P}/modules/house-numbering", "python", 200),
                ("GET", f"{P}/integration/events", "python", 200),
                ("GET", "/api/v1/health/live", "python", 200),
                ("GET", "/api/v1/health/ready", "python", 200),
                ("GET", "/api/v1/ops/health/ready", "python", 200),
                ("GET", "/api/v1/ops/metrics", "python", 200),
                ("GET", "/api/v1/ops/release-capabilities", "rust", 200),
                ("GET", "/api/v1/ops/release-capabilities/extra", "python", 200),
                ("HEAD", "/api/v1/ops/health/live", "python", 200),
                ("GET", f"/api/v1/organizations/{ORG}/ops/entitlement", "rust", 200),
                ("PUT", f"/api/v1/organizations/{ORG}/ops/entitlement", "rust", 200),
                ("POST", f"/api/v1/organizations/{ORG}/ops/entitlement", "python", 200),
            ]
            status, backend, _ = request(
                port, "GET", f"{P}/processing/algorithms", authorization="Bearer fixture"
            )
            assert (status, backend) == (200, "rust")
            for method, target, expected, expected_status in cases:
                sent, content_type = e07_request_payload(method, target)
                status, backend, raw = request(
                    port, method, target, sent, content_type=content_type
                )
                assert (status, backend) == (expected_status, expected), (
                    method,
                    target,
                    status,
                    backend,
                )
                if method != "HEAD":
                    assert json.loads(raw) == {
                        "backend": expected,
                        "method": method,
                        "target": target,
                        "cookie": "session=fixture",
                        "authorization": None,
                        "csrf": "fixture",
                        "content_type": content_type,
                        "body": sent or "",
                    }, (method, target, raw)

            for target, expected in (
                (f"{L}/viewport", "python"),
                (f"{L}/features/feature_7-A", "python"),
                (f"{L}/tiles/9/42/10.mvt", "rust"),
                (f"/api/v1/organizations/{ORG}/ops/entitlement", "python"),
            ):
                status, backend, raw = request(port, "GET", target, authorization="Bearer fixture")
                assert (status, backend) == (200, expected)
                assert json.loads(raw)["authorization"] == "Bearer fixture"

            for method, target, expected, expected_status in E07_CASES:
                sent, content_type = e07_request_payload(method, target)
                status, backend, raw = request(
                    port,
                    method,
                    target,
                    sent,
                    authorization="Bearer fixture",
                    content_type=content_type,
                )
                assert (status, backend) == (expected_status, expected), (
                    "E07 bearer",
                    method,
                    target,
                    status,
                    backend,
                )
                assert json.loads(raw) == {
                    "backend": expected,
                    "method": method,
                    "target": target,
                    "cookie": "session=fixture",
                    "authorization": "Bearer fixture",
                    "csrf": "fixture",
                    "content_type": content_type,
                    "body": sent or "",
                }, ("E07 bearer contract", method, target, raw)

            upstream_stack.close()
            assert request(port, "GET", f"{L}/viewport")[:2] == (502, None)
            assert request(port, "GET", f"{P}/cartography/layouts")[:2] == (200, "python")
            for method, target, expected, expected_status in E07_CASES:
                sent, content_type = e07_request_payload(method, target)
                status, backend, raw = request(
                    port, method, target, sent, content_type=content_type
                )
                assert (status, backend) == (expected_status, expected), (
                    "E07 after Rust outage",
                    method,
                    target,
                    status,
                    backend,
                )
                assert json.loads(raw)["body"] == (sent or "")
        finally:
            nginx.terminate()
            nginx.wait(timeout=5)
    print(
        f"{len(cases)} cookie routes, {len(E07_CASES)} E07 bearer routes, "
        f"{len(E07_CASES)} retained E07 routes after Rust outage, and two outage checks passed"
    )


if __name__ == "__main__":
    main()
