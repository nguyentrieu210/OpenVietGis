"""Cross-platform fake-HTTP acceptance. Real Windows/MicroStation acceptance still required."""
import hashlib
import importlib.util
import json
from contextlib import closing
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from uuid import uuid4

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from receiver import Connection, ReceiverError, _NoRedirect, receive_layer  # noqa: E402
from v8i_probe import ProbeError, probe  # noqa: E402


class MockResponse:
    def __init__(self, url, data, *, revision="r1", crs="EPSG:5899", sha=None, feature_count="1"):
        self.url = url
        kind = "dxf" if "format=dxf" in url else "gpkg"
        self.data = data
        self.headers = {
            "Content-Type": "application/dxf" if kind == "dxf" else "application/geopackage+sqlite3",
            "Content-Length": str(len(data)),
            "X-Content-SHA256": sha or hashlib.sha256(data).hexdigest(),
            "X-OpenVietGIS-Content-Revision": revision,
            "X-OpenVietGIS-Schema-Revision": "schema1",
            "X-OpenVietGIS-CRS": crs,
            "X-OpenVietGIS-Feature-Count": feature_count,
            "X-OpenVietGIS-Attribute-Loss": "true" if kind == "dxf" else "",
        }

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

    def geturl(self):
        return self.url

    def getcode(self):
        return 200

    def read(self, count):
        return self.data[:count]


class FakeOpener:
    def __init__(self):
        self.revision_second = None
        self.override_hash = False
        self.override_crs = None
        self.dxf = b"0\nSECTION\n2\nHEADER\n0\nENDSEC\n0\nEOF\n"
        self.gpkg = self._make_gpkg()
        self.calls = []

    @staticmethod
    def _make_gpkg():
        with tempfile.TemporaryDirectory() as folder:
            db_path = Path(folder) / "fixture.gpkg"
            with closing(sqlite3.connect(db_path)) as db:
                db.execute("CREATE TABLE gpkg_contents(table_name TEXT,data_type TEXT,srs_id INTEGER)")
                db.execute("CREATE TABLE gpkg_geometry_columns(table_name TEXT,srs_id INTEGER)")
                db.execute("CREATE TABLE parcels(fid INTEGER PRIMARY KEY, name TEXT)")
                db.execute("INSERT INTO parcels VALUES(1,'Test')")
                db.execute("INSERT INTO gpkg_contents VALUES('parcels','features',5899)")
                db.execute("INSERT INTO gpkg_geometry_columns VALUES('parcels',5899)")
                db.commit()
            return db_path.read_bytes()

    def open(self, req, timeout=25):
        url = req.full_url
        self.calls.append(req)
        kind = "format=dxf" in url
        return MockResponse(
            url, self.dxf if kind else self.gpkg,
            revision="r1" if kind or self.revision_second is None else self.revision_second,
            crs=self.override_crs or "EPSG:5899",
            sha="0" * 64 if self.override_hash and kind else None,
        )


