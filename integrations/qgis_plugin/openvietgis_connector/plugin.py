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
        self.setWindowTitle("OpenVietGIS Connector (R026 / R027)")
        self.setMinimumSize(820, 680)

        self._layers: list[QgisLayerInfo] = []
        self._pinned_revisions: dict[str, tuple[str, str]] = {}
        self._pending_preview: SyncPreviewResult | None = None
        self._pending_preview_context: tuple[str, str, str, str] | None = None
        self._workspace_root: Path | None = None
        self._workspace_preview: WorkspaceSyncPreview | None = None
        self._workspace_map_layer_id: str | None = None
        self._build_ui()

    def _build_ui(self) -> None:
        outer = QVBoxLayout(self)
        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        body = QWidget()
        root_layout = QVBoxLayout(body)
        scroll.setWidget(body)
        outer.addWidget(scroll)

        notice_label = QLabel(
            "Mặc định CHỈ ĐỌC (default_write_policy = deny). Đồng bộ ngược hai bước "
            "(Preview Diff -> Xác nhận đẩy R006 Changeset) yêu cầu quyền project.edit và "
            "chặn xóa mặc định (deletions blocked). API Key chỉ giữ trong bộ nhớ tạm và "
            "KHÔNG lưu vào tệp dự án QGIS (.qgs/.qgz)."
        )
        notice_label.setWordWrap(True)
        notice_label.setStyleSheet(
            "background-color: #f0f4f8; border: 1px solid #cbd5e1; "
            "padding: 8px; border-radius: 4px;"
        )
        root_layout.addWidget(notice_label)

        form = QFormLayout()

        pairing_row = QHBoxLayout()
        self.pairing_code_input = QLineEdit()
        self.pairing_code_input.setEchoMode(QLineEdit.Password)
        self.pairing_code_input.setPlaceholderText(
            "Dán mã ghép nối nhanh ovg-pair-v1:... từ Trung tâm Tích hợp (không cần Developer Portal)"
        )
        self.apply_pairing_button = QPushButton("Áp dụng mã ghép nối")
        self.apply_pairing_button.clicked.connect(self._on_apply_pairing_code)
        pairing_row.addWidget(self.pairing_code_input)
        pairing_row.addWidget(self.apply_pairing_button)
        form.addRow("Mã ghép nối nhanh (R027):", pairing_row)

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
            "Service Principal Bearer Key (project.read; project.edit for sync — not persisted)"
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
        self.layers_table.itemSelectionChanged.connect(self._on_layer_selection_changed)
        header = self.layers_table.horizontalHeader()
        header.setSectionResizeMode(0, QHeaderView.Stretch)
        header.setSectionResizeMode(1, QHeaderView.ResizeToContents)
        header.setSectionResizeMode(2, QHeaderView.ResizeToContents)
        header.setSectionResizeMode(3, QHeaderView.ResizeToContents)
        root_layout.addWidget(self.layers_table)

        download_row = QHBoxLayout()
        self.download_open_button = QPushButton("Tải & mở lớp vào QGIS (Chỉ đọc)")
        self.download_open_button.setEnabled(False)
        self.download_open_button.clicked.connect(self._on_download_and_open)
        download_row.addWidget(self.download_open_button)
        download_row.addStretch(1)
        root_layout.addLayout(download_row)

        sync_group = QGroupBox(
            "Đồng bộ GeoPackage hai bước (R006 Changeset Sync — yêu cầu project.edit)"
        )
        sync_layout = QVBoxLayout(sync_group)

        sync_form = QFormLayout()

        gpkg_row = QHBoxLayout()
        self.sync_gpkg_input = QLineEdit()
        self.sync_gpkg_input.setPlaceholderText(
            "Chọn tệp .gpkg cục bộ đã chỉnh sửa (tối đa 16 MiB)..."
        )
        self.sync_gpkg_input.textChanged.connect(self._on_sync_inputs_changed)
        self.browse_sync_gpkg_button = QPushButton("Chọn tệp .gpkg...")
        self.browse_sync_gpkg_button.clicked.connect(self._on_browse_sync_gpkg)
        gpkg_row.addWidget(self.sync_gpkg_input)
        gpkg_row.addWidget(self.browse_sync_gpkg_button)
        sync_form.addRow("Edited GeoPackage (.gpkg):", gpkg_row)

        self.sync_summary_input = QLineEdit("QGIS GeoPackage sync push")
        self.sync_summary_input.setPlaceholderText("Mô tả Changeset R006...")
        sync_form.addRow("Changeset Summary:", self.sync_summary_input)

        self.pinned_revision_label = QLabel("Chưa chọn lớp đích / No target layer selected.")
        self.pinned_revision_label.setWordWrap(True)
        sync_form.addRow("Pinned Revisions:", self.pinned_revision_label)

        sync_layout.addLayout(sync_form)

        sync_buttons_row = QHBoxLayout()
        self.preview_sync_button = QPushButton("1. Xem trước thay đổi (Diff)")
        self.preview_sync_button.setEnabled(False)
        self.preview_sync_button.clicked.connect(self._on_preview_sync)

        self.push_sync_button = QPushButton("2. Xác nhận đẩy Changeset")
        self.push_sync_button.setEnabled(False)
        self.push_sync_button.clicked.connect(self._on_push_sync)

        sync_buttons_row.addWidget(self.preview_sync_button)
        sync_buttons_row.addWidget(self.push_sync_button)
        sync_buttons_row.addStretch(1)
        sync_layout.addLayout(sync_buttons_row)

        self.sync_preview_label = QLabel(
            "Chưa xem trước thay đổi. Bấm bước 1 để tính diff trên máy chủ "
            "(xóa đối tượng trên máy chủ bị chặn mặc định / deletions blocked by default)."
        )
        self.sync_preview_label.setWordWrap(True)
        self.sync_preview_label.setStyleSheet(
            "background-color: #f8fafc; border: 1px solid #cbd5e1; "
            "padding: 6px; border-radius: 4px;"
        )
        sync_layout.addWidget(self.sync_preview_label)

        root_layout.addWidget(sync_group)

        workspace_group = QGroupBox("R027 — Hai chiều thủ công (QGIS ↔ OpenVietGIS)")
        ws_layout = QVBoxLayout(workspace_group)
        ws_explainer = QLabel(
            "Lưu 3 bản: mốc chung, bản QGIS đang chỉnh và bản OpenVietGIS mới nhất. "
            "Chỉ ghi khi không xung đột. Phải lưu hoàn tất chỉnh sửa QGIS trước khi đồng bộ; "
            "xóa dữ liệu và đồng bộ tự động vẫn bị chặn."
        )
        ws_explainer.setWordWrap(True)
        ws_layout.addWidget(ws_explainer)
        ws_path = QHBoxLayout()
        self.ws_path_input = QLineEdit()
        self.ws_path_input.setReadOnly(True)
        self.ws_path_input.setPlaceholderText("Chưa chọn workspace — mỗi lớp có một thư mục riêng")
        ws_path.addWidget(self.ws_path_input)
        self.ws_open_button = QPushButton("Mở workspace")
        self.ws_open_button.clicked.connect(self._on_open_workspace)
        ws_path.addWidget(self.ws_open_button)
        ws_layout.addLayout(ws_path)
        ws_buttons = QHBoxLayout()
        self.ws_create_button = QPushButton("1. Tạo bản làm việc")
        self.ws_create_button.clicked.connect(self._on_create_workspace)
        self.ws_preview_button = QPushButton("2. Xem trước 2 chiều")
        self.ws_preview_button.clicked.connect(self._on_preview_workspace)
        self.ws_commit_button = QPushButton("3. Xác nhận đồng bộ")
        self.ws_commit_button.setEnabled(False)
        self.ws_commit_button.clicked.connect(self._on_commit_workspace)
        self.ws_recover_button = QPushButton("Khôi phục giao dịch")
        self.ws_recover_button.clicked.connect(self._on_recover_workspace)
        for btn in (
            self.ws_create_button, self.ws_preview_button,
            self.ws_commit_button, self.ws_recover_button,
        ):
            ws_buttons.addWidget(btn)
        ws_layout.addLayout(ws_buttons)
        self.ws_status = QLabel("Chưa tạo workspace. Tạo bản làm việc để biên tập trực tiếp trong QGIS.")
        self.ws_status.setWordWrap(True)
        ws_layout.addWidget(self.ws_status)
        root_layout.addWidget(workspace_group)

        self.status_label = QLabel(
            "Ready. Enter connection details and load authorized layers."
        )
        self.status_label.setWordWrap(True)
        root_layout.addWidget(self.status_label)

        footer_row = QHBoxLayout()
        footer_row.addStretch(1)
        self.close_button = QPushButton("Close / Đóng")
        self.close_button.clicked.connect(self.close)
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
        self.pairing_code_input.clear()
        self._on_load_layers()

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
                raise WorkspaceConflict(
                    "Hãy lưu và kết thúc chế độ chỉnh sửa lớp QGIS rồi mới xem trước."
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
    """QGIS 3 plugin controller registering the OpenVietGIS connector action."""

    def __init__(self, iface: Any) -> None:
        self.iface = iface
        self.action: QAction | None = None
        self.dialog: OpenVietGisConnectorDialog | None = None

    def initGui(self) -> None:
        self.action = QAction(ACTION_LABEL, self.iface.mainWindow())
        self.action.setStatusTip(
            "Connect to OpenVietGIS for SHA-256 GeoPackage exchange and "
            "reviewed manual QGIS bidirectional sync via R006 changesets"
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
