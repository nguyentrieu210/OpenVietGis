"""Loopback-only static smoke-test server with restrictive CAD-origin headers."""

import argparse
import re
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("root", type=Path)
parser.add_argument("port", type=int)
parser.add_argument("parent_origin")
args = parser.parse_args()
if not re.fullmatch(r"http://127\.0\.0\.1:[1-9][0-9]{0,4}", args.parent_origin):
    raise SystemExit("Only a loopback fixture parent is allowed")


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *a, **kw):
        super().__init__(*a, directory=str(args.root), **kw)

    def list_directory(self, path):
        self.send_error(404)

    def end_headers(self):
        self.send_header(
            "Content-Security-Policy",
            "default-src 'none'; "
            "script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; "
            "style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; "
            "font-src 'self' data:; connect-src 'self'; worker-src 'self' blob:; "
            "frame-src 'self'; frame-ancestors 'self' " + args.parent_origin + "; "
            "form-action 'none'; object-src 'none'; base-uri 'none'",
        )
        self.send_header("Cache-Control", "no-store")
        self.send_header("Referrer-Policy", "no-referrer")
        self.send_header("X-Content-Type-Options", "nosniff")
        super().end_headers()


ThreadingHTTPServer(("127.0.0.1", args.port), Handler).serve_forever()
