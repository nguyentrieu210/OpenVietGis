"""R027 QGIS desktop-independent manual two-way workspace acceptance fixtures.

The mocked wire layer verifies local GPKG/three-way merge/changeset handoff.
Only an actual QGIS desktop + PostGIS run can validate full external integration.
"""
from __future__ import annotations

import hashlib
import json
import shutil
import sqlite3
from pathlib import Path
from types import SimpleNamespace
from uuid import uuid4

import pytest
from integrations.qgis_plugin.openvietgis_connector import workspace as sync
from integrations.qgis_plugin.openvietgis_connector.client import ApiError, OpenVietGisConfig

from openvietgis_api.adapters.formats.geopackage import write_geopackage
from openvietgis_api.domains.spatial.domain import FieldType, NativeGeometry
from openvietgis_api.domains.transfers.domain import ExportFieldMapping

LAYER_ID = str(uuid4())
REV_A = "a" * 64
REV_B = "b" * 64
REV_C = "c" * 64
SCHEMA = "d" * 64


def gpkg(
    path: Path, *,
    a_name: str = "Old", a_rank: int = 1, a_x: float = 1.0,
    b_name: str = "B", b_x: float = 3.0, include_b: bool = True,
) -> Path:
    features = [
        ("A-001", {"name": a_name, "rank": a_rank},
         NativeGeometry("Point", (a_x, 2.0), 5899))
    ]
    if include_b:
        features.append(
            ("B-002", {"name": b_name, "rank": 7},
             NativeGeometry("Point", (b_x, 4.0), 5899))
        )
    payload, *_ = write_geopackage(
        layer_name="parcels", crs_code=5899, geometry_family="Point",
        field_mappings=[
            ExportFieldMapping("name", "Name", "name", FieldType.TEXT),
            ExportFieldMapping("rank", "Rank", "rank", FieldType.INTEGER),
        ],
        features=features,
    )
    path.write_bytes(payload)
    return path


@pytest.fixture
def config():
    return OpenVietGisConfig(
        base_url="https://gis.example.test",
        organization_id=str(uuid4()),
        project_id=str(uuid4()),
        bearer_token="SENSITIVE_TEST_BEARER",
    )


class MockExports:
    def __init__(self, current: Path, revision: str = REV_A):
        self.current = current
        self.revision = revision
        self.fail_push = False
        self.push_calls = []
        self.preview_calls = []

    def download(self, _config, _layer_id, dest):
        path = Path(dest) / "parcels-qgis.gpkg"
        shutil.copy2(self.current, path)
        return SimpleNamespace(
            path=path, sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
            crs="EPSG:5899", schema_revision=SCHEMA,
            content_revision=self.revision, feature_count=2,
        )

    def preview(self, _config, *, gpkg_path, **kwargs):
        merged = sync.read_dataset(Path(gpkg_path))
        remote = sync.read_dataset(self.current)
        count = sum(merged.rows[k].attrs != remote.rows[k].attrs or
                    merged.rows[k].canonical_geometry != remote.rows[k].canonical_geometry
                    for k in merged.rows if k in remote.rows)
        count += len(set(merged.rows) - set(remote.rows))
        self.preview_calls.append((count, kwargs))
        return SimpleNamespace(
            stale_revision=False, can_commit=count > 0, create_count=0,
            update_count=count, omitted_server_feature_count=0,
            diff_sha256="e" * 64
        )

    def push(self, _config, *, gpkg_path, operation_id, **kwargs):
        self.push_calls.append((operation_id, kwargs))
        if self.fail_push:
            raise ConnectionError("response timed out after sending changeset")
        shutil.copy2(gpkg_path, self.current)
        self.revision = REV_C
        return SimpleNamespace(
            operation_count=1, changeset_id=str(uuid4()),
            resulting_content_revision=self.revision
        )


@pytest.fixture
def assets(tmp_path, config, monkeypatch):
    original = gpkg(tmp_path / "server.gpkg")
    wire = MockExports(original)
    monkeypatch.setattr(sync, "download_layer_geopackage", wire.download)
    monkeypatch.setattr(sync, "preview_layer_geopackage_sync", wire.preview)
    monkeypatch.setattr(sync, "push_layer_geopackage_sync", wire.push)
    root = sync.initialize_workspace(config, LAYER_ID, tmp_path / "workspaces")
    return root, wire, config


