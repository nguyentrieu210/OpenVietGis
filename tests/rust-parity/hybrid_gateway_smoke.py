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


def request(port, method, target, body=None, *, authorization=None):
    headers = {
        "Cookie": "session=fixture",
        "X-OpenVietGIS-CSRF": "fixture",
    }
    if authorization is not None:
        headers["Authorization"] = authorization
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=4)
    try:
        connection.request(method, target, body, headers)
        response = connection.getresponse()
        return response.status, response.getheader("X-Test-Upstream"), response.read()
    finally:
        connection.close()


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
                ("POST", f"{P}/imports/upload", "python", 200),
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
                sent = "{}" if method in ("POST", "PUT") else None
                status, backend, raw = request(port, method, target, sent)
                assert (status, backend) == (expected_status, expected), (method, target, status, backend)
                if method != "HEAD":
                    assert json.loads(raw) == {
                        "backend": expected,
                        "method": method,
                        "target": target,
                        "cookie": "session=fixture",
                        "authorization": None,
                        "csrf": "fixture",
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

            upstream_stack.close()
            assert request(port, "GET", f"{L}/viewport")[:2] == (502, None)
            assert request(port, "GET", f"{P}/cartography/layouts")[:2] == (200, "python")
        finally:
            nginx.terminate()
            nginx.wait(timeout=5)
    print(f"{len(cases) + 3} routing and forwarding checks and two outage checks passed")


if __name__ == "__main__":
    main()
