"""QGIS 3.28+ Desktop Read-Only Connector plugin UI for OpenVietGIS (R026).

Security boundaries:
- Strictly read-only: opens SHA-256 verified GeoPackage snapshots and marks loaded
  QGIS layers read-only (`layer.setReadOnly(True)`). Zero write/push/sync actions.
- Service Principal Bearer keys use password echo mode, remain in memory for the
  lifetime of the dialog only, and are NEVER saved to QGIS project files (.qgs/.qgz)
  or plaintext settings.
- Explicit CRS only: displays verified EPSG codes returned by OpenVietGIS and never
  guesses or mutates coordinate systems.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from qgis.core import QgsProject, QgsVectorLayer
from qgis.PyQt.QtCore import Qt
from qgis.PyQt.QtWidgets import (
    QAbstractItemView,
    QAction,
    QCheckBox,
    QDialog,
    QFileDialog,
    QFormLayout,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPushButton,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from .client import (
    OpenVietGisConfig,
    OpenVietGisError,
    QgisLayerInfo,
    download_layer_geopackage,
    fetch_qgis_profile,
)

PLUGIN_MENU_NAME = "&OpenVietGIS"
ACTION_LABEL = "OpenVietGIS Read-Only Connector"


class OpenVietGisConnectorDialog(QDialog):
    """Read-only dialog to inspect authorized project layers and load GeoPackages."""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("OpenVietGIS Read-Only Connector (R026)")
        self.setMinimumSize(760, 520)

        self._layers: list[QgisLayerInfo] = []
        self._build_ui()

    def _build_ui(self) -> None:
        root_layout = QVBoxLayout(self)

        notice_label = QLabel(
            "Chế độ CHỈ ĐỌC (default_write_policy = deny). API Key chỉ giữ trong bộ nhớ tạm "
            "và KHÔNG lưu vào tệp dự án QGIS (.qgs/.qgz). "
            "Read-only mode: Bearer keys stay in memory only and are never written to project files."
        )
        notice_label.setWordWrap(True)
        notice_label.setStyleSheet(
            "background-color: #f0f4f8; border: 1px solid #cbd5e1; "
            "padding: 8px; border-radius: 4px;"
        )
        root_layout.addWidget(notice_label)

        form = QFormLayout()

        self.server_url_input = QLineEdit()
        self.server_url_input.setPlaceholderText("https://gis.example.vn")
        form.addRow("Server URL (HTTPS):", self.server_url_input)

        self.allow_localhost_checkbox = QCheckBox(
            "Allow http://localhost or http://127.0.0.1 (local development only)"
        )
        self.allow_localhost_checkbox.setChecked(False)
        form.addRow("", self.allow_localhost_checkbox)

        self.org_id_input = QLineEdit()
        self.org_id_input.setPlaceholderText("xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx")
        form.addRow("Organization UUID:", self.org_id_input)

        self.project_id_input = QLineEdit()
        self.project_id_input.setPlaceholderText("xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx")
        form.addRow("Project UUID:", self.project_id_input)

        self.bearer_key_input = QLineEdit()
        self.bearer_key_input.setEchoMode(QLineEdit.Password)
        self.bearer_key_input.setPlaceholderText(
            "Service Principal Bearer Key (project.read — not persisted)"
        )
        form.addRow("Service Principal Bearer Key:", self.bearer_key_input)

        folder_row = QHBoxLayout()
        default_dir = str(Path.home() / "OpenVietGIS_Exports")
        self.download_dir_input = QLineEdit(default_dir)
        self.browse_dir_button = QPushButton("Browse...")
        self.browse_dir_button.clicked.connect(self._on_browse_dir)
        folder_row.addWidget(self.download_dir_input)
        folder_row.addWidget(self.browse_dir_button)
        form.addRow("Local GeoPackage Folder:", folder_row)

        root_layout.addLayout(form)

        actions_row = QHBoxLayout()
        self.load_layers_button = QPushButton("Tải danh sách lớp / Load Authorized Layers")
        self.load_layers_button.clicked.connect(self._on_load_layers)
        actions_row.addWidget(self.load_layers_button)
        actions_row.addStretch(1)
        root_layout.addLayout(actions_row)

        self.layers_table = QTableWidget(0, 4)
        self.layers_table.setHorizontalHeaderLabels(
            ["Name / Tên lớp", "Geometry", "CRS (Explicit)", "Export Status"]
        )
        self.layers_table.setSelectionBehavior(QAbstractItemView.SelectRows)
        self.layers_table.setSelectionMode(QAbstractItemView.SingleSelection)
        self.layers_table.setEditTriggers(QAbstractItemView.NoEditTriggers)
        self.layers_table.itemSelectionChanged.connect(self._update_download_button_state)
        header = self.layers_table.horizontalHeader()
        header.setSectionResizeMode(0, QHeaderView.Stretch)
        header.setSectionResizeMode(1, QHeaderView.ResizeToContents)
        header.setSectionResizeMode(2, QHeaderView.ResizeToContents)
        header.setSectionResizeMode(3, QHeaderView.ResizeToContents)
        root_layout.addWidget(self.layers_table)

        self.status_label = QLabel(
            "Ready. Enter connection details and load authorized layers."
        )
        self.status_label.setWordWrap(True)
        root_layout.addWidget(self.status_label)

        footer_row = QHBoxLayout()
        self.download_open_button = QPushButton("Tải & mở lớp vào QGIS (Chỉ đọc)")
        self.download_open_button.setEnabled(False)
        self.download_open_button.clicked.connect(self._on_download_and_open)

        self.close_button = QPushButton("Close / Đóng")
        self.close_button.clicked.connect(self.close)

        footer_row.addWidget(self.download_open_button)
        footer_row.addStretch(1)
        footer_row.addWidget(self.close_button)
        root_layout.addLayout(footer_row)

    def _build_config(self) -> OpenVietGisConfig:
        return OpenVietGisConfig(
            base_url=self.server_url_input.text(),
            organization_id=self.org_id_input.text(),
            project_id=self.project_id_input.text(),
            bearer_token=self.bearer_key_input.text(),
            allow_insecure_localhost=self.allow_localhost_checkbox.isChecked(),
        )

    def _on_browse_dir(self) -> None:
        selected = QFileDialog.getExistingDirectory(
            self,
            "Select Local GeoPackage Download Folder",
            self.download_dir_input.text() or str(Path.home()),
        )
        if selected:
            self.download_dir_input.setText(selected)

    def _on_load_layers(self) -> None:
        self.download_open_button.setEnabled(False)
        try:
            config = self._build_config()
            self.status_label.setText("Fetching read-only QGIS adapter profile...")
            profile = fetch_qgis_profile(config)
        except OpenVietGisError as exc:
            self.status_label.setText(f"Error: {exc}")
            QMessageBox.warning(self, "OpenVietGIS Connector", str(exc))
            return

        self._layers = list(profile.layers)
        self._populate_layers_table()
        scoped_note = "scoped ACL" if profile.scoped_access else "project role"
        self.status_label.setText(
            f"Loaded {len(self._layers)} authorized layer(s) "
            f"[policy={profile.default_write_policy}, status={profile.execution_status}, "
            f"access={scoped_note}]."
        )
        self._update_download_button_state()

    def _populate_layers_table(self) -> None:
        self.layers_table.setRowCount(len(self._layers))
        for row_idx, layer in enumerate(self._layers):
            name_item = QTableWidgetItem(layer.name)
            name_item.setData(Qt.UserRole, layer.layer_id)

            geom_item = QTableWidgetItem(f"{layer.geometry_family} ({layer.dimensions})")
            crs_item = QTableWidgetItem(layer.crs_display)

            if layer.export_permitted:
                export_text = "Export Ready (Read-Only)"
            elif not layer.crs_explicit:
                export_text = "Blocked (Explicit CRS Required)"
            else:
                export_text = "Not Permitted"
            export_item = QTableWidgetItem(export_text)

            self.layers_table.setItem(row_idx, 0, name_item)
            self.layers_table.setItem(row_idx, 1, geom_item)
            self.layers_table.setItem(row_idx, 2, crs_item)
            self.layers_table.setItem(row_idx, 3, export_item)

    def _selected_layer(self) -> QgisLayerInfo | None:
        row = self.layers_table.currentRow()
        if row < 0 or row >= len(self._layers):
            return None
        return self._layers[row]

    def _update_download_button_state(self) -> None:
        layer = self._selected_layer()
        self.download_open_button.setEnabled(layer is not None and layer.export_permitted)

    def _on_download_and_open(self) -> None:
        layer = self._selected_layer()
        if layer is None:
            return
        if not layer.export_permitted:
            QMessageBox.warning(
                self,
                "OpenVietGIS Connector",
                "Selected layer is not permitted for read-only GeoPackage export.",
            )
            return

        destination_dir = self.download_dir_input.text().strip()
        if not destination_dir:
            QMessageBox.warning(
                self,
                "OpenVietGIS Connector",
                "Please choose a local folder for downloaded GeoPackage files.",
            )
            return

        try:
            config = self._build_config()
            self.status_label.setText(
                f"Downloading and verifying SHA-256 for '{layer.name}'..."
            )
            downloaded = download_layer_geopackage(
                config,
                layer.layer_id,
                destination_dir,
            )
        except OpenVietGisError as exc:
            self.status_label.setText(f"Download failed: {exc}")
            QMessageBox.critical(self, "OpenVietGIS Export Error", str(exc))
            return

        vector_layer = QgsVectorLayer(str(downloaded.path), layer.name, "ogr")
        if not vector_layer.isValid():
            msg = (
                f"GeoPackage downloaded and SHA-256 verified at '{downloaded.path}', "
                "but QGIS could not open it as a valid OGR vector layer."
            )
            self.status_label.setText(msg)
            QMessageBox.critical(self, "QGIS Layer Load Error", msg)
            return

        vector_layer.setReadOnly(True)
        QgsProject.instance().addMapLayer(vector_layer)

        crs_note = downloaded.crs or layer.crs_display
        self.status_label.setText(
            f"Opened read-only layer '{layer.name}' ({crs_note}, "
            f"{downloaded.feature_count} features, SHA-256 {downloaded.sha256[:12]}...) "
            f"from {downloaded.path}"
        )

    def closeEvent(self, event: Any) -> None:
        """Clear sensitive bearer token from memory when closing the dialog."""
        self.bearer_key_input.clear()
        super().closeEvent(event)


class OpenVietGisConnectorPlugin:
    """QGIS 3 plugin controller registering the read-only connector action."""

    def __init__(self, iface: Any) -> None:
        self.iface = iface
        self.action: QAction | None = None
        self.dialog: OpenVietGisConnectorDialog | None = None

    def initGui(self) -> None:
        self.action = QAction(ACTION_LABEL, self.iface.mainWindow())
        self.action.setStatusTip(
            "Connect to OpenVietGIS and open SHA-256 verified GeoPackage layers in read-only mode"
        )
        self.action.triggered.connect(self.run)
        self.iface.addPluginToWebMenu(PLUGIN_MENU_NAME, self.action)
        self.iface.addToolBarIcon(self.action)

    def unload(self) -> None:
        if self.dialog is not None:
            self.dialog.close()
            self.dialog = None
        if self.action is not None:
            self.iface.removePluginWebMenu(PLUGIN_MENU_NAME, self.action)
            self.iface.removeToolBarIcon(self.action)
            self.action = None

    def run(self) -> None:
        if self.dialog is None:
            self.dialog = OpenVietGisConnectorDialog(parent=self.iface.mainWindow())
        self.dialog.show()
        self.dialog.raise_()
        self.dialog.activateWindow()