def test_initialize_creates_separate_durable_baseline_and_editable_workfile(assets):
    root, _wire, config = assets
    manifest = json.loads((root / "workspace.json").read_text())
    assert manifest["layer_id"] == LAYER_ID
    assert manifest["content_revision"] == REV_A
    assert manifest["pending"] is None
    assert "bearer" not in (root / "workspace.json").read_text().lower()
    assert (root / "baseline.gpkg").read_bytes() == (root / "working.gpkg").read_bytes()
    with pytest.raises(sync.WorkspaceConflict, match="already exists"):
        sync.initialize_workspace(config, LAYER_ID, root.parent)


def test_non_conflicting_both_directions_merged_then_committed(assets, tmp_path):
    root, wire, config = assets
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute("UPDATE parcels SET name='QGIS' WHERE ovg_feature_id='A-001'")
    gpkg(wire.current, a_rank=9, b_x=10.0)
    wire.revision = REV_B

    preview = sync.preview_workspace(config, root)
    assert not preview.conflicts
    assert len(preview.to_local) == 2  # remote rank and B geometry
    assert len(preview.to_server) == 1  # QGIS name
    assert preview.to_local[0].changed_fields == ("rank",)
    assert preview.to_server[0].changed_fields == ("name",)
    receipt = sync.commit_workspace(config, preview, confirmed_digest=preview.digest)
    assert receipt["pushed"] == 1 and receipt["pulled"] == 2
    assert receipt["new_content_revision"] == REV_C
    assert wire.preview_calls and wire.push_calls
    assert wire.preview_calls[0][1]["base_content_revision"] == REV_B

    committed = sync.read_dataset(root / "working.gpkg")
    assert committed.rows["A-001"].attrs == {"name": "QGIS", "rank": 9}
    assert committed.rows["B-002"].canonical_geometry == (1, (10.0, 4.0))
    assert sync.read_dataset(wire.current).rows["A-001"].attrs == committed.rows["A-001"].attrs
    assert (root / "baseline.gpkg").read_bytes() == (root / "working.gpkg").read_bytes()
    assert json.loads((root / "workspace.json").read_text())["pending"] is None

    next_preview = sync.preview_workspace(config, root)
    assert not next_preview.to_local and not next_preview.to_server


def test_concurrent_edit_blocks_both_directions_before_writes(assets):
    root, wire, config = assets
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute("UPDATE parcels SET name='Local' WHERE ovg_feature_id='A-001'")
    gpkg(wire.current, a_name="Remote")
    wire.revision = REV_B
    preview = sync.preview_workspace(config, root)
    assert preview.conflicts and "concurrent" in preview.conflicts[0]
    with pytest.raises(sync.WorkspaceConflict, match="Conflicts must be resolved"):
        sync.commit_workspace(config, preview, confirmed_digest=preview.digest)
    assert not wire.push_calls
    assert sync.read_dataset(root / "working.gpkg").rows["A-001"].attrs["name"] == "Local"


def test_deleted_feature_quarantined_not_recreated(assets):
    root, wire, config = assets
    gpkg(wire.current, include_b=False)
    wire.revision = REV_B
    result = sync.preview_workspace(config, root)
    assert result.conflicts and "deletion blocked" in result.conflicts[0]
    assert not result.to_local and not result.to_server


def test_changed_local_file_and_wrong_confirmation_are_denied(assets):
    root, wire, config = assets
    gpkg(wire.current, a_rank=2)
    wire.revision = REV_B
    preview = sync.preview_workspace(config, root)
    with pytest.raises(sync.WorkspaceConflict, match="human confirmation"):
        sync.commit_workspace(config, preview, confirmed_digest="wrong")
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute("UPDATE parcels SET name='new after preview'")
    with pytest.raises(sync.WorkspaceConflict, match="changed since preview"):
        sync.commit_workspace(config, preview, confirmed_digest=preview.digest)
    assert not wire.push_calls


