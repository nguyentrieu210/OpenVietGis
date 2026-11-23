"""R028 fake-V8i COM with real R026 GPKG writer, no live CAD/cloud writes."""
from __future__ import annotations

import hashlib
import os
import json
from unittest.mock import patch
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from uuid import uuid4

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from outbound import (  # noqa: E402
    CadSyncError, _read_geo, _write_geo, receipt, bind_selected, draft_outbound,
    MAP_FILE, OUT_FILE, JOURNAL_FILE,
)
from sync_transport import preview_cloud, commit_approved  # noqa: E402
from receiver import Connection  # noqa: E402

from openvietgis_api.adapters.formats.geopackage import write_geopackage
from openvietgis_api.domains.spatial.domain import FieldType, NativeGeometry
from openvietgis_api.domains.transfers.domain import ExportFieldMapping


class Enumerator:
    def __init__(self, entities):
        self.data = iter(entities)
        self.Current = None

    def MoveNext(self):
        self.Current = next(self.data, None)
        return self.Current is not None


class CadLine:
    Type = 3

    def __init__(self, eid, coords):
        self.ID = SimpleNamespace(High=0, Low=eid)
        self.AsLineElement = SimpleNamespace(
            StartPoint=SimpleNamespace(X=coords[0][0], Y=coords[0][1]),
            EndPoint=SimpleNamespace(X=coords[1][0], Y=coords[1][1]),
        )


class FakeApp:
    def __init__(self):
        self.line = CadLine(44, [[1., 2.], [3., 4.]])
        self.ActiveDesignFile = SimpleNamespace(FullName=r"C:\Cad\sample.dgn")
        self.ActiveModelReference = SimpleNamespace(
            Name="Default", GetSelectedElements=lambda: Enumerator([self.line]),
            Scan=lambda: Enumerator([self.line]),
        )


class MockResponse:
    def __init__(self, url, status, body):
        self.url, self.status, self.body = url, status, json.dumps(body).encode()

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False

    def geturl(self):
        return self.url

    def getcode(self):
        return self.status

    def read(self, size):
        return self.body[:size]


class Server:
    def __init__(self, fixture):
        self.fix = fixture
        self.calls = []
        self.fail_push = False
        self.change_preview = False

    def open(self, req, timeout=45):
        self.calls.append(req)
        if req.full_url.endswith("/sync-preview"):
            return MockResponse(req.full_url, 200, {
                "layer_id": self.fix.layer,
                "base_content_revision": "a"*64,
                "current_content_revision": "a"*64,
                "base_schema_revision": "b"*64,
                "current_schema_revision": "b"*64,
                "stored_crs_code": 5899,
                "stale_revision": False, "deletions_blocked": True,
                "can_commit": True, "create_count": 0,
                "update_count": 1 if not self.change_preview else 2,
                "omitted_server_feature_count": 0,
                "diff_sha256": "e"*64,
                "operations": [{"feature_id": "A-001", "op_type": "update", "geometry_changed": True, "changed_fields": []}],
            })
        if self.fail_push:
            raise TimeoutError("after server commit")
        return MockResponse(req.full_url, 201, {
            "operation_id": next(part for part in req.data.decode("latin1").split("\r\n")
                                 if len(part) == 36 and part.count("-") == 4),
            "diff_sha256": "e"*64,
            "operation_count": 1,
            "layer_id": self.fix.layer,
            "deletions_blocked": True,
            "changeset_id": str(uuid4()),
            "resulting_content_revision": "f"*64,
        })


