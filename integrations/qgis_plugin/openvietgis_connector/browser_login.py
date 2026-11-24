"""Short-lived loopback handoff from the authenticated OpenVietGIS web UI."""
from __future__ import annotations

import hmac
import queue
import secrets
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlencode, urlsplit

from .client import OpenVietGisConfig, OpenVietGisError, decode_pairing_code


class BrowserLogin:
    def __init__(self, origin: str) -> None:
        parsed = urlsplit(origin)
        if parsed.scheme != "https" or not parsed.hostname or parsed.path not in ("", "/") or parsed.query or parsed.fragment or parsed.username or parsed.password:
            raise ValueError("Đăng nhập trình duyệt yêu cầu địa chỉ HTTPS gốc.")
        self.origin = f"{parsed.scheme}://{parsed.netloc}"
        self.state = secrets.token_urlsafe(32)
        self.result: queue.Queue[str] = queue.Queue(maxsize=1)
        self.used = False
        self.lock = threading.Lock()
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def do_POST(self):
                try:
                    size = int(self.headers.get("Content-Length", "0"))
                    if (self.path != "/callback" or self.headers.get("Host") != f"127.0.0.1:{owner.port}"
                            or self.headers.get("Origin") != owner.origin
                            or self.headers.get_content_type() != "application/x-www-form-urlencoded"
                            or not 0 < size <= 16384):
                        raise ValueError()
                    self.connection.settimeout(3)
                    data = parse_qs(self.rfile.read(size).decode(), strict_parsing=True)
                    state, code = data["state"], data["pairing_code"]
                    if len(state) != 1 or len(code) != 1 or not hmac.compare_digest(state[0], owner.state):
                        raise ValueError()
                    parsed = decode_pairing_code(code[0])
                    config = OpenVietGisConfig(parsed["server_url"], parsed["organization_id"], parsed["project_id"], parsed["bearer_token"])
                    if config.base_url != owner.origin:
                        raise ValueError()
                    with owner.lock:
                        if owner.used:
                            raise ValueError()
                        owner.result.put_nowait(code[0])
                        owner.used = True
                except (ValueError, KeyError, OSError, OpenVietGisError, queue.Full):
                    self.send_error(400, "Invalid or expired connection")
                    return
                content = b"<!doctype html><meta charset=utf-8><title>OpenVietGIS</title><p>Connected. Return to QGIS.</p>"
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Cache-Control", "no-store")
                self.send_header("Content-Length", str(len(content)))
                self.end_headers()
                self.wfile.write(content)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    @property
    def url(self) -> str:
        return self.origin + "/app?qgis_connect=1#" + urlencode({"port": self.port, "state": self.state})

    def take(self) -> str | None:
        try:
            return self.result.get_nowait()
        except queue.Empty:
            return None

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()