def test_remote_only_pull_does_not_require_server_write(assets):
    root, wire, config = assets
    gpkg(wire.current, a_rank=3)
    wire.revision = REV_B
    preview = sync.preview_workspace(config, root)
    assert len(preview.to_local) == 1 and len(preview.to_server) == 0
    receipt = sync.commit_workspace(config, preview, confirmed_digest=preview.digest)
    assert receipt["pulled"] == 1 and receipt["pushed"] == 0
    assert not wire.push_calls
    assert sync.read_dataset(root / "working.gpkg").rows["A-001"].attrs["rank"] == 3


def test_network_ack_loss_stops_new_sync_and_recovers_after_remote_convergence(assets):
    root, wire, config = assets
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute("UPDATE parcels SET name='From QGIS' WHERE ovg_feature_id='A-001'")
    preview = sync.preview_workspace(config, root)
    assert len(preview.to_server) == 1
    wire.fail_push = True
    with pytest.raises(ConnectionError):
        sync.commit_workspace(config, preview, confirmed_digest=preview.digest)
    with pytest.raises(sync.WorkspaceConflict, match="outcome uncertain"):
        sync.preview_workspace(config, root)
    # Simulate server accepted changeset, but the client did not receive the HTTP receipt.
    shutil.copy2(root / ".pending-merged.gpkg", wire.current)
    wire.revision = REV_C
    assert sync.recover_workspace(config, root) is True
    assert sync.read_dataset(root / "working.gpkg").rows["A-001"].attrs["name"] == "From QGIS"
    assert json.loads((root / "workspace.json").read_text())["pending"] is None


def test_gpkg_integrity_checks_blank_and_duplicate_ids_and_crs(assets):
    root, wire, config = assets
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute("UPDATE parcels SET ovg_feature_id='' WHERE ovg_feature_id='A-001'")
    with pytest.raises(sync.WorkspaceConflict, match="Blank/invalid"):
        sync.preview_workspace(config, root)
    shutil.copy2(root / "baseline.gpkg", root / "working.gpkg")
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute("UPDATE parcels SET ovg_feature_id='A-001' WHERE ovg_feature_id='B-002'")
    with pytest.raises(sync.WorkspaceConflict, match="Duplicate"):
        sync.preview_workspace(config, root)

def test_two_sided_geometry_conflict_and_no_push(assets):
    root, wire, config = assets
    local_alternative = gpkg(root / "local-geom.gpkg", a_x=5.0)
    gpkg(wire.current, a_x=6.0)
    with sqlite3.connect(local_alternative) as source:
        local_blob = source.execute(
            "SELECT geom FROM parcels WHERE ovg_feature_id='A-001'"
        ).fetchone()[0]
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute(
            "UPDATE parcels SET geom=? WHERE ovg_feature_id='A-001'", (local_blob,)
        )
    wire.revision = REV_B
    preview = sync.preview_workspace(config, root)
    assert preview.conflicts and "geometry" in preview.conflicts[0]
    assert not preview.to_local and not preview.to_server
    assert not wire.push_calls


def test_remote_revision_moved_after_preview_fails_closed(assets, monkeypatch):
    root, wire, config = assets
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute("UPDATE parcels SET name='QGIS' WHERE ovg_feature_id='A-001'")
    preview = sync.preview_workspace(config, root)

    def stale_preview(*args, **kwargs):
        return SimpleNamespace(
            stale_revision=True, can_commit=False, create_count=0,
            update_count=0, omitted_server_feature_count=0,
            diff_sha256="a" * 64
        )

    monkeypatch.setattr(sync, "preview_layer_geopackage_sync", stale_preview)
    with pytest.raises(sync.WorkspaceConflict, match="Remote changed"):
        sync.commit_workspace(config, preview, confirmed_digest=preview.digest)
    assert not wire.push_calls
    assert json.loads((root / "workspace.json").read_text())["pending"] is None


