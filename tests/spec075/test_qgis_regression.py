"""Dependency-free subset of existing QGIS contracts; no native QGIS or live server."""
from __future__ import annotations

import hashlib
import shutil
import tempfile
import unittest
import zipfile
from pathlib import Path

from integrations.qgis_plugin.openvietgis_connector.client import ApiError
from integrations.qgis_plugin.openvietgis_connector.drafts import (
    draft_path, merge_snapshots, read_draft, rebind_pending, write_draft,
)
from integrations.qgis_plugin.openvietgis_connector.managed_client import (
    diff_managed_snapshots, validate_managed_delta, native_geometry_wkt,
)
from tools.package_qgis_plugin import PLUGIN_ROOT, REQUIRED, package_qgis_plugin


class QgisRegression(unittest.TestCase):
    def test_packaging_exact_reviewed_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            plugin = root / "plugin"
            shutil.copytree(PLUGIN_ROOT, plugin)
            (plugin / "local-notes.txt").write_text("synthetic notes")
            (plugin / "unreviewed.py").write_text("# test only")
            a, b = root / "a.zip", root / "b.zip"
            self.assertEqual(package_qgis_plugin(a, plugin_root=plugin),
                             package_qgis_plugin(b, plugin_root=plugin))
            self.assertEqual(a.read_bytes(), b.read_bytes())
            with zipfile.ZipFile(a) as archive:
                self.assertEqual(set(archive.namelist()),
                                 {"openvietgis_connector/" + f for f in REQUIRED})
            self.assertTrue((root / "a.zip.sha256").read_text().startswith(hashlib.sha256(a.read_bytes()).hexdigest()))
            (plugin / "client.py").unlink()
            (plugin / "client.py").symlink_to(PLUGIN_ROOT / "client.py")
            with self.assertRaises(ValueError):
                package_qgis_plugin(b, plugin_root=plugin)

    def test_stable_house_ids_and_deletion_guard(self):
        # QGIS sync transports existing feature IDs and raw address strings unchanged.
        def feature(number):
            return {"geometry": {"type": "Point", "coordinates": [11800000.123456789, 1200000]},
                    "properties": {"house_number": number, "street_name": "Lê Lợi"}}
        baseline = {"house-a": feature("8B/2"), "house-b": feature("8B/4")}
        current = {**baseline, "house-a": feature("8B/2A")}
        self.assertEqual(diff_managed_snapshots(baseline, current),
                         [{"op_type": "update", "feature_id": "house-a", **current["house-a"]}])
        self.assertEqual(diff_managed_snapshots(current, current), [])
        with self.assertRaises(ApiError):
            diff_managed_snapshots(baseline, {"house-b": baseline["house-b"]})
        self.assertEqual(native_geometry_wkt(baseline["house-a"]["geometry"]), "POINT (11800000.12345679 1200000)")

    def test_delta_cursor_and_duplicate_house_id(self):
        change = {"cursor": 4, "layer_id": "houses", "source_id": "src",
                  "content_revision": "a" * 64, "upserts": [{"feature_id": "house-a"}], "removals": []}
        page = {"reset_required": False, "has_more": False, "next_cursor": 4,
                "permission_epoch": 1, "changes": [change]}
        self.assertEqual(validate_managed_delta(page, cursor=3).next_cursor, 4)
        for bad in [
            {**page, "next_cursor": 2},
            {**page, "permission_epoch": True},
            {**page, "changes": [change, change]},
            {**page, "changes": [{**change, "removals": ["house-a"]}]},
            {**page, "changes": [{**change, "content_revision": "old"}]},
        ]:
            with self.subTest(bad=bad), self.assertRaises(ApiError):
                validate_managed_delta(bad, cursor=3)

    def test_draft_recovery_and_revision_binding(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = draft_path(root, "https://example.invalid", "o", "p", "houses")
            self.assertNotEqual(path, draft_path(root, "https://example.invalid", "o", "other", "houses"))
            data = {"version": 1, "local": {"house-a": "8B/2"}}
            write_draft(path, data)
            self.assertEqual(read_draft(path), data)
            with self.assertRaises(ValueError):
                write_draft(path, {"version": 1, "value": float("nan")})
            self.assertEqual(read_draft(path), data)
            self.assertEqual(merge_snapshots({"house-a": "2"}, {"house-a": "4"}, {"house-a": "6"})[1], ["house-a"])
            baseline = {"house-a": "2"}
            pending = {"operation_id": "op", "request": {"operation_id": "op", "source_id": "src",
                        "base_content_revision": "a" * 64, "base_schema_revision": "b" * 64, "session_id": "old"}}
            args = {"content_revision": "a" * 64, "schema_revision": "b" * 64, "source_id": "src", "session_id": "new"}
            rebound = rebind_pending(pending, baseline, baseline, **args)
            self.assertEqual(rebound["request"]["session_id"], "new")
            self.assertEqual(rebound["request"]["operation_id"], "op")
            with self.assertRaises(ValueError):
                rebind_pending(pending, baseline, {"house-a": "4"}, **args)


if __name__ == "__main__":
    unittest.main()
