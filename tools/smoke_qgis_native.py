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
    from time import perf_counter
    from unittest.mock import patch

    from qgis.core import QgsApplication, QgsFeature, QgsField, QgsVectorLayer
    from qgis.PyQt.QtCore import QVariant

    from integrations.qgis_plugin.openvietgis_connector.plugin import (
        OpenVietGisConnectorDialog,
    )

    app = QgsApplication([], False)
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
        dialog.close()
        print("PASS PyQGIS native plugin window: primary controls, browser login and default deny")
    finally:
        app.exitQgis()


if __name__ == "__main__":
    main()