class ReceiverTest(unittest.TestCase):
    def setUp(self):
        self.c = Connection("https://openvietgis.cloud", str(uuid4()), str(uuid4()), "private-test-token")
        self.layer = str(uuid4())
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.opener = FakeOpener()

    def test_successful_inbound_pair_same_revision_no_token_on_disk(self):
        bundle = receive_layer(self.c, self.layer, self.tmp.name, opener=self.opener)
        self.assertEqual((bundle / "geometry.dxf").read_bytes(), self.opener.dxf)
        self.assertEqual((bundle / "attributes.gpkg").read_bytes(), self.opener.gpkg)
        receipt = json.loads((bundle / "receipt.json").read_text(encoding="utf-8"))
        self.assertEqual(receipt["content_revision"], "r1")
        self.assertEqual(receipt["crs"], "EPSG:5899")
        self.assertTrue(receipt["dxf_attribute_loss"])
        self.assertFalse(receipt["cad_write_back_enabled"])
        self.assertNotIn(self.c.bearer_token, (bundle / "receipt.json").read_text())
        self.assertEqual(len(self.opener.calls), 2)
        self.assertEqual(self.opener.calls[0].get_header("Authorization"), "Bearer private-test-token")
        self.assertEqual(receive_layer(self.c, self.layer, self.tmp.name, opener=self.opener), bundle)
        self.assertEqual(len(list(Path(self.tmp.name).iterdir())), 1)

    def test_mismatched_remote_revisions_fail_no_partial_output(self):
        self.opener.revision_second = "changed"
        with self.assertRaisesRegex(ReceiverError, "revision/CRS/count"):
            receive_layer(self.c, self.layer, self.tmp.name, opener=self.opener)
        self.assertEqual(list(Path(self.tmp.name).iterdir()), [])

    def test_corrupt_geopackage_rejected_without_publication(self):
        self.opener.gpkg = b"SQLite format 3\x00" + b"not-a-real-database"
        with self.assertRaisesRegex(ReceiverError, "GeoPackage structure"):
            receive_layer(self.c, self.layer, self.tmp.name, opener=self.opener)
        self.assertEqual(list(Path(self.tmp.name).iterdir()), [])

    def test_valid_sqlite_with_wrong_count_fails(self):
        original = self.opener.open
        def changed_count(req, timeout=25):
            response = original(req, timeout)
            response.headers["X-OpenVietGIS-Feature-Count"] = "2"
            return response
        self.opener.open = changed_count
        with self.assertRaisesRegex(ReceiverError, "feature count"):
            receive_layer(self.c, self.layer, self.tmp.name, opener=self.opener)
        self.assertEqual(list(Path(self.tmp.name).iterdir()), [])

    def test_bad_hash_fail_closed(self):
        self.opener.override_hash = True
        with self.assertRaisesRegex(ReceiverError, "SHA-256"):
            receive_layer(self.c, self.layer, self.tmp.name, opener=self.opener)
        self.assertEqual(list(Path(self.tmp.name).iterdir()), [])

    def test_must_use_https_origin_no_credentials_in_url(self):
        for base in ["http://openvietgis.cloud", "https://user:pass@host", "https://host/path", "https://host?key=x"]:
            with self.subTest(base=base):
                with self.assertRaises(ReceiverError):
                    receive_layer(Connection(base, self.c.organization_id, self.c.project_id, "secret"), self.layer, self.tmp.name, opener=self.opener)
        self.assertFalse(self.opener.calls)

    def test_invalid_crs_or_layer(self):
        self.opener.override_crs = "EPSG:4326"
        with self.assertRaisesRegex(ReceiverError, "projected metre"):
            receive_layer(self.c, self.layer, self.tmp.name, opener=self.opener)
        with self.assertRaisesRegex(ReceiverError, "Layer must"):
            receive_layer(self.c, "not-a-uuid", self.tmp.name, opener=self.opener)

    def test_no_redirects_with_bearer(self):
        self.assertIsNone(_NoRedirect().redirect_request(None, None, 302, "Moved", {}, "https://evil.test"))

    def test_v8i_attach_only_with_fake_application(self):
        def attach(progid):
            self.assertEqual(progid, "MicroStationDGN.Application")
            return SimpleNamespace(ActiveDesignFile=SimpleNamespace(FullName=r"C:\CAD\trial.dgn"), ActiveModelReference=SimpleNamespace(Name="Default"))
        info = probe(attach)
        self.assertEqual(info["drawing"], "trial.dgn")
        self.assertEqual(info["model"], "Default")
        self.assertEqual(info["cad_write_back_enabled"], "false")

    def test_no_running_v8i_fail_closed(self):
        with self.assertRaises(ProbeError):
            probe(lambda key: (_ for _ in ()).throw(OSError("not running")))


if __name__ == "__main__":
    unittest.main()