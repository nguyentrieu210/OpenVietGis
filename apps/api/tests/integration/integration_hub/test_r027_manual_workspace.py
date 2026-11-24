"""R027 manual bilateral workspace via real R006/R026 HTTP + real disposable PostGIS.

Transport adapter uses a FastAPI TestClient human session; real desktop
QGIS and bearer-credential network transport require separate acceptance.
"""
from __future__ import annotations

import hashlib
import sqlite3
from pathlib import Path
from types import SimpleNamespace
from uuid import uuid4

from integrations.qgis_plugin.openvietgis_connector import workspace as sync
from integrations.qgis_plugin.openvietgis_connector.client import OpenVietGisConfig

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


def test_qgis_workspace_manual_roundtrip_real_http_postgis(
    migrated_database: None, tmp_path: Path, monkeypatch
) -> None:
    world = seed_access_world()
    root = (
        f"/api/v1/organizations/{world.organization_id}"
        f"/projects/{world.project_id}"
    )
    config = OpenVietGisConfig(
        base_url="https://gis.example.invalid",
        organization_id=str(world.organization_id),
        project_id=str(world.project_id),
        bearer_token="never-sent-by-fixture-only",
    )

    with _client(world.owner_user_id) as owner:
        create = owner.post(
            f"{root}/layers/manual",
            json={
                "operation_id": str(uuid4()), "name": "R027 Roundtrip",
                "geometry_family": "Point",
                "fields": [
                    {"field_id": "fld_name", "name": "Name", "type": "text", "nullable": False},
                    {"field_id": "fld_note", "name": "Note", "type": "text", "nullable": True},
                ],
            },
        )
        assert create.status_code == 201, create.text
        layer_id, source_id = (
            create.json()["layer_id"], create.json()["source_id"]
        )
        details = owner.get(f"{root}/layers/{layer_id}")
        assert details.status_code == 200
        source = details.json()["source"]
        seed = owner.post(
            f"{root}/sources/{source_id}/changesets",
            json={
                "operation_id": str(uuid4()),
                "base_content_revision": source["content_revision"],
                "base_schema_revision": source["schema_revision"],
                "summary": "R027 baseline seed",
                "operations": [
                    {
                        "op_type": "create", "feature_id": f"parcel-{i}",
                        "geometry": {
                            "type": "Point", "coordinates": [106.6 + i * .01, 10.7]
                        },
                        "properties": {"fld_name": f"Parcel {i}", "fld_note": "Common"}
                    }
                    for i in (1, 2)
                ],
            },
        )
        assert seed.status_code == 201, seed.text
        export_url = (
            f"{root}/integration-hub/adapters/qgis/layers/{layer_id}"
            "/export?format=geopackage"
        )
        preview_url = f"{root}/integration-hub/adapters/qgis/layers/{layer_id}/sync-preview"
        push_url = f"{root}/integration-hub/adapters/qgis/layers/{layer_id}/sync-push"

        def download(_config, _layer_id, destination):
            assert _layer_id == layer_id
            res = owner.get(export_url)
            assert res.status_code == 200, res.text
            output = Path(destination) / "roundtrip.gpkg"
            output.write_bytes(res.content)
            return SimpleNamespace(
                path=output,
                sha256=res.headers["x-content-sha256"],
                content_revision=res.headers["x-openvietgis-content-revision"],
                schema_revision=res.headers["x-openvietgis-schema-revision"],
                crs=res.headers["x-openvietgis-crs"],
                feature_count=int(res.headers["x-openvietgis-feature-count"]),
            )

        def preview(_config, *, gpkg_path, base_content_revision, base_schema_revision,
                    allow_deletes, **extra):
            res = owner.post(
                preview_url,
                data={
                    "base_content_revision": base_content_revision,
                    "base_schema_revision": base_schema_revision,
                    "allow_deletes": str(allow_deletes).lower(),
                },
                files={"file": ("edited.gpkg", Path(gpkg_path).read_bytes(),
                                "application/geopackage+sqlite3")},
            )
            assert res.status_code == 200, res.text
            return SimpleNamespace(**res.json())

        def push(_config, *, layer_id, gpkg_path, base_content_revision, base_schema_revision,
                 expected_diff_sha256, operation_id, summary, allow_deletes):
            assert layer_id == create.json()["layer_id"]
            res = owner.post(
                push_url,
                data={
                    "base_content_revision": base_content_revision,
                    "base_schema_revision": base_schema_revision,
                    "allow_deletes": str(allow_deletes).lower(),
                    "expected_diff_sha256": expected_diff_sha256,
                    "operation_id": operation_id,
                    "summary": summary,
                },
                files={"file": ("edited.gpkg", Path(gpkg_path).read_bytes(),
                                "application/geopackage+sqlite3")},
            )
            assert res.status_code == 201, res.text
            return SimpleNamespace(**res.json())

        monkeypatch.setattr(sync, "download_layer_geopackage", download)
        monkeypatch.setattr(sync, "preview_layer_geopackage_sync", preview)
        monkeypatch.setattr(sync, "push_layer_geopackage_sync", push)

        workspace = sync.initialize_workspace(config, layer_id, tmp_path)
        baseline = sync.read_dataset(workspace / "baseline.gpkg")
        assert len(baseline.rows) == 2
        with sqlite3.connect(workspace / "working.gpkg") as conn:
            conn.execute(
                "UPDATE r027_roundtrip SET fld_name='QGIS changed' "
                "WHERE ovg_feature_id='parcel-1'"
            )
        # Concurrent server edit of a different field, via real R006.
        src_new = owner.get(f"{root}/layers/{layer_id}").json()["source"]
        remote_edit = owner.post(
            f"{root}/sources/{source_id}/changesets",
            json={
                "operation_id": str(uuid4()),
                "base_content_revision": src_new["content_revision"],
                "base_schema_revision": src_new["schema_revision"],
                "summary": "Independent OpenVietGIS edit",
                "operations": [
                    {
                        "op_type": "update", "feature_id": "parcel-1",
                        "geometry": {"type": "Point", "coordinates": [106.61, 10.7]},
                        "properties": {
                            "fld_name": "Parcel 1", "fld_note": "OVG changed"
                        },
                    }
                ],
            },
        )
        assert remote_edit.status_code == 201, remote_edit.text

        plan = sync.preview_workspace(config, workspace)
        assert plan.conflicts == ()
        assert len(plan.to_local) == 1
        assert len(plan.to_server) == 1
        receipt = sync.commit_workspace(config, plan, confirmed_digest=plan.digest)
        assert receipt["pulled"] == receipt["pushed"] == 1
        assert receipt["changeset_id"] is not None
        merged = sync.read_dataset(workspace / "working.gpkg")
        assert merged.rows["parcel-1"].attrs["fld_name"] == "QGIS changed"
        assert merged.rows["parcel-1"].attrs["fld_note"] == "OVG changed"
        assert sync.read_dataset(workspace / "baseline.gpkg").rows == merged.rows
        # Audit-backed server export really reflects both changes.
        after = owner.get(export_url)
        assert after.status_code == 200
        assert after.headers["X-Content-SHA256"] == hashlib.sha256(after.content).hexdigest()
        second_plan = sync.preview_workspace(config, workspace)
        assert not second_plan.to_local and not second_plan.to_server