def test_unknown_response_does_not_silently_recover_wrong_remote(assets):
    root, wire, config = assets
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute("UPDATE parcels SET name='QGIS' WHERE ovg_feature_id='A-001'")
    preview = sync.preview_workspace(config, root)
    wire.fail_push = True
    with pytest.raises(ConnectionError):
        sync.commit_workspace(config, preview, confirmed_digest=preview.digest)
    # The server has NOT applied the write. Do not advance the baseline.
    with pytest.raises(sync.WorkspaceConflict, match="does not exactly match"):
        sync.recover_workspace(config, root)
    manifest = json.loads((root / "workspace.json").read_text())
    assert manifest["pending"]["operation_id"] == wire.push_calls[0][0]
    assert manifest["content_revision"] == REV_A


def test_recovery_rejects_another_tenant_even_with_valid_local_files(assets):
    root, wire, config = assets
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute("UPDATE parcels SET name='QGIS' WHERE ovg_feature_id='A-001'")
    preview = sync.preview_workspace(config, root)
    wire.fail_push = True
    with pytest.raises(ConnectionError):
        sync.commit_workspace(config, preview, confirmed_digest=preview.digest)
    outsider = OpenVietGisConfig(
        base_url=config.base_url, organization_id=str(uuid4()),
        project_id=config.project_id, bearer_token=config.bearer_token
    )
    with pytest.raises(sync.WorkspaceConflict, match="scope mismatch"):
        sync.recover_workspace(outsider, root)


def test_revision_extraction_rejects_invalid_geopackage_srid(assets):
    root, _wire, config = assets
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute("UPDATE gpkg_spatial_ref_sys SET organization='NOT_EPSG' WHERE srs_id=5899")
    with pytest.raises(sync.WorkspaceConflict, match="not verified EPSG"):
        sync.preview_workspace(config, root)


def test_http_403_sync_never_retries_and_verified_recovery_preserves_local(assets, monkeypatch):
    root, wire, config = assets
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute("UPDATE parcels SET name='QGIS unsent' WHERE ovg_feature_id='A-001'")
    before = sync.read_dataset(root / "working.gpkg")
    preview = sync.preview_workspace(config, root)
    calls = []

    def deny(*args, **kwargs):
        calls.append(kwargs["operation_id"])
        raise ApiError("HTTP 403: permission denied", status_code=403)

    monkeypatch.setattr(sync, "push_layer_geopackage_sync", deny)
    with pytest.raises(sync.WorkspaceConflict, match="HTTP 403"):
        sync.commit_workspace(config, preview, confirmed_digest=preview.digest)
    assert len(calls) == 1
    pending = json.loads((root / "workspace.json").read_text())["pending"]
    assert pending["definitive_denial"] is True
    with pytest.raises(sync.WorkspaceConflict, match="recover pending work"):
        sync.preview_workspace(config, root)
    assert sync.recover_workspace(config, root) is True
    assert json.loads((root / "workspace.json").read_text())["pending"] is None
    assert sync.read_dataset(root / "working.gpkg").rows == before.rows
    assert sync.read_dataset(root / "baseline.gpkg").rows["A-001"].attrs["name"] == "Old"
    result = sync.preview_workspace(config, root)
    assert len(result.to_server) == 1
    assert len(calls) == 1


def test_denied_pending_recovery_fails_closed_on_remote_change(assets, monkeypatch):
    root, wire, config = assets
    with sqlite3.connect(root / "working.gpkg") as conn:
        conn.execute("UPDATE parcels SET name='QGIS unsent' WHERE ovg_feature_id='A-001'")
    preview = sync.preview_workspace(config, root)

    def deny(*args, **kwargs):
        raise ApiError("HTTP 403: permission denied", status_code=403)

    monkeypatch.setattr(sync, "push_layer_geopackage_sync", deny)
    with pytest.raises(sync.WorkspaceConflict, match="HTTP 403"):
        sync.commit_workspace(config, preview, confirmed_digest=preview.digest)
    # A concurrent Web modification prevents automatic clearing of pending.
    gpkg(wire.current, a_name="Remote changed")
    wire.revision = REV_B
    with pytest.raises(sync.WorkspaceConflict, match="cannot be verified"):
        sync.recover_workspace(config, root)
    assert json.loads((root / "workspace.json").read_text())["pending"]
    assert sync.read_dataset(root / "working.gpkg").rows["A-001"].attrs["name"] == "QGIS unsent"