class TestOutbound(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.layer = str(uuid4())
        self.app = FakeApp()
        self.conn = Connection("https://openvietgis.cloud",str(uuid4()),str(uuid4()),"TEMP_CREDENTIAL")
        features = [
            ("A-001", {"name": "Parcel A"}, NativeGeometry("LineString", ([1.0,2.0],[3.0,4.0]),5899)),
        ]
        gpkg, *_ = write_geopackage(
            layer_name="test_parcel", crs_code=5899, geometry_family="LineString",
            field_mappings=[ExportFieldMapping("name", "name", "name", FieldType.TEXT)],
            features=features
        )
        dxf = b"  0\nSECTION\n  2\nHEADER\n  0\nENDSEC\n  0\nEOF\n"
        (self.root/"attributes.gpkg").write_bytes(gpkg)
        (self.root/"geometry.dxf").write_bytes(dxf)
        (self.root/"receipt.json").write_text(json.dumps({
            "schema_version":"ovg.windows.inbound.v1", "cad_write_back_enabled":False,
            "layer_id":self.layer, "organization_id":self.conn.organization_id,
            "project_id":self.conn.project_id, "server_origin":self.conn.server,
            "content_revision":"a"*64, "schema_revision":"b"*64,
            "attributes_geopackage_sha256":hashlib.sha256(gpkg).hexdigest(),
            "geometry_dxf_sha256":hashlib.sha256(dxf).hexdigest(),
            "crs":"EPSG:5899", "feature_count":1,
        }))

    def bind(self):
        return bind_selected(self.root, "A-001", verified_crs_units=True, app=self.app)

    def changed(self):
        self.bind()
        self.app.line.AsLineElement.EndPoint.X = 5.0
        return draft_outbound(self.root, app=self.app)

    def test_real_r026_geopackage_reader_round_trip_and_identity(self):
        receipt(self.root)
        self.assertEqual(self.bind()["feature_id"], "A-001")
        mapping = json.loads((self.root/MAP_FILE).read_text())
        self.assertEqual(mapping["bindings"]["A-001"]["cad_element_id"], "00000000:0000002c")
        self.assertEqual(_read_geo(_write_geo({"type":"LineString", "coordinates":[[1.,2.],[3.,4.]]},5899))["type"], "LineString")

    def test_unverified_crs_and_unmatched_geometry_rejected(self):
        with self.assertRaisesRegex(CadSyncError, "master units"):
            bind_selected(self.root, "A-001", verified_crs_units=False, app=self.app)
        self.app.line.AsLineElement.StartPoint.X = 3.1
        with self.assertRaisesRegex(CadSyncError, "differs"):
            self.bind()

    def test_outbound_draft_changes_mapped_geometry_only_and_keeps_baseline(self):
        before = (self.root/"attributes.gpkg").read_bytes()
        result = self.changed()
        self.assertEqual(result["changes"],1)
        self.assertEqual((self.root/"attributes.gpkg").read_bytes(),before)
        self.assertTrue((self.root/OUT_FILE).is_file())
        with sqlite3.connect(self.root/OUT_FILE) as db:
            blob = db.execute("SELECT geom FROM test_parcel WHERE ovg_feature_id='A-001'").fetchone()[0]
            self.assertEqual(_read_geo(blob)["coordinates"][-1], [5.,4.])
            bounds = db.execute("SELECT max_x FROM gpkg_contents WHERE table_name='test_parcel'").fetchone()[0]
            self.assertEqual(bounds, 5.)
        with self.assertRaisesRegex(CadSyncError,"Existing outbound draft"):
            draft_outbound(self.root,app=self.app)

    def test_removed_cad_entity_is_not_automatic_delete(self):
        self.bind()
        self.app.ActiveModelReference.Scan=lambda:Enumerator([])
        with self.assertRaisesRegex(CadSyncError,"missing"):
            draft_outbound(self.root,app=self.app)

    def test_cannot_mutate_snapshot_or_mapping(self):
        self.bind()
        (self.root/"attributes.gpkg").write_bytes(b"tampered")
        with self.assertRaisesRegex(CadSyncError,"changed after download"):
            draft_outbound(self.root,app=self.app)

    def test_cloud_manual_preview_requires_exact_mapped_ops(self):
        self.changed()
        s=Server(self)
        result=preview_cloud(self.conn,self.root,opener=s)
        self.assertEqual(result["update_count"],1)
        self.assertFalse((self.root/JOURNAL_FILE).exists())
        self.assertEqual(len(s.calls),1)
        s.change_preview=True
        with self.assertRaisesRegex(CadSyncError,"Server diff does not match"):
            preview_cloud(self.conn,self.root,opener=s)

    def test_approved_cloud_commit_success_once_with_operation_ack(self):
        self.changed()
        server = Server(self)
        preview = preview_cloud(self.conn, self.root, opener=server)
        with patch.dict(os.environ, {"OVG_V8I_WRITEBACK_PILOT": "ALLOW_EXPLICIT_TEST_WRITE",
                                       "OVG_V8I_TEST_PROJECT_ID": self.conn.project_id}):
            response = commit_approved(self.conn, self.root, preview["diff_sha256"], opener=server)
        self.assertEqual(response["state"], "acknowledged")
        self.assertEqual(response["update_count"], 1)
        self.assertEqual(len(server.calls), 3)  # initial preview, fresh preflight, single commit
        journal = json.loads((self.root/JOURNAL_FILE).read_text())
        self.assertEqual(journal["operation_id"], response["operation_id"])
        self.assertEqual(journal["state"], "acknowledged")
        with patch.dict(os.environ, {"OVG_V8I_WRITEBACK_PILOT": "ALLOW_EXPLICIT_TEST_WRITE",
                                       "OVG_V8I_TEST_PROJECT_ID": self.conn.project_id}):
            with self.assertRaisesRegex(CadSyncError, "journal exists"):
                commit_approved(self.conn,self.root,preview["diff_sha256"],opener=server)

    def test_stale_cloud_diff_never_issues_a_push(self):
        self.changed()
        server = Server(self)
        preview = preview_cloud(self.conn,self.root,opener=server)
        server.change_preview = True
        with patch.dict(os.environ, {"OVG_V8I_WRITEBACK_PILOT": "ALLOW_EXPLICIT_TEST_WRITE",
                                       "OVG_V8I_TEST_PROJECT_ID": self.conn.project_id}):
            with self.assertRaisesRegex(CadSyncError, "Server diff does not match"):
                commit_approved(self.conn,self.root,preview["diff_sha256"],opener=server)
        self.assertEqual(len(server.calls),2)
        self.assertFalse((self.root/JOURNAL_FILE).exists())

    def test_writeback_disabled_without_explicit_pilot_allowance(self):
        self.changed()
        server = Server(self)
        p = preview_cloud(self.conn, self.root, opener=server)
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaisesRegex(CadSyncError, "disabled"):
                commit_approved(self.conn, self.root, p["diff_sha256"], opener=server)
        self.assertEqual(len(server.calls), 1)
        self.assertFalse((self.root/JOURNAL_FILE).exists())

    def test_push_requires_confirmed_sha_and_never_retries_on_uncertain_timeout(self):
        self.changed()
        server=Server(self)
        p=preview_cloud(self.conn,self.root,opener=server)
        with patch.dict(os.environ, {"OVG_V8I_WRITEBACK_PILOT": "ALLOW_EXPLICIT_TEST_WRITE",
                                       "OVG_V8I_TEST_PROJECT_ID": self.conn.project_id}):
            with self.assertRaisesRegex(CadSyncError,"Human confirmation"):
                commit_approved(self.conn,self.root,"0"*64,opener=server)
            self.assertEqual(len(server.calls),1)
            server.fail_push=True
            with self.assertRaisesRegex(CadSyncError,"Server request failed"):
                commit_approved(self.conn,self.root,p["diff_sha256"],opener=server)
        journal=json.loads((self.root/JOURNAL_FILE).read_text())
        self.assertEqual(journal["state"],"sent_unconfirmed")
        with patch.dict(os.environ, {"OVG_V8I_WRITEBACK_PILOT": "ALLOW_EXPLICIT_TEST_WRITE",
                                       "OVG_V8I_TEST_PROJECT_ID": self.conn.project_id}):
            with self.assertRaisesRegex(CadSyncError,"journal exists"):
                commit_approved(self.conn,self.root,p["diff_sha256"],opener=server)


if __name__ == '__main__':
    unittest.main()