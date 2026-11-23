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
    from qgis.core import QgsApplication

    from integrations.qgis_plugin.openvietgis_connector.plugin import (
        OpenVietGisConnectorDialog,
    )

    app = QgsApplication([], False)
    app.initQgis()
    try:
        dialog = OpenVietGisConnectorDialog()
        assert dialog.ws_create_button.text().startswith("1.")
        assert dialog.ws_preview_button.text().startswith("2.")
        assert dialog.ws_commit_button.isEnabled() is False
        assert dialog.ws_recover_button.text()
        assert dialog.bearer_key_input.echoMode() == dialog.bearer_key_input.Password
        assert dialog._workspace_preview is None
        dialog.close()
        print("PASS PyQGIS native plugin window: manual sync controls and default deny")
    finally:
        app.exitQgis()


if __name__ == "__main__":
    main()
