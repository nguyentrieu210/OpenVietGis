"""QGIS 3.28+ Desktop Connector plugin UI for OpenVietGIS (R026).

Security boundaries:
- Read-only by default (`default_write_policy = deny`): downloaded GeoPackage
  snapshots are SHA-256 verified and opened read-only (`layer.setReadOnly(True)`).
- Controlled two-step GeoPackage sync (`project.edit` + GIS ACL):
  1. Step 1 ("1. Xem trước thay đổi (Diff)") calls `sync-preview` without mutating
     server state and returns counts plus a deterministic `diff_sha256`.
  2. Step 2 ("2. Xác nhận đẩy Changeset") remains DISABLED unless `preview.can_commit`
     is True, requires explicit `QMessageBox.question` confirmation, and submits the
     exact previewed `diff_sha256` with a fresh UUID `operation_id` via R006 changesets.
  3. Deletions are blocked by default: features absent from the local `.gpkg` are
     reported as `omitted_server_feature_count` and never deleted on the server.
- Service Principal Bearer keys use password echo mode, remain in memory for the
  lifetime of the dialog only, and are NEVER saved to QGIS project files (.qgs/.qgz)
  or plaintext settings.
- Explicit CRS only: displays verified EPSG codes returned by OpenVietGIS and never
  guesses or mutates coordinate systems.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any
from uuid import uuid4

from qgis.core import QgsProject, QgsVectorLayer
from qgis.PyQt.QtCore import Qt
from qgis.PyQt.QtWidgets import (
    QAbstractItemView,
    QAction,
    QCheckBox,
    QDialog,
    QDockWidget,
    QFileDialog,
    QFormLayout,
    QGroupBox,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPushButton,
    QScrollArea,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from .client import (
    OpenVietGisConfig,
    OpenVietGisError,
    QgisLayerInfo,
    SyncPreviewResult,
    decode_pairing_code,
    download_layer_geopackage,
    fetch_qgis_profile,
    preview_layer_geopackage_sync,
    push_layer_geopackage_sync,
)
from .workspace import (
    SyncPreview as WorkspaceSyncPreview,
)
from .workspace import (
    WorkspaceConflict,
    commit_workspace,
    initialize_workspace,
    preview_workspace,
    recover_workspace,
)

PLUGIN_MENU_NAME = "&OpenVietGIS"
ACTION_LABEL = "OpenVietGIS Connector (R026 / R027)"


class OpenVietGisConnectorDialog(QDialog):
    """Dialog for read-only GeoPackage download and controlled two-step R006 sync."""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("OpenVietGIS Connector (0.5.0)")
        self.setMinimumWidth(420)

        self._layers: list[QgisLayerInfo] = []
        self._pinned_revisions: dict[str, tuple[str, str]] = {}
        self._pending_preview: SyncPreviewResult | None = None
        self._pending_preview_context: tuple[str, str, str, str] | None = None
        self._workspace_root: Path | None = None
        self._workspace_preview: WorkspaceSyncPreview | None = None
        self._workspace_map_layer_id: str | None = None
        self._preferred_layer_id: str | None = None
        self._build_ui()

    def _build_ui(self) -> None:
        outer = QVBoxLayout(self)
        outer.setContentsMargins(4, 4, 4, 4)
        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        body = QWidget()
        root_layout = QVBoxLayout(body)
        root_layout.setSpacing(8)
        scroll.setWidget(body)
        outer.addWidget(scroll)

        # 1. Quick Pairing Bar (always visible, 1 click)
        pair_group = QGroupBox("Bước 1 · Kết nối OpenVietGIS")
        pair_layout = QVBoxLayout(pair_group)
        pairing_row = QHBoxLayout()
        self.pairing_code_input = QLineEdit()
        self.pairing_code_input.setEchoMode(QLineEdit.Password)
        self.pairing_code_input.setPlaceholderText(
            "Dán mã ghép nối ovg-pair-v1:... từ web"
        )
        self.apply_pairing_button = QPushButton("Kết nối ngay")
        self.apply_pairing_button.setStyleSheet(
            "background-color: #1d4ed8; color: #ffffff; font-weight: bold; "
            "padding: 5px 12px; border-radius: 4px;"
        )
        self.apply_pairing_button.clicked.connect(self._on_apply_pairing_code)
        pairing_row.addWidget(self.pairing_code_input)
        pairing_row.addWidget(self.apply_pairing_button)
        pair_layout.addLayout(pairing_row)

        conn_bar = QHBoxLayout()
        self.load_layers_button = QPushButton("↻ Tải lại lớp")
        self.load_layers_button.clicked.connect(self._on_load_layers)
        self.manual_conn_toggle = QPushButton("Cài đặt tay ▾")
        self.manual_conn_toggle.setCheckable(True)
        self.manual_conn_toggle.toggled.connect(self._on_toggle_manual_conn)
        conn_bar.addWidget(self.load_layers_button)
        conn_bar.addWidget(self.manual_conn_toggle)
        conn_bar.addStretch(1)
        pair_layout.addLayout(conn_bar)

        self.manual_conn_box = QWidget()
        form = QFormLayout(self.manual_conn_box)
        form.setContentsMargins(0, 4, 0, 0)

        self.server_url_input = QLineEdit()
        self.server_url_input.setPlaceholderText("https://openvietgis.cloud")
        form.addRow("Server URL:", self.server_url_input)

        self.allow_localhost_checkbox = QCheckBox("Cho phép http://localhost (dev)")
        self.allow_localhost_checkbox.setChecked(False)
        form.addRow("", self.allow_localhost_checkbox)

        self.org_id_input = QLineEdit()
        self.org_id_input.setPlaceholderText("Organization UUID")
        form.addRow("Org UUID:", self.org_id_input)

        self.project_id_input = QLineEdit()
        self.project_id_input.setPlaceholderText("Project UUID")
        form.addRow("Project UUID:", self.project_id_input)

        self.bearer_key_input = QLineEdit()
        self.bearer_key_input.setEchoMode(QLineEdit.Password)
        self.bearer_key_input.setPlaceholderText("Bearer Key (không lưu xuống đĩa)")
        form.addRow("Bearer Key:", self.bearer_key_input)

        folder_row = QHBoxLayout()
        default_dir = str(Path.home() / "OpenVietGIS_Exports")
        self.download_dir_input = QLineEdit(default_dir)
        self.browse_dir_button = QPushButton("Chọn…")
        self.browse_dir_button.clicked.connect(self._on_browse_dir)
        folder_row.addWidget(self.download_dir_input)
        folder_row.addWidget(self.browse_dir_button)
        form.addRow("Thư mục lưu:", folder_row)

        self.manual_conn_box.setVisible(False)
        pair_layout.addWidget(self.manual_conn_box)
        root_layout.addWidget(pair_group)

        # 2. Layer Picker (compact, directly visible)
        layers_group = QGroupBox("Bước 2 · Chọn lớp bản đồ")
        layers_layout = QVBoxLayout(layers_group)
        self.layers_table = QTableWidget(0, 4)
        self.layers_table.setMinimumHeight(150)
        self.layers_table.setMaximumHeight(220)
        self.layers_table.setHorizontalHeaderLabels(
            ["Tên lớp", "Loại", "CRS", "Trạng thái"]
        )
        self.layers_table.setSelectionBehavior(QAbstractItemView.SelectRows)
        self.layers_table.setSelectionMode(QAbstractItemView.SingleSelection)
        self.layers_table.setEditTriggers(QAbstractItemView.NoEditTriggers)
        self.layers_table.itemSelectionChanged.connect(self._on_layer_selection_changed)
        header = self.layers_table.horizontalHeader()
        header.setSectionResizeMode(0, QHeaderView.Stretch)
        header.setSectionResizeMode(1, QHeaderView.ResizeToContents)
        header.setSectionResizeMode(2, QHeaderView.ResizeToContents)
        header.setSectionResizeMode(3, QHeaderView.ResizeToContents)
        layers_layout.addWidget(self.layers_table)
        root_layout.addWidget(layers_group)

        # 3. Primary Two-Way Sync Box (top-level, 3 big buttons, no scrolling!)
        workspace_group = QGroupBox("Bước 3 · Sửa & Đồng bộ 2 chiều (QGIS ↔ Web)")
        ws_layout = QVBoxLayout(workspace_group)

        ws_buttons = QVBoxLayout()
        self.ws_create_button = QPushButton("1. Mở lớp để sửa trên QGIS")
        self.ws_create_button.setStyleSheet(
            "background-color: #0284c7; color: #ffffff; font-weight: bold; "
            "padding: 7px 12px; border-radius: 4px;"
        )
        self.ws_create_button.clicked.connect(self._on_create_workspace)

        ws_sync_row = QHBoxLayout()
        self.ws_preview_button = QPushButton("2. Xem thay đổi")
        self.ws_preview_button.setStyleSheet(
            "padding: 7px 10px; font-weight: bold; border-radius: 4px;"
        )
        self.ws_preview_button.clicked.connect(self._on_preview_workspace)

        self.ws_commit_button = QPushButton("3. Đồng bộ lên Web")
        self.ws_commit_button.setEnabled(False)
        self.ws_commit_button.setStyleSheet(
            "background-color: #16a34a; color: #ffffff; font-weight: bold; "
            "padding: 7px 10px; border-radius: 4px;"
        )
        self.ws_commit_button.clicked.connect(self._on_commit_workspace)

        ws_sync_row.addWidget(self.ws_preview_button)
        ws_sync_row.addWidget(self.ws_commit_button)
        ws_buttons.addWidget(self.ws_create_button)
        ws_buttons.addLayout(ws_sync_row)
        ws_layout.addLayout(ws_buttons)

        self.ws_status = QLabel(
            "Chọn lớp → bấm '1. Mở lớp để sửa' → vẽ/sửa trực tiếp trên bản đồ → "
            "bấm '2. Xem thay đổi' → '3. Đồng bộ lên Web'."
        )
        self.ws_status.setWordWrap(True)
        self.ws_status.setStyleSheet(
            "background-color: #f0f9ff; border: 1px solid #bae6fd; "
            "padding: 6px; border-radius: 4px;"
        )
        ws_layout.addWidget(self.ws_status)
        root_layout.addWidget(workspace_group)

        self.status_label = QLabel("Sẵn sàng. Dán mã ghép nối để bắt đầu.")
        self.status_label.setWordWrap(True)
        root_layout.addWidget(self.status_label)

        # 4. Collapsible Advanced Section (hidden by default to keep UI clean)
        self.advanced_toggle = QPushButton("Công cụ nâng cao (Chỉ đọc / R026 / Khôi phục) ▾")
        self.advanced_toggle.setCheckable(True)
        self.advanced_toggle.toggled.connect(self._on_toggle_advanced)
        root_layout.addWidget(self.advanced_toggle)

        self.advanced_box = QWidget()
        adv_layout = QVBoxLayout(self.advanced_box)
        adv_layout.setContentsMargins(0, 4, 0, 0)

        adv_quick_row = QHBoxLayout()
        self.download_open_button = QPushButton("Tải & mở Chỉ đọc")
        self.download_open_button.setEnabled(False)
        self.download_open_button.clicked.connect(self._on_download_and_open)
        self.ws_open_button = QPushButton("Mở workspace cũ")
        self.ws_open_button.clicked.connect(self._on_open_workspace)
        self.ws_recover_button = QPushButton("Khôi phục giao dịch")
        self.ws_recover_button.clicked.connect(self._on_recover_workspace)
        adv_quick_row.addWidget(self.download_open_button)
        adv_quick_row.addWidget(self.ws_open_button)
        adv_quick_row.addWidget(self.ws_recover_button)
        adv_layout.addLayout(adv_quick_row)

        self.ws_path_input = QLineEdit()
        self.ws_path_input.setReadOnly(True)
        self.ws_path_input.setPlaceholderText("Đường dẫn thư mục workspace hiện tại")
        adv_layout.addWidget(self.ws_path_input)

        sync_group = QGroupBox("R026 thủ công (chọn tệp .gpkg rời)")
        sync_layout = QVBoxLayout(sync_group)
        sync_form = QFormLayout()

        gpkg_row = QHBoxLayout()
        self.sync_gpkg_input = QLineEdit()
        self.sync_gpkg_input.setPlaceholderText("Chọn tệp .gpkg (tối đa 16 MiB)...")
        self.sync_gpkg_input.textChanged.connect(self._on_sync_inputs_changed)
        self.browse_sync_gpkg_button = QPushButton("Chọn…")
        self.browse_sync_gpkg_button.clicked.connect(self._on_browse_sync_gpkg)
        gpkg_row.addWidget(self.sync_gpkg_input)
        gpkg_row.addWidget(self.browse_sync_gpkg_button)
        sync_form.addRow("Tệp .gpkg:", gpkg_row)

        self.sync_summary_input = QLineEdit("QGIS GeoPackage sync push")
        sync_form.addRow("Mô tả:", self.sync_summary_input)

        self.pinned_revision_label = QLabel("Chưa chọn lớp đích.")
        self.pinned_revision_label.setWordWrap(True)
        sync_form.addRow("Revision:", self.pinned_revision_label)
        sync_layout.addLayout(sync_form)

        sync_buttons_row = QHBoxLayout()
        self.preview_sync_button = QPushButton("Xem trước Diff (R026)")
        self.preview_sync_button.setEnabled(False)
        self.preview_sync_button.clicked.connect(self._on_preview_sync)
        self.push_sync_button = QPushButton("Đẩy Changeset (R026)")
        self.push_sync_button.setEnabled(False)
        self.push_sync_button.clicked.connect(self._on_push_sync)
        sync_buttons_row.addWidget(self.preview_sync_button)
        sync_buttons_row.addWidget(self.push_sync_button)
        sync_layout.addLayout(sync_buttons_row)

        self.sync_preview_label = QLabel("Chưa xem trước thay đổi R026.")
        self.sync_preview_label.setWordWrap(True)
        sync_layout.addWidget(self.sync_preview_label)
        adv_layout.addWidget(sync_group)

        self.advanced_box.setVisible(False)
        root_layout.addWidget(self.advanced_box)
        root_layout.addStretch(1)

    def _on_toggle_manual_conn(self, checked: bool) -> None:
        self.manual_conn_box.setVisible(checked)
        self.manual_conn_toggle.setText("Cài đặt tay ▴" if checked else "Cài đặt tay ▾")

    def _on_toggle_advanced(self, checked: bool) -> None:
        self.advanced_box.setVisible(checked)
        self.advanced_toggle.setText(
            "Công cụ nâng cao (Chỉ đọc / R026 / Khôi phục) ▴"
            if checked
            else "Công cụ nâng cao (Chỉ đọc / R026 / Khôi phục) ▾"
        )

    def _build_config(self) -> OpenVietGisConfig:
        return OpenVietGisConfig(
            base_url=self.server_url_input.text(),
            organization_id=self.org_id_input.text(),
            project_id=self.project_id_input.text(),
            bearer_token=self.bearer_key_input.text(),
            allow_insecure_localhost=self.allow_localhost_checkbox.isChecked(),
        )

    def _on_apply_pairing_code(self) -> None:
        raw = self.pairing_code_input.text().strip()
        if not raw:
            QMessageBox.warning(
                self, "OpenVietGIS Connector", "Hãy dán mã ghép nối ovg-pair-v1:... trước."
            )
            return
        try:
            parsed = decode_pairing_code(raw)
        except OpenVietGisError as exc:
            QMessageBox.warning(self, "Mã ghép nối không hợp lệ", str(exc))
            return
        self.server_url_input.setText(parsed["server_url"])
        self.org_id_input.setText(parsed["organization_id"])
        self.project_id_input.setText(parsed["project_id"])
        self.bearer_key_input.setText(parsed["bearer_token"])
        self._preferred_layer_id = parsed.get("layer_id")
        self.pairing_code_input.clear()
        self._on_load_layers()
        if self._preferred_layer_id:
            for row_idx, lyr in enumerate(self._layers):
                if lyr.layer_id == self._preferred_layer_id:
                    self.layers_table.selectRow(row_idx)
                    break

    def _on_browse_dir(self) -> None:
        selected = QFileDialog.getExistingDirectory(
            self,
            "Select Local GeoPackage Download Folder",
            self.download_dir_input.text() or str(Path.home()),
        )
        if selected:
            self.download_dir_input.setText(selected)

    def _on_browse_sync_gpkg(self) -> None:
        start_dir = self.download_dir_input.text().strip() or str(Path.home())
        selected, _ = QFileDialog.getOpenFileName(
            self,
            "Select Edited Local GeoPackage (.gpkg)",
            start_dir,
            "GeoPackage (*.gpkg);;All Files (*)",
        )
        if selected:
            self.sync_gpkg_input.setText(selected)

    def _invalidate_sync_preview(self, message: str | None = None) -> None:
        self._pending_preview = None
        self._pending_preview_context = None
        self.push_sync_button.setEnabled(False)
        if message is not None:
            self.sync_preview_label.setText(message)

    def _pinned_revisions_for_layer(self, layer: QgisLayerInfo) -> tuple[str, str]:
        return self._pinned_revisions.get(
            layer.layer_id,
            (layer.content_revision, layer.schema_revision),
        )

    def _on_load_layers(self) -> None:
        self.download_open_button.setEnabled(False)
        self.preview_sync_button.setEnabled(False)
        self._invalidate_sync_preview(
            "Danh sách lớp đã tải lại; hãy chọn lớp và bấm bước 1 để xem trước diff."
        )
        try:
            config = self._build_config()
            self.status_label.setText("Fetching QGIS adapter profile...")
            profile = fetch_qgis_profile(config)
        except OpenVietGisError as exc:
            self.status_label.setText(f"Error: {exc}")
            QMessageBox.warning(self, "OpenVietGIS Connector", str(exc))
            return

        self._layers = list(profile.layers)
        self._populate_layers_table()
        scoped_note = "scoped ACL" if profile.scoped_access else "project role"
        sync_note = "two-step sync enabled" if profile.sync_push_enabled else "read-only"
        self.status_label.setText(
            f"Loaded {len(self._layers)} authorized layer(s) "
            f"[policy={profile.default_write_policy}, status={profile.execution_status}, "
            f"access={scoped_note}, mode={sync_note}]."
        )
        self._update_action_button_states()

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

    def _on_layer_selection_changed(self) -> None:
        self._invalidate_sync_preview(
            "Đã thay đổi lớp đích. Bấm '1. Xem trước thay đổi (Diff)' trước khi đẩy Changeset."
        )
        self._update_action_button_states()

    def _on_sync_inputs_changed(self) -> None:
        self._invalidate_sync_preview(
            "Tệp .gpkg đã thay đổi. Bấm '1. Xem trước thay đổi (Diff)' trước khi đẩy Changeset."
        )
        self._update_action_button_states()

    def _update_action_button_states(self) -> None:
        layer = self._selected_layer()
        self.download_open_button.setEnabled(layer is not None and layer.export_permitted)
        has_gpkg = bool(self.sync_gpkg_input.text().strip())
        self.preview_sync_button.setEnabled(
            layer is not None and layer.crs_explicit and has_gpkg
        )
        if layer is None:
            self.pinned_revision_label.setText(
                "Chưa chọn lớp đích / No target layer selected."
            )
        else:
            content_rev, schema_rev = self._pinned_revisions_for_layer(layer)
            self.pinned_revision_label.setText(
                f"{layer.name} ({layer.crs_display}) — "
                f"content={content_rev[:12]}..., schema={schema_rev[:12]}..."
            )

    def _workspace_layer(self) -> QgsVectorLayer | None:
        if not self._workspace_map_layer_id:
            return None
        layer = QgsProject.instance().mapLayer(self._workspace_map_layer_id)
        return layer if isinstance(layer, QgsVectorLayer) else None

    def _workspace_file(self) -> Path:
        if self._workspace_root is None:
            raise WorkspaceConflict("Chưa chọn workspace.")
        return self._workspace_root / "working.gpkg"

    def _open_working_layer(self) -> None:
        workfile = self._workspace_file()
        current = self._workspace_layer()
        if current is not None:
            if current.isEditable():
                raise WorkspaceConflict("Hãy Lưu/Kết thúc chế độ chỉnh sửa QGIS trước.")
            QgsProject.instance().removeMapLayer(current.id())
        vector = QgsVectorLayer(str(workfile), "OpenVietGIS R027 — Bản làm việc", "ogr")
        if not vector.isValid():
            raise WorkspaceConflict("Không thể mở GeoPackage của workspace bằng OGR.")
        QgsProject.instance().addMapLayer(vector)
        self._workspace_map_layer_id = vector.id()
        self.ws_path_input.setText(str(self._workspace_root))

    def _require_workspace_layer_closed(self) -> None:
        workfile = self._workspace_file().resolve()
        own = self._workspace_layer()
        for layer in list(QgsProject.instance().mapLayers().values()):
            if isinstance(layer, QgsVectorLayer):
                path = layer.source().split("|", 1)[0]
                try:
                    same = Path(path).resolve() == workfile
                except (OSError, ValueError):
                    same = False
                if same:
                    if layer.isEditable():
                        raise WorkspaceConflict(
                            "QGIS đang ở chế độ chỉnh sửa. Lưu và tắt chỉnh sửa trước khi tiếp tục."
                        )
                    if own is None or layer.id() != own.id():
                        raise WorkspaceConflict(
                            "Có lớp QGIS khác đang mở cùng tệp. Hãy đóng các lớp bản sao trước."
                        )
        if own is not None:
            QgsProject.instance().removeMapLayer(own.id())
            self._workspace_map_layer_id = None

    def _on_create_workspace(self) -> None:
        layer = self._selected_layer()
        if layer is None or not layer.export_permitted or not layer.crs_explicit:
            QMessageBox.warning(self, "R027 Workspace", "Chọn lớp QGIS được phép xuất và có CRS hợp lệ.")
            return
        try:
            config = self._build_config()
            root = initialize_workspace(
                config, layer.layer_id,
                self.download_dir_input.text().strip() or str(Path.home()),
            )
            self._workspace_root = root
            self._workspace_preview = None
            self.ws_commit_button.setEnabled(False)
            self._open_working_layer()
            self.ws_status.setText(
                "Đã tạo bản làm việc: lưu chỉnh sửa trong QGIS rồi bấm Xem trước 2 chiều. "
                "Tất cả thay đổi đều yêu cầu xác nhận, không có đồng bộ tự động."
            )
        except (OpenVietGisError, WorkspaceConflict, OSError, ValueError) as exc:
            QMessageBox.warning(self, "R027 Workspace", str(exc))
            self.ws_status.setText(f"Lỗi tạo workspace: {exc}")

    def _on_open_workspace(self) -> None:
        chosen = QFileDialog.getExistingDirectory(
            self, "Chọn thư mục workspace R027",
            self.download_dir_input.text().strip() or str(Path.home())
        )
        if not chosen:
            return
        try:
            config = self._build_config()
            # Reading a preview validates server scope, baseline, and schema;
            # never bypass pending-recovery state.
            self._workspace_root = Path(chosen).resolve()
            result = preview_workspace(config, self._workspace_root)
            result_digest = result.digest[:12]
            self._workspace_preview = None
            self.ws_commit_button.setEnabled(False)
            self._open_working_layer()
            self.ws_status.setText(
                f"Đã mở workspace; kiểm tra an toàn OK (digest {result_digest}). "
                "Bấm Xem trước 2 chiều để duyệt thay đổi."
            )
        except (OpenVietGisError, WorkspaceConflict, OSError, ValueError) as exc:
            QMessageBox.warning(self, "R027 Workspace", str(exc))
            self.ws_status.setText(f"Không thể mở workspace: {exc}")

    def _on_preview_workspace(self) -> None:
        try:
            config = self._build_config()
            layer = self._workspace_layer()
            if layer is not None and layer.isEditable():
                if not layer.commitChanges():
                    raise WorkspaceConflict(
                        "Không thể tự lưu chỉnh sửa trên QGIS. Hãy bấm Save và tắt Edit Mode."
                    )
            preview = preview_workspace(config, self._workspace_file().parent)
            self._workspace_preview = preview
            self.ws_commit_button.setEnabled(not bool(preview.conflicts))
            if preview.conflicts:
                self.ws_status.setText(
                    "CÓ XUNG ĐỘT — KHÔNG GHI DỮ LIỆU: "
                    + "; ".join(preview.conflicts[:5])
                    + (" ..." if len(preview.conflicts) > 5 else "")
                )
            else:
                self.ws_status.setText(
                    f"Xem trước: về QGIS {len(preview.to_local)} thay đổi, "
                    f"lên OpenVietGIS {len(preview.to_server)} thay đổi. "
                    f"Digest {preview.digest[:16]}... Không tự động xóa."
                )
        except (OpenVietGisError, WorkspaceConflict, OSError, ValueError) as exc:
            self._workspace_preview = None
            self.ws_commit_button.setEnabled(False)
            self.ws_status.setText(f"Lỗi xem trước: {exc}")
            QMessageBox.warning(self, "R027 Preview", str(exc))

    def _on_commit_workspace(self) -> None:
        preview = self._workspace_preview
        if preview is None or preview.conflicts:
            self.ws_commit_button.setEnabled(False)
            return
        confirmation = QMessageBox.question(
            self, "Xác nhận đồng bộ hai chiều",
            f"Đẩy lên máy chủ {len(preview.to_server)} thay đổi, "
            f"nhận về QGIS {len(preview.to_local)} thay đổi?\n"
            f"Digest: {preview.digest[:16]}...\n"
            "Đã lưu và tắt chế độ chỉnh sửa trong QGIS? "
            "Hệ thống sẽ sử dụng changeset R006 và khóa xóa mặc định.",
            QMessageBox.Yes | QMessageBox.No, QMessageBox.No,
        )
        if confirmation != QMessageBox.Yes:
            return
        self.ws_commit_button.setEnabled(False)
        try:
            config = self._build_config()
            # Close the OGR file handle before atomic file replacement.
            self._require_workspace_layer_closed()
            receipt = commit_workspace(config, preview, confirmed_digest=preview.digest)
            self._workspace_preview = None
            self.ws_status.setText(
                f"Đồng bộ thành công. Về QGIS: {receipt['pulled']}; "
                f"lên OpenVietGIS: {receipt['pushed']}; "
                f"revision: {receipt['new_content_revision'][:12]}..."
            )
        except (OpenVietGisError, WorkspaceConflict, OSError, ValueError) as exc:
            self.ws_status.setText(
                f"Chưa xác nhận đồng bộ thành công: {exc}. "
                "Nếu mạng gián đoạn sau khi gửi, chọn 'Khôi phục giao dịch' "
                "trước khi tiếp tục. Không tạo giao dịch mới."
            )
            QMessageBox.warning(self, "R027 Sync", str(exc))
        finally:
            if self._workspace_root is not None:
                try:
                    self._open_working_layer()
                except (WorkspaceConflict, OSError) as exc:
                    self.ws_status.setText(
                        f"Workspace đã xử lý nhưng QGIS chưa thể mở lại tệp: {exc}"
                    )

    def _on_recover_workspace(self) -> None:
        if self._workspace_root is None:
            return
        try:
            config = self._build_config()
            self._require_workspace_layer_closed()
            recovered = recover_workspace(config, self._workspace_root)
            self._workspace_preview = None
            self.ws_commit_button.setEnabled(False)
            self.ws_status.setText(
                "Đã xác minh dữ liệu hai phía và khôi phục mốc chung."
                if recovered else "Không có giao dịch cần khôi phục."
            )
        except (OpenVietGisError, WorkspaceConflict, OSError, ValueError) as exc:
            self.ws_status.setText(f"Chưa thể khôi phục an toàn: {exc}")
            QMessageBox.warning(self, "R027 Recovery", str(exc))
        finally:
            try:
                self._open_working_layer()
            except (WorkspaceConflict, OSError):
                pass

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

        if downloaded.content_revision and downloaded.schema_revision:
            self._pinned_revisions[layer.layer_id] = (
                downloaded.content_revision,
                downloaded.schema_revision,
            )
            self._update_action_button_states()

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

    def _on_preview_sync(self) -> None:
        self._invalidate_sync_preview()
        layer = self._selected_layer()
        if layer is None:
            QMessageBox.warning(
                self,
                "OpenVietGIS Sync Preview",
                "Vui lòng chọn lớp đích trong danh sách trước khi xem trước diff.",
            )
            return

        gpkg_path = self.sync_gpkg_input.text().strip()
        if not gpkg_path:
            QMessageBox.warning(
                self,
                "OpenVietGIS Sync Preview",
                "Vui lòng chọn tệp GeoPackage (.gpkg) cục bộ đã chỉnh sửa.",
            )
            return

        base_content_rev, base_schema_rev = self._pinned_revisions_for_layer(layer)
        try:
            config = self._build_config()
            self.status_label.setText(
                f"Previewing GeoPackage diff for '{layer.name}' (no server mutation)..."
            )
            preview = preview_layer_geopackage_sync(
                config,
                layer_id=layer.layer_id,
                gpkg_path=gpkg_path,
                base_content_revision=base_content_rev,
                base_schema_revision=base_schema_rev,
            )
        except OpenVietGisError as exc:
            self.status_label.setText(f"Sync preview failed: {exc}")
            self.sync_preview_label.setText(f"Lỗi xem trước diff: {exc}")
            QMessageBox.critical(self, "OpenVietGIS Sync Preview Error", str(exc))
            return

        self._pending_preview = preview
        self._pending_preview_context = (
            layer.layer_id,
            gpkg_path,
            base_content_rev,
            base_schema_rev,
        )
        self.push_sync_button.setEnabled(bool(preview.can_commit))

        stale_text = (
            "CÓ (stale_revision=True — dữ liệu trên máy chủ đã thay đổi, cần tải lại)"
            if preview.stale_revision
            else "Không (stale_revision=False)"
        )
        deletion_note = (
            "xóa đối tượng bị chặn mặc định / deletions blocked by default"
            if preview.deletions_blocked
            else "deletions enabled"
        )
        commit_status = (
            "Sẵn sàng đẩy Changeset (can_commit=True)"
            if preview.can_commit
            else "Không thể đẩy Changeset (can_commit=False)"
        )
        self.sync_preview_label.setText(
            f"Diff Preview ({layer.name} | EPSG:{preview.stored_crs_code}): "
            f"Create={preview.create_count}, Update={preview.update_count}, "
            f"Unchanged={preview.unchanged_count}, "
            f"Omitted on server={preview.omitted_server_feature_count} ({deletion_note}).\n"
            f"Stale revision: {stale_text} | diff_sha256={preview.diff_sha256[:16]}... | "
            f"{commit_status}."
        )
        self.status_label.setText(
            f"Sync preview complete for '{layer.name}': "
            f"+{preview.create_count} create, ~{preview.update_count} update, "
            f"{preview.unchanged_count} unchanged, "
            f"{preview.omitted_server_feature_count} omitted (deletions blocked), "
            f"diff={preview.diff_sha256[:16]}..., can_commit={preview.can_commit}."
        )

    def _on_push_sync(self) -> None:
        preview = self._pending_preview
        context = self._pending_preview_context
        layer = self._selected_layer()
        if preview is None or context is None or not preview.can_commit:
            self.push_sync_button.setEnabled(False)
            QMessageBox.warning(
                self,
                "OpenVietGIS Sync Push",
                "Không có bản xem trước diff hợp lệ (can_commit=True). "
                "Vui lòng chạy bước 1 trước.",
            )
            return

        preview_layer_id, preview_gpkg_path, base_content_rev, base_schema_rev = context
        if (
            layer is None
            or layer.layer_id != preview_layer_id
            or self.sync_gpkg_input.text().strip() != preview_gpkg_path
        ):
            self._invalidate_sync_preview(
                "Lớp đích hoặc tệp .gpkg đã thay đổi sau khi xem trước. "
                "Vui lòng chạy lại bước 1."
            )
            return

        confirm_text = (
            f"Xác nhận đẩy Changeset R006 lên lớp '{layer.name}' "
            f"(EPSG:{preview.stored_crs_code})?\n\n"
            f"- Thêm mới (create_count): {preview.create_count}\n"
            f"- Cập nhật (update_count): {preview.update_count}\n"
            f"- Không đổi (unchanged_count): {preview.unchanged_count}\n"
            f"- Đối tượng thiếu trong tệp cục bộ (omitted_server_feature_count): "
            f"{preview.omitted_server_feature_count} "
            f"(KHÔNG bị xóa — deletions blocked by default)\n"
            f"- diff_sha256: {preview.diff_sha256[:16]}...\n\n"
            "Thao tác này sẽ tạo một R006 Changeset có ghi nhật ký kiểm toán trên máy chủ."
        )
        choice = QMessageBox.question(
            self,
            "Xác nhận đẩy Changeset / Confirm Sync Push",
            confirm_text,
            QMessageBox.Yes | QMessageBox.No,
            QMessageBox.No,
        )
        if choice != QMessageBox.Yes:
            return

        operation_id = str(uuid4())
        summary = self.sync_summary_input.text().strip() or "QGIS GeoPackage sync push"
        try:
            config = self._build_config()
            self.status_label.setText(
                f"Pushing confirmed changeset to '{layer.name}' "
                f"(operation_id={operation_id[:12]}...)..."
            )
            result = push_layer_geopackage_sync(
                config,
                layer_id=preview_layer_id,
                gpkg_path=preview_gpkg_path,
                base_content_revision=base_content_rev,
                base_schema_revision=base_schema_rev,
                expected_diff_sha256=preview.diff_sha256,
                operation_id=operation_id,
                summary=summary,
            )
        except OpenVietGisError as exc:
            self.status_label.setText(f"Sync push failed: {exc}")
            QMessageBox.critical(self, "OpenVietGIS Sync Push Error", str(exc))
            return

        self._pinned_revisions[preview_layer_id] = (
            result.resulting_content_revision,
            base_schema_rev,
        )
        self._invalidate_sync_preview(
            f"Đã đẩy Changeset thành công: changeset_id={result.changeset_id}, "
            f"operations={result.operation_count} "
            f"(create={result.create_count}, update={result.update_count}, "
            f"omitted={result.omitted_server_feature_count}, deletions_blocked="
            f"{result.deletions_blocked}), "
            f"new_revision={result.resulting_content_revision[:16]}..., "
            f"audit_event_id={result.audit_event_id}."
        )
        self._update_action_button_states()
        self.status_label.setText(
            f"Committed R006 changeset {result.changeset_id} "
            f"({result.operation_count} ops, new content_revision="
            f"{result.resulting_content_revision[:12]}...)."
        )
        QMessageBox.information(
            self,
            "OpenVietGIS Sync Push Complete",
            f"Đã ghi nhận R006 Changeset thành công!\n\n"
            f"Changeset ID: {result.changeset_id}\n"
            f"Operations: {result.operation_count} "
            f"(Create={result.create_count}, Update={result.update_count})\n"
            f"Omitted Server Features: {result.omitted_server_feature_count} "
            f"(Deletions Blocked: {result.deletions_blocked})\n"
            f"Resulting Content Revision: {result.resulting_content_revision[:16]}...\n"
            f"Audit Event ID: {result.audit_event_id}",
        )

    def closeEvent(self, event: Any) -> None:
        """Clear sensitive bearer token from memory when closing the dialog."""
        self.bearer_key_input.clear()
        self._invalidate_sync_preview()
        self._workspace_preview = None
        self.ws_commit_button.setEnabled(False)
        super().closeEvent(event)


class OpenVietGisConnectorPlugin:
    """QGIS 3 plugin controller docking OpenVietGIS right beside the map canvas."""

    def __init__(self, iface: Any) -> None:
        self.iface = iface
        self.action: QAction | None = None
        self.dialog: OpenVietGisConnectorDialog | None = None
        self.dock: QDockWidget | None = None

    def initGui(self) -> None:
        self.action = QAction(ACTION_LABEL, self.iface.mainWindow())
        self.action.setStatusTip(
            "OpenVietGIS Connector — Ghép nối nhanh & Đồng bộ hai chiều QGIS ↔ Web"
        )
        self.action.triggered.connect(self.run)
        self.iface.addPluginToWebMenu(PLUGIN_MENU_NAME, self.action)
        self.iface.addToolBarIcon(self.action)

    def unload(self) -> None:
        if self.dock is not None:
            self.iface.removeDockWidget(self.dock)
            self.dock.close()
            self.dock = None
        if self.dialog is not None:
            self.dialog.close()
            self.dialog = None
        if self.action is not None:
            self.iface.removePluginWebMenu(PLUGIN_MENU_NAME, self.action)
            self.iface.removeToolBarIcon(self.action)
            self.action = None

    def run(self) -> None:
        if self.dock is None:
            self.dialog = OpenVietGisConnectorDialog(parent=self.iface.mainWindow())
            self.dialog.setWindowFlags(Qt.Widget)
            self.dock = QDockWidget("OpenVietGIS", self.iface.mainWindow())
            self.dock.setObjectName("OpenVietGisConnectorDock")
            self.dock.setAllowedAreas(Qt.LeftDockWidgetArea | Qt.RightDockWidgetArea)
            self.dock.setWidget(self.dialog)
            self.iface.addDockWidget(Qt.RightDockWidgetArea, self.dock)
        self.dock.show()
        self.dock.raise_()
