"""Native headless PyQGIS smoke: verify R027 widget loads without web or GIS writes.

Run with the QGIS system Python (usually /usr/bin/python3) after installing
QGIS 3.34+ and PyQt bindings. No server credentials or project data needed.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

def main() -> None:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from threading import Event
    from time import perf_counter, sleep
    from types import SimpleNamespace
    from unittest.mock import patch

    from qgis.core import (
        QgsApplication,
        QgsFeature,
        QgsField,
        QgsProject,
        QgsVectorLayer,
    )
    from qgis.PyQt.QtCore import QVariant
    from qgis.PyQt.QtWidgets import QMessageBox

    from integrations.qgis_plugin.openvietgis_connector.plugin import (
        OpenVietGisConnectorDialog,
    )

    app = QgsApplication([], False, os.environ.get("OVG_QGIS_QA_PROFILE", ""))
    app.initQgis()
    try:
        dialog = OpenVietGisConnectorDialog()
        assert dialog.r029_open_button.text() == "Mở bản đồ trong QGIS"
        assert dialog.r029_save_button.text() == "Lưu thay đổi lên Web"
        assert not dialog.r029_save_button.isEnabled()
        assert not dialog.managed_auto_save.isChecked()
        assert dialog.browser_login_button.isEnabled()
        assert dialog.ws_commit_button.isEnabled() is False
        assert dialog.ws_recover_button.text()
        assert dialog.bearer_key_input.echoMode() == dialog.bearer_key_input.Password
        assert dialog._workspace_preview is None
        assert dialog.connection_summary.isHidden()
        assert not dialog.browser_login_button.isHidden()
        assert dialog.layers_group.isHidden()
        dialog._set_connection_state(True)
        assert not dialog.connection_summary.isHidden()
        assert dialog.browser_login_button.isHidden()
        assert dialog.pairing_box.isHidden()
        assert dialog.manual_conn_toggle.isHidden()
        assert not dialog.switch_project_button.isHidden()
        assert not dialog.logout_session_button.isHidden()
        assert not dialog.layers_group.isHidden()
        dialog._set_connection_state(False)
        assert not dialog.browser_login_button.isHidden()
        assert dialog.logout_session_button.isHidden()
        layer = QgsVectorLayer("Point?crs=EPSG:3857", "benchmark", "memory")
        provider = layer.dataProvider()
        assert provider.addAttributes([QgsField("ovg_feature_id", QVariant.String),
                                       QgsField("fld_name", QVariant.String)])
        layer.updateFields()
        started = perf_counter()
        features = []
        for i in range(8000):
            feature = QgsFeature(layer.fields())
            feature.setGeometry(dialog._geojson_to_qgs_geometry({
                "type": "Point", "coordinates": [11800000 + i, 1200000],
            }))
            feature.setAttributes([str(i), "Sample"])
            features.append(feature)
        assert provider.addFeatures(features)[0]
        assert layer.featureCount() == 8000
        info = {"field_names": ["fld_name"],
                "baseline": dialog._managed_snapshot(layer, ["fld_name"])}
        dialog._track_managed_layer(layer, info)
        with patch.object(dialog, "_managed_snapshot", wraps=dialog._managed_snapshot) as snapshot:
            for _ in range(20):
                assert not dialog._managed_is_dirty(layer, info)
            assert snapshot.call_count == 0
            assert layer.startEditing()
            fid = next(layer.getFeatures()).id()
            assert layer.changeAttributeValue(fid, 1, "Edited")
            assert dialog._managed_is_dirty(layer, info)
            assert layer.commitChanges()
            assert dialog._managed_is_dirty(layer, info)
        print(f"Native 8000 points build + snapshot + native edit: {perf_counter()-started:.2f}s")
        # Actual OGR GeoPackage provider: persistent native edits, no provider FID leakage.
        from tempfile import TemporaryDirectory
        with TemporaryDirectory(ignore_cleanup_errors=True) as folder:
            disk = dialog._persistent_layer(layer, Path(folder))
            original = dialog._managed_snapshot(disk, ["fld_name"])
            assert original == dialog._managed_snapshot(layer, ["fld_name"])
            assert disk.startEditing()
            fid_disk = next(disk.getFeatures()).id()
            feature_id = str(disk.getFeature(fid_disk)["ovg_feature_id"])
            assert disk.changeAttributeValue(fid_disk, disk.fields().indexFromName("fld_name"), "Offline edit")
            assert disk.commitChanges()
            uri = disk.source()
            disk = None
            disk = QgsVectorLayer(uri, "reopen", "ogr")
            assert disk.isValid()
            restored = dialog._managed_snapshot(disk, ["fld_name"])
            assert restored[feature_id]["properties"] == {"fld_name": "Offline edit"}
            assert len(restored) == 8000
            assert disk.startEditing()
            extra = dialog._managed_feature(disk, "new-offline", {"fld_name": "New"},
                dialog._geojson_to_qgs_geometry({"type":"Point","coordinates":[11800000,1200000]}), ["fld_name"])
            assert disk.addFeature(extra)
            assert disk.commitChanges()
            assert "new-offline" in dialog._managed_snapshot(disk, ["fld_name"])
            # A native Save must journal first, then schedule the network action.
            native_info = {"baseline": dialog._managed_snapshot(disk, ["fld_name"]),
                           "field_names": ["fld_name"]}
            dialog._managed_layers["native-test"] = native_info
            dialog._managed_config = object()
            dialog._wire_native_save(disk, native_info, "native-test")
            with patch.object(dialog, "_write_managed_draft") as journal, patch.object(
                dialog, "_on_commit_r029_managed"
            ) as send:
                assert disk.startEditing()
                native_fid = next(disk.getFeatures()).id()
                assert disk.changeAttributeValue(native_fid, disk.fields().indexFromName("fld_name"), "Native Save")
                assert disk.commitChanges()
                app.processEvents()
                assert journal.call_count == 1
                assert send.call_count == 1
            disk.afterCommitChanges.disconnect()
            dialog._managed_layers.pop("native-test")
            dialog._managed_config = None
            disk = None
        print("PASS GeoPackage actual OGR provider: reopen native edit and create; no FID leakage")

        # Real Qt task scheduling, with isolated mock transport (no production writes).
        QgsProject.instance().addMapLayer(layer)
        info.update({"map_layer_id": layer.id(), "source_id": "s",
                     "content_revision": "a" * 64, "schema_revision": "b" * 64,
                     "cursor": 0, "permission_epoch": 1})
        dialog._managed_layers["l"] = info
        dialog._managed_session = SimpleNamespace(id="test", can_edit=True)
        dialog._managed_config = object()
        release = Event()
        def mock_send(*args, **kwargs):
            assert release.wait(5)
            return SimpleNamespace(resulting_content_revision="c" * 64,
                                   event_cursor=1, operation_count=1)
        with patch.object(dialog, "_write_managed_draft"), patch.object(
            dialog._managed_delta_debounce, "start"
        ), patch(
            "integrations.qgis_plugin.openvietgis_connector.plugin.send_pending_commit",
            side_effect=mock_send,
        ):
            dialog._on_commit_r029_managed()
            assert dialog._commit_task is not None
            assert layer.startEditing()
            assert layer.changeAttributeValue(fid, 1, "Edited while saving")
            release.set()
            until = perf_counter() + 10
            while dialog._commit_task is not None and perf_counter() < until:
                app.processEvents()
                sleep(.02)
            assert dialog._commit_task is None
            assert info["baseline"][str(layer.getFeature(fid)["ovg_feature_id"])]["properties"]["fld_name"] == "Edited"
            assert layer.getFeature(fid)["fld_name"] == "Edited while saving"
            assert dialog._managed_is_dirty(layer, info)
        # A dirty layer must not block a clean layer, or lose its own deferred cursor.
        clean = QgsVectorLayer("Point?crs=EPSG:3857", "clean layer", "memory")
        assert clean.dataProvider().addAttributes([QgsField("ovg_feature_id", QVariant.String),
                                                   QgsField("fld_name", QVariant.String)])
        clean.updateFields()
        QgsProject.instance().addMapLayer(clean)
        clean_info = {"map_layer_id": clean.id(), "source_id": "s2", "field_names": ["fld_name"],
                      "baseline": {}, "cursor": 0, "content_revision": "a" * 64}
        dialog._track_managed_layer(clean, clean_info)
        dialog._managed_layers["clean"] = clean_info
        dialog._managed_cursor = 1
        delta = SimpleNamespace(reset_required=False, next_cursor=3, has_more=False, changes=[
            {"layer_id": "l", "cursor": 2, "content_revision": "d" * 64,
             "upserts": [], "removals": [str(layer.getFeature(fid)["ovg_feature_id"])]},
            {"layer_id": "clean", "cursor": 3, "content_revision": "e" * 64,
             "removals": [], "upserts": [{"feature_id": "new",
               "geometry": {"type": "Point", "coordinates": [11800000, 1200000]},
               "properties": {"fld_name": "Remote"}}]},
        ])
        dialog._apply_managed_delta(delta)
        assert clean.featureCount() == 1
        assert clean_info["cursor"] == 3
        assert dialog._managed_cursor == 1
        assert layer.getFeature(fid)["fld_name"] == "Edited while saving"
        dialog._apply_managed_delta(delta)
        assert clean.featureCount() == 1
        assert dialog._managed_cursor == 1
        dialog._managed_layers.pop("clean")
        QgsProject.instance().removeMapLayer(clean.id())
        print("PASS native delta: clean layer updates, dirty layer and cursor preserved")
        # A retry tick can submit only an existing pending request, not unrelated edits.
        import time
        info["pending"] = {"operation_id":"retry-test"}
        info["retry_at"] = time.monotonic() - 1
        with patch.object(dialog, "_checkpoint_managed_drafts"), patch.object(
            dialog, "_on_commit_r029_managed"
        ) as retry:
            dialog._managed_tick()
            retry.assert_called_once_with(automatic=True, retry_only=True)
        info.pop("pending")
        info.pop("retry_at")
        print("PASS retry tick isolates previously approved transactions")

        # Native delete: auto-save never deletes; manual Save prompts once.
        assert layer.deleteFeature(fid)
        with patch.object(dialog, "_write_managed_draft"), patch.object(
            dialog, "_start_commit_task"
        ) as start, patch.object(QMessageBox, "question", return_value=QMessageBox.No) as ask:
            dialog._on_commit_r029_managed(automatic=True)
            assert start.call_count == 0
            assert ask.call_count == 0
            dialog._on_commit_r029_managed()
            assert start.call_count == 0
            assert ask.call_count == 1
            ask.return_value = QMessageBox.Yes
            dialog._on_commit_r029_managed()
            assert start.call_count == 1
            pending = start.call_args.args[3]
            assert pending["request"]["allow_deletes"] is True
            assert pending["request"]["operations"][0]["op_type"] == "delete"
            assert ask.call_count == 2
        print("PASS native delete: auto-save defers, manual cancel preserves, confirm submits")
        dialog._managed_layers.clear()
        dialog._managed_config = None
        dialog._managed_session = None
        QgsProject.instance().removeMapLayer(layer.id())
        print("PASS native background commit preserves edit made while sending")
        dialog.close()
        print("PASS PyQGIS native plugin window: primary controls, browser login and default deny")
    finally:
        app.exitQgis()


if __name__ == "__main__":
    main()
