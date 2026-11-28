"""Standalone smoke for the E10 CAD/georeferencing hybrid-gateway slice.

Uses local recording upstreams so it can verify dispatch and transparent request
forwarding without a database or a running Rust service.
"""

import http.client
import json
import socket
import subprocess
import tempfile
import time
from contextlib import ExitStack
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread

CONFIG = Path(__file__).resolve().parents[2] / "infra/rust-platform/hybrid-gateway.conf"
ORG = "27ac050e-e443-480e-bdc5-b36bcab9403d"
PROJECT = "c498bd02-fdd3-47b1-a897-ca2b1d59f60a"
DOCUMENT = "859cc005-3e22-4707-8763-707d08183407"
P = f"/api/v1/organizations/{ORG}/projects/{PROJECT}"
DOC = f"{P}/cad/documents/{DOCUMENT}/versions/2"
STALE_REVISION = "0" * 64
REVISION = "a" * 64
SCHEMA_REVISION = "b" * 64


def upstream(name):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def respond(self):
            body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            body_text = body.decode("utf-8")
            try:
                parsed = json.loads(body_text)
            except (json.JSONDecodeError, UnicodeDecodeError):
                parsed = {}

            status = 200
            decision = "accepted"
            if self.command == "POST" and not self.headers.get("X-OpenVietGIS-CSRF"):
                status, decision = 403, "csrf_required"
            elif parsed.get("expected_content_revision") == STALE_REVISION:
                status, decision = 409, "stale_content_revision"
            elif parsed.get("expected_document_sha256") == STALE_REVISION:
                status, decision = 409, "stale_document_revision"

            result = json.dumps({
                "backend": name,
                "method": self.command,
                "target": self.path,
                "cookie": self.headers.get("Cookie"),
                "authorization": self.headers.get("Authorization"),
                "csrf": self.headers.get("X-OpenVietGIS-CSRF"),
                "content_type": self.headers.get("Content-Type"),
                "body": body_text,
                "decision": decision,
            }).encode()
            self.send_response(status)
            self.send_header("Content-Length", str(len(result)))
            self.send_header("Connection", "close")
            self.send_header("X-Test-Upstream", name)
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(result)

        do_GET = do_HEAD = do_POST = do_PUT = do_DELETE = do_OPTIONS = respond

        def log_message(self, *_args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    worker = Thread(target=server.serve_forever, daemon=True)
    worker.start()
    return server, worker


def stop_upstream(server, worker):
    server.shutdown()
    server.server_close()
    worker.join(timeout=3)


def request(port, method, target, body=b"", *, authorization=None, csrf="fixture",
            content_type="application/json"):
    headers = {"Cookie": "session=fixture"}
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


def multipart_upload():
    boundary = "----ovg-e10-smoke"
    parts = [
        ("units", "meters"),
        ("crs_code", "3857"),
        ("layer_name", "CAD Fixture"),
        ("acknowledge_loss", "true"),
        ("expected_sha256", "c" * 64),
        ("expected_inspection_fingerprint", "d" * 64),
    ]
    chunks = []
    for name, value in parts:
        chunks.append(
            f"--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\""
            f"\r\n\r\n{value}\r\n".encode()
        )
    chunks.append(
        f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; "
        "filename=\"fixture.dxf\"\r\nContent-Type: application/dxf\r\n\r\n"
        "0\r\nSECTION\r\n2\r\nENTITIES\r\n0\r\nENDSEC\r\n0\r\nEOF\r\n"
        f"--{boundary}--\r\n".encode()
    )
    return boundary, b"".join(chunks)


def json_body(value):
    return json.dumps(value, separators=(",", ":")).encode()


def check(port, method, target, expected_backend, *, body=b"", csrf="fixture",
          authorization=None, content_type="application/json", expected_status=200):
    status, backend, raw = request(
        port,
        method,
        target,
        body,
        csrf=csrf,
        authorization=authorization,
        content_type=content_type,
    )
    assert (status, backend) == (expected_status, expected_backend), (
        method, target, status, backend, raw
    )
    if expected_status == 502:
        return None
    received = json.loads(raw)
    assert received["method"] == method
    assert received["target"] == target
    assert received["backend"] == expected_backend
    assert received["cookie"] == "session=fixture"
    assert received["authorization"] == authorization
    assert received["csrf"] == csrf
    assert received["content_type"] == content_type
    assert received["body"] == body.decode("utf-8")
    return received


def main():
    with ExitStack() as stack:
        temp = Path(stack.enter_context(tempfile.TemporaryDirectory()))
        legacy, legacy_worker = upstream("python")
        rust, rust_worker = upstream("rust")
        stack.callback(stop_upstream, legacy, legacy_worker)
        stack.callback(stop_upstream, rust, rust_worker)

        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]

        config = CONFIG.read_text()
        config = config.replace("api-python:8000", f"127.0.0.1:{legacy.server_port}")
        config = config.replace("api-rust:8080", f"127.0.0.1:{rust.server_port}")
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
                    status, _, _ = request(port, "GET", "/healthz", csrf=None, content_type=None)
                    if status == 200:
                        break
                except ConnectionRefusedError:
                    time.sleep(0.05)
            else:
                raise RuntimeError("Nginx did not start")

            controls = [[[0.0, 0.0], [10.0, 20.0]],
                        [[1.0, 0.0], [11.0, 20.0]],
                        [[0.0, 1.0], [10.0, 21.0]]]
            georef = {
                "crs_code": 3857,
                "units": "meters",
                "controls": controls,
                "tolerance_m": 0.25,
                "max_scale_deviation": 0.01,
            }
            import_georef = georef | {
                "layer_name": "CAD Georeferenced",
                "expected_sha256": "c" * 64,
                "expected_inspection_fingerprint": "d" * 64,
                "acknowledge_loss": True,
            }
            draft = {
                "conversion_id": "a640e08f-b03a-462d-a0d0-6ab56f3120de",
                "expected_document_sha256": "c" * 64,
                "expected_content_revision": REVISION,
                "expected_schema_revision": SCHEMA_REVISION,
            }
            native_preview = {
                "conversion_id": "a640e08f-b03a-462d-a0d0-6ab56f3120de",
                "expected_candidate_sha256": "e" * 64,
            }
            export = {
                "layer_id": DOCUMENT,
                "units": "meters",
                "expected_content_revision": REVISION,
                "acknowledge_attribute_loss": True,
            }
            boundary, multipart = multipart_upload()
            multipart_type = f"multipart/form-data; boundary={boundary}"

            routes = [
                ("POST", f"{P}/cad/inspect", b"multipart", multipart_type),
                ("POST", f"{P}/cad/import", b"multipart", multipart_type),
                ("POST", f"{P}/cad/export?trace=smoke", json_body(export), "application/json"),
                ("POST", f"{DOC}/inspect-georef", json_body(georef), "application/json"),
                ("POST", f"{DOC}/import-georef", json_body(import_georef), "application/json"),
                ("POST", f"{DOC}/export-georef-draft", json_body(draft), "application/json"),
                ("POST", f"{DOC}/preview-native-import", json_body(native_preview), "application/json"),
            ]
            for method, target, payload, media_type in routes:
                sent_body = multipart if payload == b"multipart" else payload
                received = check(
                    port, method, target, "rust", body=sent_body, content_type=media_type
                )
                assert received["decision"] == "accepted"

            stale = draft | {"expected_content_revision": STALE_REVISION}
            received = check(
                port, "POST", f"{DOC}/export-georef-draft", "rust",
                body=json_body(stale), expected_status=409,
            )
            assert received["decision"] == "stale_content_revision"

            received = check(
                port, "POST", f"{DOC}/inspect-georef", "rust",
                body=json_body(georef), csrf=None, expected_status=403,
            )
            assert received["decision"] == "csrf_required"

            malformed = [
                ("POST", f"/api/v1/organizations/{ORG}/projects/{PROJECT}/cad/documents/bad.id/versions/2/inspect-georef"),
                ("POST", f"{DOC.rsplit('/', 1)[0]}/not-a-version/export-georef-draft"),
                ("POST", f"{DOC}/inspect-georef/extra"),
                ("GET", f"{DOC}/inspect-georef"),
                ("GET", f"{P}/cad/import"),
                ("POST", f"{P}/cad/precision/traverse"),
                ("GET", f"{P}/cad/documents"),
                ("POST", f"{DOC}/%69nspect-georef"),
            ]
            for method, target in malformed:
                check(port, method, target, "python", body=b"{}", content_type="application/json")

            bearer = check(
                port, "POST", f"{DOC}/import-georef", "python",
                body=json_body(import_georef), authorization="Bearer fixture",
            )
            assert bearer["authorization"] == "Bearer fixture"

            stop_upstream(rust, rust_worker)
            check(port, "POST", f"{DOC}/export-georef-draft", None,
                  body=json_body(draft), expected_status=502)
            check(port, "POST", f"{DOC}/inspect-georef/extra", "python",
                  body=b"{}", content_type="application/json")
            check(port, "POST", f"{DOC}/import-georef", "python",
                  body=json_body(import_georef), authorization="Bearer fixture")
        finally:
            nginx.terminate()
            nginx.wait(timeout=5)

    print(f"{len(routes)} supported E10 routes, {len(malformed)} fallback paths, "
          "CSRF/revision forwarding, bearer fallback, and Rust outage behavior passed")


if __name__ == "__main__":
    main()
