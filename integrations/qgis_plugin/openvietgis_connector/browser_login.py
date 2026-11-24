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
        self.confirmed = threading.Event()
        self.success = False
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
                # A received credential is not yet a successful GIS connection.
                # Wait until the QGIS UI has validated its server profile.
                confirmed = owner.confirmed.wait(40)
                if confirmed and owner.success:
                    title = "Kết nối QGIS thành công"
                    detail = "Plugin đã xác thực và tải quyền dự án. QGIS sẽ tự chuyển về cửa sổ làm việc nếu hệ điều hành cho phép."
                    action = "Nếu cửa sổ chưa tự chuyển, hãy bấm biểu tượng QGIS trên thanh tác vụ hoặc Alt + Tab. Bạn có thể đóng tab này."
                    color = "#087c65"
                    status = 200
                elif confirmed:
                    title = "Chưa kết nối được QGIS"
                    detail = "QGIS đã nhận kết quả cấp quyền nhưng chưa xác thực được kết nối máy chủ."
                    action = "Quay lại QGIS để xem lỗi và thử đăng nhập lại."
                    color = "#a13b24"
                    status = 200
                else:
                    title = "QGIS đang kiểm tra kết nối"
                    detail = "Đã chuyển yêu cầu cấp quyền tới QGIS, nhưng chưa nhận xác nhận."
                    action = "Quay lại QGIS để kiểm tra trạng thái. Không cần tải lại trang này."
                    color = "#93600c"
                    status = 200
                content = f"""<!doctype html><html lang="vi"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title} · OpenVietGIS</title>
<style>body{{margin:0;background:#f0f5f5;font:16px system-ui;color:#19343c}}
main{{max-width:520px;margin:12vh auto;padding:32px;background:white;border:1px solid #d6e3e4;border-radius:16px}}
h1{{font-size:25px;color:{color}}}p{{line-height:1.6}}small{{color:#53646a}}button{{background:#124e55;color:white;border:0;padding:12px 20px;border-radius:8px;font:inherit;cursor:pointer}}</style>
<main><small>OpenVietGIS · QGIS Desktop</small><h1>{title}</h1><p>{detail}</p>
<p>{action}</p><button onclick="window.close()">Đóng tab này</button></main></html>""".encode()
                self.send_response(status)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Cache-Control", "no-store")
                self.send_header("Referrer-Policy", "no-referrer")
                self.send_header("X-Content-Type-Options", "nosniff")
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

    def complete(self, success: bool) -> None:
        self.success = success
        self.confirmed.set()

    def close(self) -> None:
        if not self.confirmed.is_set():
            self.complete(False)
        self.server.shutdown()
        self.server.server_close()
