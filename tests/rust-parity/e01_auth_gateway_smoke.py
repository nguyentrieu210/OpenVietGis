"""Exercise E01 identity dispatch and fail-closed forwarding through Nginx."""

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
API = "/api/v1"


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

        do_GET = do_POST = do_PUT = do_DELETE = respond

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
        "Cookie": "ovg_session=fixture",
        "X-OpenVietGIS-CSRF": "fixture-csrf",
    }
    if body is not None:
        headers["Content-Type"] = "application/json"
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
        python_port = stack.enter_context(upstream("python"))
        rust_stack = stack.enter_context(ExitStack())
        rust_port = rust_stack.enter_context(upstream("rust"))
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]

        config = CONFIG.read_text().replace("api-python:8000", f"127.0.0.1:{python_port}")
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

            invite = f"{API}/organizations/{ORG}/invitations/email"
            avatar_body = json.dumps({"data_url": "data:image/jpeg;base64,ZmFrZQ=="})
            invite_body = json.dumps({"email": "friend@example.test", "organization_role": "member"})
            cases = [
                ("GET", f"{API}/auth/login/keycloak?mode=login&return_to=%2Fapp", "rust", None, None),
                ("GET", f"{API}/auth/callback/keycloak?state=fixture&code=fixture", "rust", None, None),
                ("GET", f"{API}/me/avatar", "rust", None, None),
                ("PUT", f"{API}/me/avatar?format=jpeg", "rust", avatar_body, None),
                ("POST", invite + "?source=settings", "rust", invite_body, None),
                ("GET", f"{API}/auth/login/other", "python", None, None),
                ("GET", f"{API}/auth/callback/does-not-exist?state=x", "python", None, None),
                ("POST", f"{API}/auth/login/keycloak", "python", "{}", None),
                ("POST", f"{API}/auth/callback/keycloak", "python", "{}", None),
                ("POST", f"{API}/me/avatar", "python", avatar_body, None),
                ("GET", invite, "python", None, None),
                ("POST", f"{API}/organizations/not-a-uuid/invitations/email", "python", invite_body, None),
                ("GET", f"{API}/auth/login/keycloak", "python", None, "Bearer fixture"),
                ("GET", f"{API}/auth/callback/keycloak?state=x", "python", None, "Bearer fixture"),
                ("GET", f"{API}/me/avatar", "python", None, "Bearer fixture"),
                ("PUT", f"{API}/me/avatar", "python", avatar_body, "Bearer fixture"),
                ("POST", invite, "python", invite_body, "Bearer fixture"),
                ("GET", f"{API}/auth/login/other", "python", None, "Bearer fixture"),
            ]

            for method, target, expected_backend, body, authorization in cases:
                status, backend, raw = request(
                    port, method, target, body, authorization=authorization
                )
                assert (status, backend) == (200, expected_backend), (
                    method,
                    target,
                    status,
                    backend,
                )
                sent_headers = {
                    "backend": expected_backend,
                    "method": method,
                    "target": target,
                    "cookie": "ovg_session=fixture",
                    "authorization": authorization,
                    "csrf": "fixture-csrf",
                    "content_type": "application/json" if body is not None else None,
                    "body": body or "",
                }
                assert json.loads(raw) == sent_headers, (method, target, raw)

            rust_stack.close()
            outage_cases = [
                ("GET", f"{API}/auth/login/keycloak?mode=login", None),
                ("GET", f"{API}/auth/callback/keycloak?state=fixture", None),
                ("PUT", f"{API}/me/avatar", avatar_body),
                ("POST", invite, invite_body),
            ]
            for method, target, body in outage_cases:
                assert request(port, method, target, body)[:2] == (502, None), (method, target)
            assert request(port, "GET", f"{API}/auth/login/other")[:2] == (200, "python")
        finally:
            nginx.terminate()
            nginx.wait(timeout=5)
    print(f"{len(cases)} routes and {len(outage_cases) + 1} outage/fallback checks passed")


if __name__ == "__main__":
    main()
