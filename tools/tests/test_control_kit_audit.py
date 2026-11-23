"""Offline tests for the read-only release asset verifier."""

import hashlib
import tempfile
import unittest
from pathlib import Path

from tools.control_kit_audit import (
    check_assets,
    check_index_assets,
    strip_cloudflare_beacon,
)


class ControlKitTests(unittest.TestCase):
    def test_release_asset_hashes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "assets").mkdir()
            payload = b"test compiled JS"
            (root / "assets/main.js").write_bytes(payload)
            expected = hashlib.sha256(payload).hexdigest()
            self.assertEqual(check_assets(root, {"assets/main.js": expected}), [])
            self.assertIn(
                "checksum mismatch",
                check_assets(root, {"assets/main.js": "bad"})[0].lower(),
            )

    def test_missing_or_escaping_assets(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.assertIn(
                "Missing asset", check_assets(root, {"missing.js": "hash"})[0]
            )
            self.assertIn(
                "escapes web root", check_assets(root, {"../secret": "hash"})[0]
            )

    def test_cloudflare_beacon_only_normalization(self):
        html = b'<body>GIS<script src="https://static.cloudflareinsights.com/beacon.min.js/123" data-cf-beacon="x"></script></body>'
        self.assertEqual(strip_cloudflare_beacon(html), b"<body>GIS</body>")
        self.assertNotEqual(
            strip_cloudflare_beacon(b"<body>changed</body>"),
            b"<body>GIS</body>",
        )

    def test_index_asset_references(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "assets").mkdir()
            (root / "assets/main.js").write_text("ok", encoding="utf-8")
            (root / "index.html").write_text(
                '<script src="/rewrite/assets/main.js"></script>', encoding="utf-8"
            )
            self.assertEqual(check_index_assets(root), [])
            (root / "assets/main.js").unlink()
            self.assertIn("unavailable file", check_index_assets(root)[0])


if __name__ == "__main__":
    unittest.main()
