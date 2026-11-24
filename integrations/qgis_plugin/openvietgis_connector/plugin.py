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

import base64
import contextlib
import ctypes
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any
from uuid import uuid4

from qgis.core import (
    QgsFeature,
    QgsField,
    QgsGeometry,
    QgsJsonUtils,
    QgsProject,
    QgsVectorLayer,
)
from qgis.PyQt.QtCore import QSettings, Qt, QThread, QTimer, QVariant, pyqtSignal
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
    USER_AGENT,
    ApiError,
    OpenVietGisConfig,
    OpenVietGisError,
    QgisLayerInfo,
    SyncPreviewResult,
    build_integration_events_url,
    decode_pairing_code,
    download_layer_geopackage,
    fetch_qgis_profile,
    preview_layer_geopackage_sync,
    push_layer_geopackage_sync,
)
from .managed_client import (
    ManagedSessionInfo,
    bootstrap_managed_layer,
    build_managed_events_url,
    commit_managed_changes,
    create_managed_session,
    fetch_managed_delta,
    list_desktop_commands,
    respond_desktop_command,
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
ACTION_LABEL = "Kết nối OpenVietGIS"


def _dpapi_protect(secret: str) -> str:
    """Encrypt secret for the current Windows user via DPAPI; falls back to base64 on POSIX."""
    raw = secret.encode("utf-8")
    if sys.platform == "win32":
        class _Blob(ctypes.Structure):
            _fields_ = [("cbData", ctypes.c_uint32), ("pbData", ctypes.POINTER(ctypes.c_char))]

        in_blob = _Blob(len(raw), ctypes.create_string_buffer(raw, len(raw)))
        out_blob = _Blob()
        crypt32 = ctypes.windll.crypt32  # type: ignore[attr-defined]
        kernel32 = ctypes.windll.kernel32  # type: ignore[attr-defined]
        if crypt32.CryptProtectData(
            ctypes.byref(in_blob), None, None, None, None, 0, ctypes.byref(out_blob)
        ):
            try:
                enc = ctypes.string_at(out_blob.pbData, out_blob.cbData)
                return "dpapi:" + base64.b64encode(enc).decode("ascii")
            finally:
                kernel32.LocalFree(out_blob.pbData)
    return "b64:" + base64.b64encode(raw).decode("ascii")


def _dpapi_unprotect(stored: str) -> str:
    """Decrypt secret previously protected by _dpapi_protect."""
    if not stored:
        return ""
    if stored.startswith("dpapi:") and sys.platform == "win32":
        class _Blob(ctypes.Structure):
            _fields_ = [("cbData", ctypes.c_uint32), ("pbData", ctypes.POINTER(ctypes.c_char))]

        enc = base64.b64decode(stored[6:])
        in_blob = _Blob(len(enc), ctypes.create_string_buffer(enc, len(enc)))
        out_blob = _Blob()
        crypt32 = ctypes.windll.crypt32  # type: ignore[attr-defined]
        kernel32 = ctypes.windll.kernel32  # type: ignore[attr-defined]
        if crypt32.CryptUnprotectData(
            ctypes.byref(in_blob), None, None, None, None, 0, ctypes.byref(out_blob)
        ):
            try:
                return ctypes.string_at(out_blob.pbData, out_blob.cbData).decode("utf-8")
            finally:
                kernel32.LocalFree(out_blob.pbData)
        return ""
    if stored.startswith("b64:"):
        return base64.b64decode(stored[4:]).decode("utf-8")
    return ""


class _RealtimeEventWorker(QThread):
    """Background SSE listener receiving PostgreSQL NOTIFY spatial_change events."""

    spatial_changed = pyqtSignal()
    status_changed = pyqtSignal(str)

    def __init__(self, config: OpenVietGisConfig, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._config = config
        self._stop_requested = False
        self._active_response: Any = None

    def stop(self) -> None:
        self._stop_requested = True
        resp = self._active_response
        self._active_response = None
        if resp is not None:
            with contextlib.suppress(OSError, ValueError):
                resp.close()

    def run(self) -> None:
        url = build_integration_events_url(self._config)
        while not self._stop_requested:
            req = urllib.request.Request(
                url,
                headers={
                    "Authorization": f"Bearer {self._config.bearer_token}",
                    "Accept": "text/event-stream",
                    "Cache-Control": "no-cache",
                    "User-Agent": USER_AGENT,
                },
                method="GET",
            )
            try:
                resp = urllib.request.urlopen(req, timeout=65.0)
                self._active_response = resp
                self.status_changed.emit("🟢 Realtime: Đã kết nối (chờ thay đổi từ Web/nhóm)")
                current_event = ""
                while not self._stop_requested:
                    raw_line = resp.readline()
                    if not raw_line:
                        break
                    line = raw_line.decode("utf-8", errors="ignore").strip()
                    if line.startswith("event:"):
                        current_event = line.split(":", 1)[1].strip()
                    elif line.startswith("data:") and current_event == "spatial_change":
                        self.spatial_changed.emit()
                        current_event = ""
                    elif not line:
                        current_event = ""
            except urllib.error.HTTPError as exc:
                if exc.code in (401, 403):
                    self.status_changed.emit("🔴 Realtime: Hết phiên hoặc không đủ quyền.")
                    break
                self.status_changed.emit("🟡 Realtime: Đang kết nối lại...")
            except (urllib.error.URLError, OSError, TimeoutError):
                if not self._stop_requested:
                    self.status_changed.emit("🟡 Realtime: Mất kết nối, đang thử lại...")
            finally:
                resp = self._active_response
                self._active_response = None
                if resp is not None:
                    with contextlib.suppress(OSError, ValueError):
                        resp.close()
            if not self._stop_requested:
                for _ in range(30):
                    if self._stop_requested:
                        break
                    time.sleep(0.1)


class _ManagedRealtimeWorker(QThread):
    """R029 SSE listener: delta wakeups, session revocation, and AI desktop commands."""

    spatial_changed = pyqtSignal()
    session_revoked = pyqtSignal()
    desktop_command = pyqtSignal()
    status_changed = pyqtSignal(str)

    def __init__(
        self,
        config: OpenVietGisConfig,
        session_id: str,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._config = config
        self._session_id = session_id
        self._stop_requested = False
        self._active_response: Any = None

    def stop(self) -> None:
        self._stop_requested = True
        resp = self._active_response
        self._active_response = None
        if resp is not None:
            with contextlib.suppress(OSError, ValueError):
                resp.close()

    def run(self) -> None:
        url = build_managed_events_url(self._config, self._session_id)
        while not self._stop_requested:
            req = urllib.request.Request(
                url,
                headers={
                    "Authorization": f"Bearer {self._config.bearer_token}",
                    "Accept": "text/event-stream",
                    "Cache-Control": "no-cache",
                    "User-Agent": USER_AGENT,
                },
                method="GET",
            )
            try:
                resp = urllib.request.urlopen(req, timeout=65.0)
                self._active_response = resp
                self.status_changed.emit("🟢 Đã kết nối · Đang nhận cập nhật từ Web")
                current_event = ""
                while not self._stop_requested:
                    raw_line = resp.readline()
                    if not raw_line:
                        break
                    line = raw_line.decode("utf-8", errors="ignore").strip()
                    if line.startswith("event:"):
                        current_event = line.split(":", 1)[1].strip()
                    elif line.startswith("data:"):
                        if current_event == "spatial_change":
                            self.spatial_changed.emit()
                        elif current_event == "session_revoked":
                            self.session_revoked.emit()
                            return
                        elif current_event == "desktop_command":
                            self.desktop_command.emit()
                        current_event = ""
                    elif not line:
                        current_event = ""
            except urllib.error.HTTPError as exc:
                if exc.code in (401, 403):
                    self.session_revoked.emit()
                    break
                self.status_changed.emit("🟡 Đang kết nối lại...")
            except (urllib.error.URLError, OSError, TimeoutError):
                if not self._stop_requested:
                    self.status_changed.emit("🟡 Mất kết nối · Đang thử lại...")
            finally:
                resp = self._active_response
                self._active_response = None
                if resp is not None:
                    with contextlib.suppress(OSError, ValueError):
                        resp.close()
            if not self._stop_requested:
                for _ in range(30):
                    if self._stop_requested:
                        break
                    time.sleep(0.1)


class OpenVietGisConnectorDialog(QDialog):
    """Dialog for read-only GeoPackage download and controlled two-step R006 sync."""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("OpenVietGIS · Kết nối QGIS")
        self.setMinimumSize(580, 490)

        self._layers: list[QgisLayerInfo] = []
        self._sync_push_enabled: bool = False
        self._pinned_revisions: dict[str, tuple[str, str]] = {}
        self._pending_preview: SyncPreviewResult | None = None
        self._pending_preview_context: tuple[str, str, str, str] | None = None
        self._workspace_root: Path | None = None
        self._workspace_preview: WorkspaceSyncPreview | None = None
        self._workspace_map_layer_id: str | None = None
        self._active_workspaces: dict[str, tuple[Path, str]] = {}
        self._preferred_layer_id: str | None = None
        self._preferred_binding_id: str | None = None
        self._auto_sync_busy: bool = False
        self._sync_in_progress: bool = False
        self._realtime_worker: _RealtimeEventWorker | None = None
        self._managed_session: ManagedSessionInfo | None = None
        self._managed_cursor: int = 0
        self._managed_layers: dict[str, dict[str, Any]] = {}
        self._managed_worker: _ManagedRealtimeWorker | None = None
        self._managed_delta_debounce = QTimer(self)
        self._managed_delta_debounce.setSingleShot(True)
        self._managed_delta_debounce.setInterval(300)
        self._managed_delta_debounce.timeout.connect(self._on_managed_fetch_delta)
        self._realtime_debounce = QTimer(self)
        self._realtime_debounce.setSingleShot(True)
        self._realtime_debounce.setInterval(500)
        self._realtime_debounce.timeout.connect(self._on_realtime_pull_all)
        self._auto_sync_timer = QTimer(self)
        self._auto_sync_timer.setInterval(15_000)
        self._auto_sync_timer.timeout.connect(self._on_auto_sync_tick)
        self._build_ui()
        if os.environ.get("QT_QPA_PLATFORM") != "offscreen":
            QTimer.singleShot(250, self._try_auto_reconnect)

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
        pair_group = QGroupBox("Kết nối")
        pair_layout = QVBoxLayout(pair_group)
        pairing_row = QHBoxLayout()
        self.pairing_code_input = QLineEdit()
        self.pairing_code_input.setEchoMode(QLineEdit.Password)
        self.pairing_code_input.setPlaceholderText(
            "Dán mã kết nối từ OpenVietGIS Web"
        )
        self.apply_pairing_button = QPushButton("Kết nối")
        self.apply_pairing_button.setStyleSheet(
            "background-color: #1d4ed8; color: #ffffff; font-weight: bold; "
            "padding: 5px 12px; border-radius: 4px;"
        )
        self.apply_pairing_button.clicked.connect(self._on_apply_pairing_code)
        pairing_row.addWidget(self.pairing_code_input)
        pairing_row.addWidget(self.apply_pairing_button)
        pair_layout.addLayout(pairing_row)

        conn_bar = QHBoxLayout()
        self.remember_session_checkbox = QCheckBox("Ghi nhớ & tự kết nối lại")
        self.remember_session_checkbox.setChecked(True)
        self.load_layers_button = QPushButton("↻ Tải lại lớp")
        self.load_layers_button.clicked.connect(self._on_load_layers)
        self.logout_session_button = QPushButton("Đăng xuất")
        self.logout_session_button.clicked.connect(self._on_clear_saved_session)
        self.manual_conn_toggle = QPushButton("Thiết lập thủ công ▾")
        self.manual_conn_toggle.setCheckable(True)
        self.manual_conn_toggle.toggled.connect(self._on_toggle_manual_conn)
        conn_bar.addWidget(self.remember_session_checkbox)
        conn_bar.addWidget(self.load_layers_button)
        conn_bar.addWidget(self.logout_session_button)
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

        # 2. Layer Picker (multi-layer checkboxes, wide readable table)
        layers_group = QGroupBox("Lớp bản đồ")
        layers_layout = QVBoxLayout(layers_group)
        select_bar = QHBoxLayout()
        self.select_all_layers_btn = QPushButton("☑ Chọn tất cả")
        self.select_all_layers_btn.clicked.connect(lambda: self._set_all_layers_checked(True))
        self.clear_all_layers_btn = QPushButton("☐ Bỏ chọn")
        self.clear_all_layers_btn.clicked.connect(lambda: self._set_all_layers_checked(False))
        select_bar.addWidget(self.select_all_layers_btn)
        select_bar.addWidget(self.clear_all_layers_btn)
        select_bar.addStretch(1)
        layers_layout.addLayout(select_bar)

        self.layers_table = QTableWidget(0, 4)
        self.layers_table.setMinimumHeight(158)
        self.layers_table.setHorizontalHeaderLabels(
            ["Tên lớp (tích chọn)", "Loại", "CRS", "Chế độ"]
        )
        self.layers_table.setSelectionBehavior(QAbstractItemView.SelectRows)
        self.layers_table.setSelectionMode(QAbstractItemView.ExtendedSelection)
        self.layers_table.setEditTriggers(QAbstractItemView.NoEditTriggers)
        self.layers_table.itemSelectionChanged.connect(self._on_layer_selection_changed)
        header = self.layers_table.horizontalHeader()
        header.setSectionResizeMode(0, QHeaderView.Stretch)
        header.setSectionResizeMode(1, QHeaderView.ResizeToContents)
        header.setSectionResizeMode(2, QHeaderView.ResizeToContents)
        header.setSectionResizeMode(3, QHeaderView.ResizeToContents)
        layers_layout.addWidget(self.layers_table)
        root_layout.addWidget(layers_group)

        # 3. Primary Two-Way Sync Box (Realtime Workspace Client + 1-click sync!)
        workspace_group = QGroupBox("Làm việc với bản đồ")
        ws_layout = QVBoxLayout(workspace_group)

        r029_row = QHBoxLayout()
        self.r029_open_button = QPushButton("Mở bản đồ trong QGIS")
        self.r029_open_button.setStyleSheet(
            "background-color: #7c3aed; color: #ffffff; font-weight: bold; "
            "padding: 8px 12px; border-radius: 4px;"
        )
        self.r029_open_button.clicked.connect(self._on_open_r029_managed)

        self.r029_save_button = QPushButton("Lưu thay đổi lên Web")
        self.r029_save_button.setEnabled(False)
        self.r029_save_button.setStyleSheet(
            "background-color: #0f766e; color: #ffffff; font-weight: bold; "
            "padding: 8px 12px; border-radius: 4px;"
        )
        self.r029_save_button.clicked.connect(self._on_commit_r029_managed)
        r029_row.addWidget(self.r029_open_button)
        r029_row.addWidget(self.r029_save_button)
        ws_layout.addLayout(r029_row)

        self.realtime_status = QLabel("Chưa mở bản đồ")
        self.realtime_status.setStyleSheet("color: #475569; font-size: 11px;")
        ws_layout.addWidget(self.realtime_status)

        self.ws_status = QLabel(
            "Dán mã kết nối, chọn lớp rồi mở bản đồ. "
            "Sau khi chỉnh sửa, dùng nút ‘Lưu thay đổi lên Web’ tại đây."
        )
        self.ws_status.setWordWrap(True)
        self.ws_status.setStyleSheet(
            "background-color: #f0f9ff; border: 1px solid #bae6fd; "
            "padding: 6px; border-radius: 4px;"
        )
        ws_layout.addWidget(self.ws_status)
        root_layout.addWidget(workspace_group)

        self.status_label = QLabel("Dán mã kết nối từ Web để bắt đầu.")
        self.status_label.setWordWrap(True)
        root_layout.addWidget(self.status_label)

        # 4. Collapsible Advanced Section (hidden by default to keep UI clean)
        self.advanced_toggle = QPushButton("Tùy chọn & khôi phục ▾")
        self.advanced_toggle.setCheckable(True)
        self.advanced_toggle.toggled.connect(self._on_toggle_advanced)
        root_layout.addWidget(self.advanced_toggle)

        self.advanced_box = QWidget()
        adv_layout = QVBoxLayout(self.advanced_box)
        adv_layout.setContentsMargins(0, 4, 0, 0)

        adv_quick_row = QHBoxLayout()
        self.download_open_button = QPushButton("Mở bản chỉ xem")
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

        legacy_group = QGroupBox("Đồng bộ bằng GeoPackage (tùy chọn)")
        legacy_layout = QVBoxLayout(legacy_group)
        legacy_help = QLabel(
            "Chỉ sử dụng cho workspace GeoPackage cũ hoặc cần làm việc với tệp. "
            "Không sử dụng đồng thời với lớp đang mở trực tiếp trên Web."
        )
        legacy_help.setWordWrap(True)
        legacy_layout.addWidget(legacy_help)
        open_row = QHBoxLayout()
        self.ws_open_all_button = QPushButton("Mở tất cả lớp qua GeoPackage")
        self.ws_open_all_button.setStyleSheet(
            "background-color: #4f46e5; color: #ffffff; font-weight: bold; "
            "padding: 7px 12px; border-radius: 4px;"
        )
        self.ws_open_all_button.clicked.connect(self._on_open_full_workspace)

        self.ws_create_button = QPushButton("Mở lớp đã chọn qua GeoPackage")
        self.ws_create_button.setStyleSheet(
            "background-color: #0284c7; color: #ffffff; font-weight: bold; "
            "padding: 7px 12px; border-radius: 4px;"
        )
        self.ws_create_button.clicked.connect(self._on_create_workspace)
        open_row.addWidget(self.ws_open_all_button)
        open_row.addWidget(self.ws_create_button)

        self.ws_sync_now_button = QPushButton("Đồng bộ GeoPackage ngay")
        self.ws_sync_now_button.setStyleSheet(
            "background-color: #16a34a; color: #ffffff; font-weight: bold; "
            "padding: 8px 12px; border-radius: 4px;"
        )
        self.ws_sync_now_button.clicked.connect(self._on_sync_now_one_click)

        self.realtime_checkbox = QCheckBox(
            "Nhận thay đổi từ Web và đồng bộ tệp sau khi lưu QGIS"
        )
        self.realtime_checkbox.setChecked(False)
        self.realtime_checkbox.toggled.connect(self._on_toggle_realtime)

        self.allow_deletes_checkbox = QCheckBox(
            "Cho phép đồng bộ xóa đối tượng (có hoàn tác qua Lịch sử phiên bản trên Web)"
        )
        self.allow_deletes_checkbox.setChecked(False)

        self.auto_sync_checkbox = QCheckBox(
            "Tự kiểm tra thay đổi GeoPackage mỗi 15 giây"
        )
        self.auto_sync_checkbox.setChecked(False)
        self.auto_sync_checkbox.toggled.connect(self._on_toggle_auto_sync)

        ws_sync_row = QHBoxLayout()
        self.ws_preview_button = QPushButton("Xem thay đổi")
        self.ws_preview_button.setStyleSheet(
            "padding: 5px 8px; border-radius: 4px;"
        )
        self.ws_preview_button.clicked.connect(self._on_preview_workspace)

        self.ws_commit_button = QPushButton("Xác nhận đồng bộ")
        self.ws_commit_button.setEnabled(False)
        self.ws_commit_button.setStyleSheet(
            "padding: 5px 8px; border-radius: 4px;"
        )
        self.ws_commit_button.clicked.connect(self._on_commit_workspace)

        ws_sync_row.addWidget(self.ws_preview_button)
        ws_sync_row.addWidget(self.ws_commit_button)
        legacy_layout.addLayout(open_row)
        legacy_layout.addWidget(self.ws_sync_now_button)
        legacy_layout.addWidget(self.realtime_checkbox)
        legacy_layout.addWidget(self.allow_deletes_checkbox)
        legacy_layout.addWidget(self.auto_sync_checkbox)
        legacy_layout.addLayout(ws_sync_row)

        adv_layout.addWidget(legacy_group)

        sync_group = QGroupBox("Đồng bộ tệp GeoPackage riêng")
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
        self.preview_sync_button = QPushButton("Xem thay đổi trong tệp")
        self.preview_sync_button.setEnabled(False)
        self.preview_sync_button.clicked.connect(self._on_preview_sync)
        self.push_sync_button = QPushButton("Xác nhận đồng bộ tệp")
        self.push_sync_button.setEnabled(False)
        self.push_sync_button.clicked.connect(self._on_push_sync)
        sync_buttons_row.addWidget(self.preview_sync_button)
        sync_buttons_row.addWidget(self.push_sync_button)
        sync_layout.addLayout(sync_buttons_row)

        self.sync_preview_label = QLabel("Chưa kiểm tra tệp GeoPackage.")
        self.sync_preview_label.setWordWrap(True)
        sync_layout.addWidget(self.sync_preview_label)
        adv_layout.addWidget(sync_group)

        self.advanced_box.setVisible(False)
        root_layout.addWidget(self.advanced_box)
        root_layout.addStretch(1)

    def _on_toggle_manual_conn(self, checked: bool) -> None:
        self.manual_conn_box.setVisible(checked)
        self.manual_conn_toggle.setText("Thiết lập thủ công ▴" if checked else "Thiết lập thủ công ▾")

    def _on_toggle_advanced(self, checked: bool) -> None:
        self.advanced_box.setVisible(checked)
        self.advanced_toggle.setText(
            "Tùy chọn & khôi phục ▴" if checked else "Tùy chọn & khôi phục ▾"
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
        if any(
            isinstance(layer := QgsProject.instance().mapLayer(info["map_layer_id"]), QgsVectorLayer)
            and layer.isModified()
            for info in self._managed_layers.values()
        ):
            QMessageBox.warning(
                self, "Còn chỉnh sửa chưa lưu",
                "Hãy lưu lên Web hoặc xuất bản sao dữ liệu từ QGIS "
                "trước khi thay mã kết nối."
            )
            return
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
        self._preferred_binding_id = parsed.get("binding_id")
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
            self.status_label.setText("Đang tải danh sách lớp...")
            profile = fetch_qgis_profile(config)
        except OpenVietGisError as exc:
            self.status_label.setText(f"Không thể kết nối: {exc}")
            QMessageBox.warning(self, "OpenVietGIS Connector", str(exc))
            return

        self._layers = list(profile.layers)
        self._sync_push_enabled = bool(profile.sync_push_enabled)
        if self.remember_session_checkbox.isChecked():
            self._save_local_session()
        self._populate_layers_table()
        scoped_note = "scoped ACL" if profile.scoped_access else "quyền dự án"
        sync_note = "Đã có quyền đồng bộ" if self._sync_push_enabled else "Chỉ xem"
        self.status_label.setText(
            f"Đã tải {len(self._layers)} lớp ({scoped_note} · {sync_note})."
        )
        self._update_action_button_states()

    def _set_all_layers_checked(self, checked: bool) -> None:
        state = Qt.Checked if checked else Qt.Unchecked
        for row_idx in range(self.layers_table.rowCount()):
            item = self.layers_table.item(row_idx, 0)
            if item is not None:
                item.setCheckState(state)

    def _checked_or_selected_layers(self) -> list[QgisLayerInfo]:
        checked: list[QgisLayerInfo] = []
        for row_idx, layer in enumerate(self._layers):
            item = self.layers_table.item(row_idx, 0)
            if item is not None and item.checkState() == Qt.Checked:
                checked.append(layer)
        if checked:
            return checked
        selected_rows = sorted({idx.row() for idx in self.layers_table.selectedIndexes()})
        if selected_rows:
            return [self._layers[r] for r in selected_rows if 0 <= r < len(self._layers)]
        single = self._selected_layer()
        return [single] if single is not None else []

    def _populate_layers_table(self) -> None:
        self.layers_table.setRowCount(len(self._layers))
        for row_idx, layer in enumerate(self._layers):
            name_item = QTableWidgetItem(layer.name)
            name_item.setData(Qt.UserRole, layer.layer_id)
            name_item.setFlags(name_item.flags() | Qt.ItemIsUserCheckable)
            name_item.setCheckState(
                Qt.Checked if layer.layer_id == self._preferred_layer_id else Qt.Unchecked
            )

            geom_item = QTableWidgetItem(f"{layer.geometry_family} ({layer.dimensions})")
            crs_item = QTableWidgetItem(layer.crs_display)

            if layer.export_permitted and self._sync_push_enabled:
                export_text = "Có thể chỉnh sửa"
            elif layer.export_permitted:
                export_text = "Chỉ xem"
            elif not layer.crs_explicit:
                export_text = "Thiếu CRS rõ ràng"
            else:
                export_text = "Không có quyền"
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

    def _open_working_layer(
        self,
        *,
        display_name: str = "OpenVietGIS · GeoPackage cục bộ",
        layer_key: str | None = None,
    ) -> None:
        workfile = self._workspace_file()
        current = self._workspace_layer()
        if current is not None:
            if current.isEditable():
                raise WorkspaceConflict("Hãy Lưu/Kết thúc chế độ chỉnh sửa QGIS trước.")
            QgsProject.instance().removeMapLayer(current.id())
        vector = QgsVectorLayer(str(workfile), display_name, "ogr")
        if not vector.isValid():
            raise WorkspaceConflict("Không thể mở GeoPackage của workspace bằng OGR.")
        QgsProject.instance().addMapLayer(vector)
        self._workspace_map_layer_id = vector.id()
        self.ws_path_input.setText(str(self._workspace_root))
        if layer_key is not None:
            vector.afterCommitChanges.connect(
                lambda lid=layer_key: self._on_layer_committed_realtime(lid)
            )

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

    def _layer_display_name(self, layer_id: str) -> str:
        for lyr in self._layers:
            if lyr.layer_id == layer_id:
                return f"OpenVietGIS · {lyr.name}"
        return "OpenVietGIS · GeoPackage cục bộ"

    def _open_targets_into_workspace(
        self,
        targets: list[QgisLayerInfo],
        *,
        enable_realtime: bool = False,
    ) -> None:
        if self._managed_layers:
            QMessageBox.warning(
                self, "Đang mở bản đồ trực tiếp",
                "Không thể đồng thời đồng bộ GeoPackage với lớp đang mở trực tiếp. "
                "Hãy lưu thay đổi và kết thúc phiên trước khi đổi chế độ."
            )
            return
        if not targets:
            QMessageBox.warning(
                self, "OpenVietGIS", "Hãy tích chọn ít nhất 1 lớp hợp lệ (có CRS) để mở sửa."
            )
            return
        try:
            config = self._build_config()
            base_dir = Path(self.download_dir_input.text().strip() or str(Path.home())).resolve()
            opened_names: list[str] = []
            self._sync_in_progress = True
            try:
                for target in targets:
                    existing_root = base_dir / f"ovg_qgis_{target.layer_id}"
                    if existing_root.is_dir() and (existing_root / "workspace.json").is_file():
                        root = existing_root
                    else:
                        root = initialize_workspace(config, target.layer_id, base_dir)
                    prev = self._active_workspaces.get(target.layer_id)
                    self._workspace_root = root
                    self._workspace_map_layer_id = prev[1] if prev else None
                    self._workspace_preview = None
                    self.ws_commit_button.setEnabled(False)
                    self._open_working_layer(
                        display_name=f"OpenVietGIS · {target.name}",
                        layer_key=target.layer_id,
                    )
                    opened = self._workspace_layer()
                    if opened is not None:
                        if not opened.isEditable():
                            opened.startEditing()
                        self._active_workspaces[target.layer_id] = (root, opened.id())
                    opened_names.append(target.name)
            finally:
                self._sync_in_progress = False
            if enable_realtime and not self.realtime_checkbox.isChecked():
                self.realtime_checkbox.setChecked(True)
            self.ws_status.setText(
                f"Đã mở {len(opened_names)} lớp ({', '.join(opened_names[:3])}"
                + ("..." if len(opened_names) > 3 else "")
                + "). Sửa trên bản đồ rồi nhấn Ctrl+S (Realtime) hoặc bấm '⇄ Đồng bộ 2 chiều ngay'."
            )
        except (OpenVietGisError, WorkspaceConflict, OSError, ValueError) as exc:
            QMessageBox.warning(self, "OpenVietGIS", str(exc))
            self.ws_status.setText(f"Lỗi mở lớp: {exc}")

    def _on_open_full_workspace(self) -> None:
        self._set_all_layers_checked(True)
        targets = [
            lyr for lyr in self._layers
            if lyr.export_permitted and lyr.crs_explicit
        ]
        self._open_targets_into_workspace(targets, enable_realtime=True)

    def _on_create_workspace(self) -> None:
        targets = [
            lyr for lyr in self._checked_or_selected_layers()
            if lyr.export_permitted and lyr.crs_explicit
        ]
        self._open_targets_into_workspace(targets, enable_realtime=False)

    def _stop_realtime_worker(self) -> None:
        worker = self._realtime_worker
        self._realtime_worker = None
        if worker is not None:
            worker.stop()
            worker.wait(1500)

    def _on_toggle_realtime(self, checked: bool) -> None:
        if checked:
            self._stop_realtime_worker()
            try:
                config = self._build_config()
            except OpenVietGisError as exc:
                self.realtime_checkbox.setChecked(False)
                self.realtime_status.setText(f"🔴 Realtime: {exc}")
                return
            worker = _RealtimeEventWorker(config, self)
            worker.spatial_changed.connect(self._on_realtime_signal)
            worker.status_changed.connect(self.realtime_status.setText)
            self._realtime_worker = worker
            worker.start()
            self.realtime_status.setText("🟡 Realtime: Đang kết nối tới máy chủ...")
        else:
            self._stop_realtime_worker()
            self.realtime_status.setText("⚪ Realtime: Đã tắt")

    def _on_realtime_signal(self) -> None:
        if not self._sync_in_progress:
            self._realtime_debounce.start()

    def _on_layer_committed_realtime(self, layer_id: str) -> None:
        if self._sync_in_progress or not self.realtime_checkbox.isChecked():
            return
        entry = self._active_workspaces.get(layer_id)
        if entry is None:
            return
        QTimer.singleShot(150, lambda lid=layer_id: self._sync_single_layer_realtime(lid))

    def _sync_single_layer_realtime(self, layer_id: str) -> None:
        if self._sync_in_progress:
            return
        entry = self._active_workspaces.get(layer_id)
        if entry is None:
            return
        root, map_lid = entry
        allow_deletes = self.allow_deletes_checkbox.isChecked()
        self._sync_in_progress = True
        try:
            config = self._build_config()
            self._workspace_root = root
            self._workspace_map_layer_id = map_lid
            layer = self._workspace_layer()
            was_editable = False
            if layer is not None and layer.isEditable():
                if layer.isModified():
                    return
                was_editable = True
                if not layer.commitChanges():
                    return
            preview = preview_workspace(config, root, allow_deletes=allow_deletes)
            if preview.conflicts:
                self.ws_status.setText(
                    "⚠ Xung đột Realtime: " + "; ".join(preview.conflicts[:3])
                )
                return
            if not preview.to_server and not preview.to_local:
                if was_editable and layer is not None and not layer.isEditable():
                    layer.startEditing()
                return
            self._require_workspace_layer_closed()
            receipt = commit_workspace(
                config,
                preview,
                confirmed_digest=preview.digest,
                allow_deletes=allow_deletes,
            )
            self._open_working_layer(
                display_name=self._layer_display_name(layer_id),
                layer_key=layer_id,
            )
            reopened = self._workspace_layer()
            if reopened is not None:
                if was_editable and not reopened.isEditable():
                    reopened.startEditing()
                self._active_workspaces[layer_id] = (root, reopened.id())
            self.ws_status.setText(
                f"⚡ Realtime đồng bộ xong — Đẩy lên Web: {receipt['pushed']} · "
                f"Nhận về QGIS: {receipt['pulled']}."
            )
        except (OpenVietGisError, WorkspaceConflict, OSError, ValueError) as exc:
            self.ws_status.setText(f"Realtime tạm dừng: {exc}")
        finally:
            self._sync_in_progress = False

    def _on_realtime_pull_all(self) -> None:
        if self._sync_in_progress or not self._active_workspaces:
            return
        for lid in list(self._active_workspaces):
            entry = self._active_workspaces.get(lid)
            if entry is None:
                continue
            _, map_lid = entry
            layer = QgsProject.instance().mapLayer(map_lid)
            if isinstance(layer, QgsVectorLayer) and layer.isEditable() and layer.isModified():
                self.ws_status.setText(
                    "🔔 Có cập nhật mới từ Web/nhóm — nhấn Ctrl+S trên QGIS để đồng bộ ngay."
                )
                continue
            self._sync_single_layer_realtime(lid)

    def _on_open_workspace(self) -> None:
        chosen = QFileDialog.getExistingDirectory(
            self, "Chọn thư mục GeoPackage",
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
            QMessageBox.warning(self, "Workspace GeoPackage", str(exc))
            self.ws_status.setText(f"Không thể mở workspace: {exc}")

    def _on_toggle_auto_sync(self, checked: bool) -> None:
        if checked:
            self._auto_sync_timer.start()
            self.ws_status.setText(
                "Đã bật Tự động đồng bộ 2 chiều (15 giây/lần). "
                "Lưu chỉnh sửa (Ctrl+S) trên QGIS để tự đẩy lên Web, hoặc sửa trên Web để tự kéo về."
            )
        else:
            self._auto_sync_timer.stop()
            self.ws_status.setText("Đã tắt tự động đồng bộ. Bạn có thể bấm '⇄ Đồng bộ 2 chiều ngay'.")

    def _on_auto_sync_tick(self) -> None:
        if self._auto_sync_busy or self._sync_in_progress or self._workspace_root is None:
            return
        layer = self._workspace_layer()
        # Do not interrupt while user is actively editing in QGIS edit mode.
        if layer is not None and layer.isEditable():
            return
        allow_deletes = self.allow_deletes_checkbox.isChecked()
        self._auto_sync_busy = True
        self._sync_in_progress = True
        try:
            config = self._build_config()
            preview = preview_workspace(
                config, self._workspace_file().parent, allow_deletes=allow_deletes
            )
            if preview.conflicts:
                self.auto_sync_checkbox.setChecked(False)
                self.ws_status.setText(
                    "Tạm dừng tự động đồng bộ do CÓ XUNG ĐỘT: "
                    + "; ".join(preview.conflicts[:3])
                )
                return
            if not preview.to_server and not preview.to_local:
                return
            self._require_workspace_layer_closed()
            receipt = commit_workspace(
                config,
                preview,
                confirmed_digest=preview.digest,
                allow_deletes=allow_deletes,
            )
            self._open_working_layer()
            self.ws_status.setText(
                f"✓ Tự động đồng bộ xong — Đẩy lên Web: {receipt['pushed']} · "
                f"Kéo về QGIS: {receipt['pulled']}."
            )
        except (OpenVietGisError, WorkspaceConflict, OSError, ValueError) as exc:
            blocked = (
                isinstance(exc, ApiError) and exc.status_code in (401, 403)
                or isinstance(exc, WorkspaceConflict) and (
                    "recover pending work" in str(exc)
                    or "HTTP 403" in str(exc)
                )
            )
            if blocked:
                # Never retry a refused or indeterminate server write on a timer.
                self.auto_sync_checkbox.setChecked(False)
                self.ws_status.setText(
                    f"🔴 Đã tắt tự đồng bộ để bảo vệ dữ liệu: {exc}. "
                    "Kiểm tra mã ghép nối/quyền project.edit + GIS ACL. "
                    "Nếu có pending, mở Công cụ nâng cao → Khôi phục giao dịch; "
                    "không xóa thư mục workspace hay tạo lại lớp."
                )
            else:
                self.ws_status.setText(f"Tự động đồng bộ gặp lỗi: {exc}")
        finally:
            self._auto_sync_busy = False
            self._sync_in_progress = False

    def _on_sync_now_one_click(self) -> None:
        items: list[tuple[str, Path, str | None]] = [
            (lid, root, map_lid) for lid, (root, map_lid) in self._active_workspaces.items()
        ]
        if not items and self._workspace_root is not None:
            items = [("current", self._workspace_root, self._workspace_map_layer_id)]
        if not items:
            QMessageBox.warning(
                self, "OpenVietGIS", "Hãy bấm '🚀 Mở toàn bộ Workspace' hoặc '1. Mở lớp' trước."
            )
            return
        allow_deletes = self.allow_deletes_checkbox.isChecked()
        total_pushed = 0
        total_pulled = 0
        conflicts_found: list[str] = []
        self._sync_in_progress = True
        try:
            config = self._build_config()
            for lid, root, map_lid in items:
                self._workspace_root = root
                self._workspace_map_layer_id = map_lid
                layer = self._workspace_layer()
                if layer is not None and layer.isEditable() and not layer.commitChanges():
                    raise WorkspaceConflict(
                        "Không thể tự lưu chỉnh sửa trên QGIS. Hãy bấm Save và tắt Edit Mode."
                    )
                preview = preview_workspace(config, root, allow_deletes=allow_deletes)
                if preview.conflicts:
                    conflicts_found.extend(preview.conflicts[:3])
                    continue
                if not preview.to_server and not preview.to_local:
                    continue
                self._require_workspace_layer_closed()
                receipt = commit_workspace(
                    config,
                    preview,
                    confirmed_digest=preview.digest,
                    allow_deletes=allow_deletes,
                )
                total_pushed += int(receipt["pushed"])
                total_pulled += int(receipt["pulled"])
                self._open_working_layer(
                    display_name=self._layer_display_name(lid),
                    layer_key=None if lid == "current" else lid,
                )
                reopened = self._workspace_layer()
                if reopened is not None:
                    if not reopened.isEditable():
                        reopened.startEditing()
                    if lid != "current":
                        self._active_workspaces[lid] = (root, reopened.id())
            self._workspace_preview = None
            self.ws_commit_button.setEnabled(False)
            if conflicts_found:
                self.ws_status.setText("CÓ XUNG ĐỘT: " + "; ".join(conflicts_found[:5]))
                QMessageBox.warning(
                    self, "Phát hiện xung đột", "\n".join(conflicts_found[:5])
                )
            elif total_pushed == 0 and total_pulled == 0:
                self.ws_status.setText(
                    f"Đã kiểm tra {len(items)} lớp — khớp 100%, chưa có chỉnh sửa mới ở QGIS hay Web."
                )
            else:
                self.ws_status.setText(
                    f"✓ Đã đồng bộ 2 chiều ({len(items)} lớp)! "
                    f"Đẩy lên Web: {total_pushed} · Kéo về QGIS: {total_pulled}."
                )
        except (OpenVietGisError, WorkspaceConflict, OSError, ValueError) as exc:
            self.ws_status.setText(f"Lỗi đồng bộ: {exc}")
            QMessageBox.warning(self, "Lỗi đồng bộ OpenVietGIS", str(exc))
        finally:
            self._sync_in_progress = False

    def _on_preview_workspace(self) -> None:
        allow_deletes = self.allow_deletes_checkbox.isChecked()
        try:
            config = self._build_config()
            layer = self._workspace_layer()
            if layer is not None and layer.isEditable() and not layer.commitChanges():
                raise WorkspaceConflict(
                    "Không thể tự lưu chỉnh sửa trên QGIS. Hãy bấm Save và tắt Edit Mode."
                )
            preview = preview_workspace(
                config, self._workspace_file().parent, allow_deletes=allow_deletes
            )
            self._workspace_preview = preview
            self.ws_commit_button.setEnabled(not bool(preview.conflicts))
            delete_note = (
                "Có đồng bộ xóa (hoàn tác được trên Web)."
                if allow_deletes
                else "Không tự động xóa."
            )
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
                    f"Digest {preview.digest[:16]}... {delete_note}"
                )
        except (OpenVietGisError, WorkspaceConflict, OSError, ValueError) as exc:
            self._workspace_preview = None
            self.ws_commit_button.setEnabled(False)
            self.ws_status.setText(f"Lỗi xem trước: {exc}")
            QMessageBox.warning(self, "Xem thay đổi GeoPackage", str(exc))

    def _on_commit_workspace(self) -> None:
        preview = self._workspace_preview
        if preview is None or preview.conflicts:
            self.ws_commit_button.setEnabled(False)
            return
        allow_deletes = self.allow_deletes_checkbox.isChecked()
        confirmation = QMessageBox.question(
            self, "Xác nhận đồng bộ hai chiều",
            f"Đẩy lên máy chủ {len(preview.to_server)} thay đổi, "
            f"nhận về QGIS {len(preview.to_local)} thay đổi?\n"
            f"Digest: {preview.digest[:16]}...\n"
            "Đã lưu và tắt chế độ chỉnh sửa trong QGIS? "
            + (
                "Đồng bộ xóa ĐANG BẬT (mọi lệnh xóa đều hoàn tác được qua Lịch sử phiên bản)."
                if allow_deletes
                else "Máy chủ sẽ lưu lịch sử thay đổi; mặc định không cho phép xóa."
            ),
            QMessageBox.Yes | QMessageBox.No, QMessageBox.No,
        )
        if confirmation != QMessageBox.Yes:
            return
        self.ws_commit_button.setEnabled(False)
        self._sync_in_progress = True
        try:
            config = self._build_config()
            # Close the OGR file handle before atomic file replacement.
            self._require_workspace_layer_closed()
            receipt = commit_workspace(
                config,
                preview,
                confirmed_digest=preview.digest,
                allow_deletes=allow_deletes,
            )
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
            QMessageBox.warning(self, "Đồng bộ GeoPackage", str(exc))
        finally:
            self._sync_in_progress = False
            if self._workspace_root is not None:
                try:
                    self._open_working_layer()
                except (WorkspaceConflict, OSError) as exc:
                    self.ws_status.setText(
                        f"Workspace đã xử lý nhưng QGIS chưa thể mở lại tệp: {exc}"
                    )

    def _on_recover_workspace(self) -> None:
        if self._workspace_root is None:
            chosen = QFileDialog.getExistingDirectory(
                self, "Chọn thư mục workspace cần khôi phục",
                self.download_dir_input.text().strip() or str(Path.home())
            )
            if not chosen:
                return
            self._workspace_root = Path(chosen).resolve()
            self.ws_path_input.setText(str(self._workspace_root))
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
            QMessageBox.warning(self, "Khôi phục giao dịch", str(exc))
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
            self.status_label.setText(f"Không thể xem thay đổi: {exc}")
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
            f"Xác nhận lưu các thay đổi lên lớp '{layer.name}' "
            f"(EPSG:{preview.stored_crs_code})?\n\n"
            f"- Thêm mới (create_count): {preview.create_count}\n"
            f"- Cập nhật (update_count): {preview.update_count}\n"
            f"- Không đổi (unchanged_count): {preview.unchanged_count}\n"
            f"- Đối tượng thiếu trong tệp cục bộ (omitted_server_feature_count): "
            f"{preview.omitted_server_feature_count} "
            f"(KHÔNG bị xóa — deletions blocked by default)\n"
            f"- diff_sha256: {preview.diff_sha256[:16]}...\n\n"
            "Thao tác sẽ được ghi vào lịch sử máy chủ."
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
            self.status_label.setText(f"Không thể lưu thay đổi: {exc}")
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
            f"Đã lưu {result.operation_count} thay đổi lên máy chủ. "
            f"Mã giao dịch: {result.changeset_id}."
        )
        QMessageBox.information(
            self,
            "OpenVietGIS Sync Push Complete",
            f"Đã lưu thay đổi thành công!\n\n"
            f"Changeset ID: {result.changeset_id}\n"
            f"Operations: {result.operation_count} "
            f"(Create={result.create_count}, Update={result.update_count})\n"
            f"Omitted Server Features: {result.omitted_server_feature_count} "
            f"(Deletions Blocked: {result.deletions_blocked})\n"
            f"Resulting Content Revision: {result.resulting_content_revision[:16]}...\n"
            f"Audit Event ID: {result.audit_event_id}",
        )

    def _save_local_session(self) -> None:
        settings = QSettings("OpenVietGIS", "Connector")
        settings.setValue("server_url", self.server_url_input.text().strip())
        settings.setValue("organization_id", self.org_id_input.text().strip())
        settings.setValue("project_id", self.project_id_input.text().strip())
        settings.setValue("binding_id", self._preferred_binding_id or "")
        settings.setValue("preferred_layer_id", self._preferred_layer_id or "")
        settings.setValue("allow_localhost", self.allow_localhost_checkbox.isChecked())
        settings.setValue("download_dir", self.download_dir_input.text().strip())
        token = self.bearer_key_input.text().strip()
        if token:
            settings.setValue("protected_token", _dpapi_protect(token))

    def _on_clear_saved_session(self) -> None:
        for info in self._managed_layers.values():
            layer = QgsProject.instance().mapLayer(info["map_layer_id"])
            if isinstance(layer, QgsVectorLayer) and layer.isModified():
                QMessageBox.warning(
                    self, "Còn chỉnh sửa chưa lưu",
                    "Hãy lưu lên Web hoặc xuất bản sao lớp từ QGIS "
                    "trước khi đăng xuất để tránh mất dữ liệu."
                )
                return
        self._stop_managed_worker()
        self._stop_realtime_worker()
        self._auto_sync_timer.stop()
        self._preferred_binding_id = None
        self._preferred_layer_id = None
        settings = QSettings("OpenVietGIS", "Connector")
        settings.clear()
        self.bearer_key_input.clear()
        self.pairing_code_input.clear()
        self._layers = []
        self.layers_table.setRowCount(0)
        self.r029_save_button.setEnabled(False)
        self.realtime_status.setText("⚪ Đã đăng xuất & xóa phiên đã lưu trên máy.")
        self.status_label.setText("Đã xóa phiên kết nối. Dán mã ghép nối mới để đăng nhập lại.")

    def _try_auto_reconnect(self) -> None:
        settings = QSettings("OpenVietGIS", "Connector")
        server_url = str(settings.value("server_url", "") or "").strip()
        org_id = str(settings.value("organization_id", "") or "").strip()
        project_id = str(settings.value("project_id", "") or "").strip()
        protected_token = str(settings.value("protected_token", "") or "").strip()
        if not (server_url and org_id and project_id and protected_token):
            return
        token = _dpapi_unprotect(protected_token)
        if not token:
            return
        self.server_url_input.setText(server_url)
        self.org_id_input.setText(org_id)
        self.project_id_input.setText(project_id)
        self._preferred_binding_id = str(settings.value("binding_id", "") or "") or None
        self._preferred_layer_id = str(settings.value("preferred_layer_id", "") or "") or None
        self.allow_localhost_checkbox.setChecked(
            str(settings.value("allow_localhost", "false")).lower() == "true"
        )
        saved_dir = str(settings.value("download_dir", "") or "").strip()
        if saved_dir:
            self.download_dir_input.setText(saved_dir)
        self.bearer_key_input.setText(token)
        try:
            config = self._build_config()
            profile = fetch_qgis_profile(config)
            self._layers = list(profile.layers)
            self._sync_push_enabled = bool(profile.sync_push_enabled)
            self._populate_layers_table()
            self._update_action_button_states()
            self.status_label.setText(
                f"🟢 Đã tự động kết nối lại ({len(self._layers)} lớp sẵn sàng)."
            )
            self.ws_status.setText(
                "Đã kết nối lại. Chọn lớp rồi bấm ‘Mở bản đồ trong QGIS’."
            )
        except OpenVietGisError:
            self.status_label.setText(
                "Phiên đã lưu đã hết hạn hoặc bị thu hồi — hãy dán mã kết nối mới."
            )

    def _stop_managed_worker(self) -> None:
        worker = self._managed_worker
        self._managed_worker = None
        if worker is not None:
            worker.stop()
            worker.wait(1500)

    def _qgis_field_type(self, data_type: str) -> Any:
        dt = data_type.lower()
        if "int" in dt:
            return QVariant.LongLong
        if "float" in dt or "double" in dt or "numeric" in dt or "real" in dt:
            return QVariant.Double
        if "bool" in dt:
            return QVariant.Bool
        return QVariant.String

    def _geojson_to_qgs_geometry(self, geom_obj: dict[str, Any] | None) -> QgsGeometry:
        if not geom_obj:
            return QgsGeometry()
        fc = json.dumps({"type": "Feature", "geometry": geom_obj, "properties": {}})
        feats = QgsJsonUtils.stringToFeatureList(fc, QgsVectorLayer("Point", "t", "memory").fields())
        if feats:
            return QgsGeometry(feats[0].geometry())
        return QgsGeometry()

    def _on_open_r029_managed(self) -> None:
        if self._managed_layers:
            QMessageBox.information(
                self, "Bản đồ đang mở",
                "Các lớp đã mở trong QGIS. Hãy lưu thay đổi tại công cụ trước "
                "khi ngắt kết nối hoặc mở lại dự án."
            )
            return
        targets = [
            lyr for lyr in self._checked_or_selected_layers()
            if lyr.export_permitted and lyr.crs_explicit
        ]
        if not targets:
            targets = [
                lyr for lyr in self._layers if lyr.export_permitted and lyr.crs_explicit
            ]
        if not targets:
            QMessageBox.warning(
                self, "Kết nối bản đồ", "Không có lớp bản đồ hợp lệ (có CRS) để mở."
            )
            return
        # A previously interrupted GeoPackage sync needs explicit recovery.
        # Never hide its unsent edits behind a fresh in-memory managed layer.
        base_dir = Path(self.download_dir_input.text().strip() or str(Path.home()))
        for target in targets:
            root = base_dir / f"ovg_qgis_{target.layer_id}"
            state_path = root / "workspace.json"
            if not state_path.is_file():
                continue
            try:
                state = json.loads(state_path.read_text(encoding="utf-8"))
            except (OSError, ValueError):
                QMessageBox.warning(
                    self, "Workspace cần kiểm tra",
                    "Có workspace GeoPackage cũ không đọc được trạng thái. "
                    "Hãy kiểm tra bản sao cục bộ trước khi mở chế độ trực tiếp."
                )
                return
            if state.get("pending"):
                self._workspace_root = root
                self.ws_path_input.setText(str(root))
                self.advanced_toggle.setChecked(True)
                QMessageBox.warning(
                    self, "Cần khôi phục đồng bộ cũ",
                    "Có thay đổi GeoPackage chưa xác nhận. Chọn ‘Khôi phục giao dịch’ "
                    "trong ‘Tùy chọn & khôi phục’ trước khi mở bản đồ trực tiếp. "
                    "Không xóa dữ liệu trong thư mục workspace."
                )
                return
        try:
            config = self._build_config()
            self._stop_managed_worker()
            sess = create_managed_session(
                config, device_label="QGIS Desktop", client_version="0.9.3",
                binding_id=self._preferred_binding_id,
            )
            self._managed_session = sess
            self._managed_cursor = sess.current_cursor
            self._managed_layers.clear()

            opened_names: list[str] = []
            for target in targets:
                offset = 0
                all_feats: list[dict[str, Any]] = []
                first_page = None
                while True:
                    page = bootstrap_managed_layer(
                        config,
                        layer_id=target.layer_id,
                        session_id=sess.id,
                        limit=1000,
                        offset=offset,
                    )
                    if first_page is None:
                        first_page = page
                    all_feats.extend(page.features)
                    if not page.has_more or page.next_offset is None:
                        break
                    offset = page.next_offset
                if first_page is None or not first_page.crs_code:
                    continue
                self._managed_cursor = max(self._managed_cursor, first_page.cursor)
                geom_type = first_page.geometry_family or "Point"
                uri = f"{geom_type}?crs=EPSG:{first_page.crs_code}"
                vlayer = QgsVectorLayer(uri, f"OpenVietGIS · {first_page.name}", "memory")
                prov = vlayer.dataProvider()
                qfields = [QgsField("ovg_feature_id", QVariant.String)]
                field_names: list[str] = []
                for fld in first_page.fields:
                    fname = str(fld["name"])
                    field_names.append(fname)
                    qfields.append(QgsField(fname, self._qgis_field_type(str(fld["data_type"]))))
                prov.addAttributes(qfields)
                vlayer.updateFields()

                qfeatures: list[QgsFeature] = []
                for item in all_feats:
                    qf = QgsFeature(vlayer.fields())
                    qf.setGeometry(self._geojson_to_qgs_geometry(item.get("geometry")))
                    props = item.get("properties") or {}
                    attrs = [str(item["feature_id"]), *(props.get(fn) for fn in field_names)]
                    qf.setAttributes(attrs)
                    qfeatures.append(qf)
                if qfeatures:
                    prov.addFeatures(qfeatures)
                vlayer.updateExtents()
                if not sess.can_edit:
                    vlayer.setReadOnly(True)
                QgsProject.instance().addMapLayer(vlayer)
                if sess.can_edit:
                    vlayer.startEditing()
                self._managed_layers[first_page.layer_id] = {
                    "map_layer_id": vlayer.id(),
                    "source_id": first_page.source_id,
                    "content_revision": first_page.content_revision,
                    "schema_revision": first_page.schema_revision,
                    "field_names": field_names,
                }
                opened_names.append(first_page.name)

            self.r029_save_button.setEnabled(sess.can_edit and bool(self._managed_layers))
            worker = _ManagedRealtimeWorker(config, sess.id, self)
            worker.spatial_changed.connect(lambda: self._managed_delta_debounce.start())
            worker.session_revoked.connect(self._on_managed_session_revoked)
            worker.desktop_command.connect(self._on_managed_desktop_command)
            worker.status_changed.connect(self.realtime_status.setText)
            self._managed_worker = worker
            worker.start()
            mode_txt = "Có quyền chỉnh sửa" if sess.can_edit else "Chỉ xem"
            self.ws_status.setText(
                f"Đã mở {len(opened_names)} lớp ({mode_txt}). "
                "Chỉnh sửa trong QGIS rồi bấm ‘Lưu thay đổi lên Web’ ở công cụ này."
            )
        except (OpenVietGisError, OSError, ValueError) as exc:
            reason = str(exc)
            if isinstance(exc, ApiError) and exc.status_code == 403:
                reason = (
                    "HTTP 403: Server từ chối tạo phiên cho dự án này.\n"
                    "Hãy mở đúng Project trên Web, cấp quyền project.read "
                    "(và project.edit để ghi), kiểm tra GIS ACL, rồi tạo mã ghép "
                    "nối QGIS MỚI từ chính Project đó. Không dùng mã đã lưu của dự án khác.\n"
                    "Không có thao tác ghi nào được thực hiện."
                )
            QMessageBox.warning(self, "Kết nối bản đồ", reason)
            self.ws_status.setText(f"Không thể mở bản đồ: {reason}")

    def _on_managed_session_revoked(self) -> None:
        self._stop_managed_worker()
        self.r029_save_button.setEnabled(False)
        has_unsent = False
        for info in self._managed_layers.values():
            layer = QgsProject.instance().mapLayer(info["map_layer_id"])
            if isinstance(layer, QgsVectorLayer):
                if layer.isEditable() and layer.isModified():
                    has_unsent = True
                    # Revoke write access but preserve the local edit buffer.
                    # QGIS can still export it to a separate file for recovery.
                    self.ws_status.setText(
                        "Phiên kết nối đã hết hạn. Bản chỉnh sửa chưa gửi vẫn nằm "
                        "trong QGIS; hãy xuất bản sao trước khi đóng QGIS."
                    )
                    continue
                if layer.isEditable():
                    layer.rollBack()
                layer.setReadOnly(True)
        self.realtime_status.setText("🔒 Phiên đã hết hạn hoặc bị thu hồi.")
        if not has_unsent:
            self.ws_status.setText(
                "Phiên đã hết hạn hoặc bị thu hồi. Hãy kết nối lại với dự án."
            )

    def _on_managed_fetch_delta(self) -> None:
        if self._managed_session is None or not self._managed_layers:
            return
        # Never acknowledge a server cursor while QGIS has uncommitted changes.
        # The same delta must remain available after the user saves their work.
        for info in self._managed_layers.values():
            local_layer = QgsProject.instance().mapLayer(info["map_layer_id"])
            if isinstance(local_layer, QgsVectorLayer) and local_layer.isModified():
                self.ws_status.setText(
                    "Có thay đổi mới từ Web. Lưu bản chỉnh sửa trong công cụ "
                    "trước khi tải cập nhật để tránh xung đột."
                )
                return
        try:
            config = self._build_config()
            delta = fetch_managed_delta(
                config,
                session_id=self._managed_session.id,
                cursor=self._managed_cursor,
            )
            if delta.reset_required:
                self.ws_status.setText(
                    "🔔 Quyền hoặc cấu trúc lớp đã thay đổi. Lưu bản nháp trước khi mở lại."
                )
                return
            applied_count = 0
            for change in delta.changes:
                lid = str(change["layer_id"])
                info = self._managed_layers.get(lid)
                if info is None:
                    continue
                layer = QgsProject.instance().mapLayer(info["map_layer_id"])
                if not isinstance(layer, QgsVectorLayer):
                    continue
                if layer.isEditable() and layer.isModified():
                    self.ws_status.setText(
                        "🔔 Có cập nhật từ Web. Lưu chỉnh sửa đang làm rồi nhận cập nhật mới."
                    )
                    continue
                was_edit = layer.isEditable()
                if was_edit:
                    layer.commitChanges()
                prov = layer.dataProvider()
                id_to_fid: dict[str, int] = {}
                for feat in layer.getFeatures():
                    id_to_fid[str(feat["ovg_feature_id"])] = feat.id()
                removals = [id_to_fid[str(oid)] for oid in change.get("removals", []) if str(oid) in id_to_fid]
                if removals:
                    prov.deleteFeatures(removals)
                    applied_count += len(removals)
                field_names = info["field_names"]
                to_add: list[QgsFeature] = []
                geom_updates: dict[int, QgsGeometry] = {}
                attr_updates: dict[int, dict[int, Any]] = {}
                for item in change.get("upserts", []):
                    oid = str(item["feature_id"])
                    qgeom = self._geojson_to_qgs_geometry(item.get("geometry"))
                    props = item.get("properties") or {}
                    if oid in id_to_fid:
                        fid = id_to_fid[oid]
                        geom_updates[fid] = qgeom
                        attr_updates[fid] = {
                            idx + 1: props.get(fn) for idx, fn in enumerate(field_names)
                        }
                    else:
                        qf = QgsFeature(layer.fields())
                        qf.setGeometry(qgeom)
                        qf.setAttributes([oid, *(props.get(fn) for fn in field_names)])
                        to_add.append(qf)
                    applied_count += 1
                if geom_updates:
                    prov.changeGeometryValues(geom_updates)
                if attr_updates:
                    prov.changeAttributeValues(attr_updates)
                if to_add:
                    prov.addFeatures(to_add)
                layer.updateExtents()
                layer.triggerRepaint()
                info["content_revision"] = str(change["content_revision"])
                if was_edit:
                    layer.startEditing()
            self._managed_cursor = delta.next_cursor
            if applied_count > 0:
                self.ws_status.setText(
                    f"Đã nhận {applied_count} đối tượng cập nhật từ Web."
                )
        except OpenVietGisError as exc:
            if getattr(exc, "status_code", None) in (401, 403):
                self._on_managed_session_revoked()
            else:
                self.ws_status.setText(f"Tạm dừng nhận cập nhật: {exc}")

    def _on_commit_r029_managed(self) -> None:
        if self._managed_session is None or not self._managed_layers:
            return
        allow_deletes = self.allow_deletes_checkbox.isChecked()
        total_ops = 0
        try:
            config = self._build_config()
            for info in self._managed_layers.values():
                layer = QgsProject.instance().mapLayer(info["map_layer_id"])
                if not isinstance(layer, QgsVectorLayer) or not layer.isEditable():
                    continue
                buf = layer.editBuffer()
                if buf is None:
                    continue
                field_names: list[str] = info["field_names"]
                ops: list[dict[str, Any]] = []
                prov_by_fid = {f.id(): f for f in layer.dataProvider().getFeatures()}

                for fid, feat in buf.addedFeatures().items():
                    oid = str(feat["ovg_feature_id"] or "").strip() or str(uuid4())
                    layer.changeAttributeValue(fid, 0, oid)
                    geom_dict = json.loads(feat.geometry().asJson()) if not feat.geometry().isEmpty() else None
                    props = {fn: feat[fn] for fn in field_names}
                    ops.append({
                        "op_type": "create",
                        "feature_id": oid,
                        "geometry": geom_dict,
                        "properties": props,
                    })

                changed_fids = set(buf.changedGeometries().keys()) | set(buf.changedAttributeValues().keys())
                for fid in changed_fids:
                    if fid in buf.addedFeatures():
                        continue
                    live_feat = layer.getFeature(fid)
                    orig_feat = prov_by_fid.get(fid)
                    oid = str((orig_feat["ovg_feature_id"] if orig_feat else live_feat["ovg_feature_id"]) or "").strip()
                    if not oid:
                        continue
                    geom_dict = json.loads(live_feat.geometry().asJson()) if not live_feat.geometry().isEmpty() else None
                    props = {fn: live_feat[fn] for fn in field_names}
                    ops.append({
                        "op_type": "update",
                        "feature_id": oid,
                        "geometry": geom_dict,
                        "properties": props,
                    })

                for fid in buf.deletedFeatureIds():
                    orig_feat = prov_by_fid.get(fid)
                    if orig_feat is None:
                        continue
                    oid = str(orig_feat["ovg_feature_id"] or "").strip()
                    if oid:
                        ops.append({"op_type": "delete", "feature_id": oid})

                if not ops:
                    continue
                # Server-First: ONLY commit local memory buffer AFTER durable server receipt!
                receipt = commit_managed_changes(
                    config,
                    session_id=self._managed_session.id,
                    source_id=info["source_id"],
                    base_content_revision=info["content_revision"],
                    base_schema_revision=info["schema_revision"],
                    operation_id=str(uuid4()),
                    operations=ops,
                    summary="QGIS R029 Managed Realtime Edit",
                    allow_deletes=allow_deletes,
                )
                layer.commitChanges()
                layer.startEditing()
                info["content_revision"] = receipt.resulting_content_revision
                self._managed_cursor = max(self._managed_cursor, receipt.event_cursor)
                total_ops += receipt.operation_count

            if total_ops == 0:
                self.ws_status.setText("Không có chỉnh sửa mới cần lưu.")
            else:
                self.ws_status.setText(
                    f"✓ Đã lưu thành công {total_ops} thay đổi lên Web."
                )
                self._managed_delta_debounce.start()
        except OpenVietGisError as exc:
            self.ws_status.setText(
                f"⚠ Server từ chối/chưa xác nhận lưu (bản nháp trên QGIS được giữ nguyên): {exc}"
            )
            QMessageBox.warning(self, "Lưu thay đổi lên Web", str(exc))

    def _on_managed_desktop_command(self) -> None:
        if self._managed_session is None:
            return
        try:
            config = self._build_config()
            cmds = list_desktop_commands(config, session_id=self._managed_session.id, status="pending")
            for cmd in cmds:
                tool = str(cmd["tool_name"])
                args = dict(cmd.get("arguments") or {})
                choice = QMessageBox.question(
                    self,
                    "Xác nhận yêu cầu từ AI",
                    f"AI / Web yêu cầu chạy công cụ QGIS được cấp phép:\n\n"
                    f"- Công cụ: {tool}\n"
                    f"- Tham số: {json.dumps(args, ensure_ascii=False)}\n\n"
                    "Bạn có đồng ý cho phép thực thi không?",
                    QMessageBox.Yes | QMessageBox.No,
                    QMessageBox.No,
                )
                if choice != QMessageBox.Yes:
                    respond_desktop_command(
                        config,
                        session_id=self._managed_session.id,
                        command_id=str(cmd["id"]),
                        status="rejected",
                        result={"reason": "user_declined"},
                    )
                    continue
                res_payload: dict[str, Any] = {"ok": True}
                if tool == "inspect_project":
                    res_payload["layers"] = [
                        {"layer_id": lid, "name": QgsProject.instance().mapLayer(i["map_layer_id"]).name()}
                        for lid, i in self._managed_layers.items()
                        if QgsProject.instance().mapLayer(i["map_layer_id"]) is not None
                    ]
                elif tool == "inspect_layer":
                    lid = str(args.get("layer_id") or "")
                    info = self._managed_layers.get(lid)
                    layer = QgsProject.instance().mapLayer(info["map_layer_id"]) if info else None
                    res_payload["feature_count"] = layer.featureCount() if isinstance(layer, QgsVectorLayer) else 0
                elif tool == "select_features":
                    lid = str(args.get("layer_id") or "")
                    target_ids = {str(x) for x in args.get("feature_ids", [])}
                    info = self._managed_layers.get(lid)
                    layer = QgsProject.instance().mapLayer(info["map_layer_id"]) if info else None
                    if isinstance(layer, QgsVectorLayer):
                        fids = [f.id() for f in layer.getFeatures() if str(f["ovg_feature_id"]) in target_ids]
                        layer.selectByIds(fids)
                        res_payload["selected_count"] = len(fids)
                respond_desktop_command(
                    config,
                    session_id=self._managed_session.id,
                    command_id=str(cmd["id"]),
                    status="completed",
                    result=res_payload,
                )
        except OpenVietGisError as exc:
            self.ws_status.setText(f"Không thể thực hiện yêu cầu từ Web: {exc}")

    def closeEvent(self, event: Any) -> None:
        """Clear sensitive bearer token and stop realtime & auto-sync timers when closing."""
        self._stop_managed_worker()
        self._managed_delta_debounce.stop()
        self._stop_realtime_worker()
        self._realtime_debounce.stop()
        self._auto_sync_timer.stop()
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
            self.dock.setMinimumWidth(620)
            self.dock.resize(660, 740)
            self.iface.addDockWidget(Qt.RightDockWidgetArea, self.dock)
        self.dock.show()
        self.dock.raise_()