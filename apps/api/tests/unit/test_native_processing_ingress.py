"""Retired E08 routes dispatch to Rust, never to a local implementation."""

from __future__ import annotations

import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Thread

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from openvietgis_api.adapters.http.native_processing import install_native_processing

PATH = "/api/v1/organizations/org/projects/project/processing/jobs"


def test_native_ingress_preserves_auth_body_and_raw_query(monkeypatch):
    observed = []

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            observed.append(
                (
                    self.path,
                    self.headers["Authorization"],
                    self.headers["Cookie"],
                    self.headers["X-OpenVietGIS-CSRF"],
                    json.loads(self.rfile.read(int(self.headers["Content-Length"]))),
                )
            )
            self.send_response(201)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"status":"queued"}')

        def log_message(self, *args):
            pass

    with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
        worker = Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            monkeypatch.setenv("OPENVIETGIS_E08_RUST_URL", f"http://127.0.0.1:{server.server_port}")
            app = FastAPI()
            install_native_processing(app)
            with TestClient(app) as client:
                response = client.post(
                    PATH + "?x=a%2Fb",
                    headers={
                        "Authorization": "Bearer test",
                        "Cookie": "ovg_session=test",
                        "X-OpenVietGIS-CSRF": "test-csrf",
                    },
                    json={"algorithm_id": "buffer"},
                )
                assert response.status_code == 201
                assert response.json() == {"status": "queued"}
                assert client.get("/unrelated").status_code == 404
            assert observed == [
                (
                    PATH + "?x=a%2Fb",
                    "Bearer test",
                    "ovg_session=test",
                    "test-csrf",
                    {"algorithm_id": "buffer"},
                )
            ]
        finally:
            server.shutdown()
            worker.join()


def test_retired_route_fails_closed_without_rust(monkeypatch):
    monkeypatch.delenv("OPENVIETGIS_E08_RUST_URL", raising=False)
    app = FastAPI()
    install_native_processing(app)
    with TestClient(app) as client:
        response = client.get(PATH)
        assert response.status_code == 503
        assert response.json()["code"] == "NATIVE_BACKEND_UNAVAILABLE"


@pytest.mark.parametrize(
    "url", ["https://example.com", "http://evil.test:8080", "http://127.0.0.1:80/a"]
)
def test_no_arbitrary_upstream(monkeypatch, url):
    monkeypatch.setenv("OPENVIETGIS_E08_RUST_URL", url)
    with pytest.raises(ValueError):
        install_native_processing(FastAPI())
