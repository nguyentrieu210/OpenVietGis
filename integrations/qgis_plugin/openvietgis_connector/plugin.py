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
import webbrowser
from pathlib import Path
from typing import Any
from uuid import uuid4

from qgis.core import (
    QgsApplication,
    QgsFeature,
    QgsField,
    QgsGeometry,
    QgsJsonUtils,
    QgsProject,
    QgsVariantUtils,
    QgsVectorLayer,
)
from qgis.PyQt.QtCore import (
    QDate,
    QDateTime,
    QSettings,
    Qt,
    QThread,
    QTime,
    QTimer,
    QVariant,
    pyqtSignal,
)
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

from .browser_login import BrowserLogin
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
from .drafts import draft_path, merge_snapshots, read_draft, write_draft
from .managed_client import (
    ManagedSessionInfo,
    bootstrap_managed_layer,
    build_managed_events_url,
    commit_managed_changes,
    create_managed_session,
    diff_managed_snapshots,
    fetch_managed_delta,
    get_managed_commit_receipt,
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
    """Encrypt secret for the current Windows user via DPAPI; returns empty if secure persistence is unavailable."""
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
    return ""


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

        self._connected = False
        self._host_window = parent.window() if parent is not None else self
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
        self._managed_config: OpenVietGisConfig | None = None
        self._managed_poll = QTimer(self)
        self._managed_poll.setInterval(10000)
        self._managed_poll.timeout.connect(self._managed_tick)
        self._draft_timer = QTimer(self)
        self._draft_timer.setInterval(3000)
        self._draft_timer.timeout.connect(self._checkpoint_managed_drafts)
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
        self._browser_login: BrowserLogin | None = None
        self._browser_deadline = 0.0
        self._browser_timer = QTimer(self)
        self._browser_timer.setInterval(300)
        self._browser_timer.timeout.connect(self._poll_browser_login)
        self._build_ui()
        self._set_connection_state(False)
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
        self.pairing_box = QWidget()
        pairing_row = QHBoxLayout(self.pairing_box)
        pairing_row.setContentsMargins(0, 0, 0, 0)
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
        self.connection_summary = QLabel()
        self.connection_summary.setTextFormat(Qt.PlainText)
        self.connection_summary.setWordWrap(True)
        self.connection_summary.setStyleSheet("background:#e9f7ef;color:#145c39;border:1px solid #a4d8b8;padding:10px;border-radius:5px;font-weight:bold;")
        pair_layout.addWidget(self.connection_summary)
        pair_layout.addWidget(self.pairing_box)
        self.browser_login_button = QPushButton("Đăng nhập bằng tài khoản OpenVietGIS")
        self.browser_login_button.setStyleSheet("background:#155d72;color:white;padding:10px;border-radius:4px;font-weight:bold;")
        self.browser_login_button.clicked.connect(self._start_browser_login)
        pair_layout.addWidget(self.browser_login_button)

        conn_bar = QHBoxLayout()
        self.remember_session_checkbox = QCheckBox("Ghi nhớ & tự kết nối lại")
        self.remember_session_checkbox.setChecked(True)
        self.load_layers_button = QPushButton("↻ Tải lại lớp")
        self.load_layers_button.clicked.connect(self._on_load_layers)
        self.switch_project_button = QPushButton("Đổi dự án")
        self.switch_project_button.clicked.connect(self._start_browser_login)
        self.logout_session_button = QPushButton("Đăng xuất")
        self.logout_session_button.clicked.connect(self._on_clear_saved_session)
        self.manual_conn_toggle = QPushButton("Thiết lập thủ công ▾")
        self.manual_conn_toggle.setCheckable(True)
        self.manual_conn_toggle.toggled.connect(self._on_toggle_manual_conn)
        conn_bar.addWidget(self.remember_session_checkbox)
        conn_bar.addWidget(self.load_layers_button)
        conn_bar.addWidget(self.switch_project_button)
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
        self.layers_group = layers_group
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
        self.workspace_group = workspace_group
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
        self.managed_auto_save = QCheckBox("Tự lưu thay đổi lên Web (không tự xóa)")
        self.managed_auto_save.setChecked(False)
        ws_layout.addWidget(self.managed_auto_save)

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

        self.status_label = QLabel("Đăng nhập để mở bản đồ từ OpenVietGIS.")
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

    def _set_connection_state(self, connected: bool) -> None:
        self._connected = connected
        self.connection_summary.setVisible(connected)
        self.browser_login_button.setVisible(not connected)
        self.manual_conn_toggle.setVisible(not connected)
        self.load_layers_button.setVisible(connected)
        self.switch_project_button.setVisible(connected)
        self.logout_session_button.setVisible(connected)
        self.remember_session_checkbox.setVisible(not connected)
        self.manual_conn_toggle.setChecked(False)
        self.pairing_box.setVisible(False)
        self.manual_conn_box.setVisible(False)
        self.layers_group.setVisible(connected or bool(self._managed_layers))
        self.workspace_group.setVisible(connected or bool(self._managed_layers))
        self.r029_open_button.setEnabled(connected)
        if connected:
            role = "Có quyền đồng bộ" if self._sync_push_enabled else "Chỉ xem"
            self.connection_summary.setText(
                f"● Đã kết nối OpenVietGIS\n{len(self._layers)} lớp bản đồ · {role}\n{self.server_url_input.text()}"
            )
        else:
            self.connection_summary.clear()

    def _on_toggle_manual_conn(self, checked: bool) -> None:
        self.pairing_box.setVisible(checked and not self._connected)
        self.load_layers_button.setVisible(self._connected or checked)
        self.manual_conn_box.setVisible(checked and not self._connected)
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

    def _stop_browser_login(self) -> None:
        self._browser_timer.stop()
        if self._browser_login is not None:
            self._browser_login.close()
            self._browser_login = None
        self.browser_login_button.setEnabled(True)
        self.switch_project_button.setEnabled(True)

    def _start_browser_login(self) -> None:
        self._stop_browser_login()
        if any(
            isinstance(layer := QgsProject.instance().mapLayer(info["map_layer_id"]), QgsVectorLayer)
            and self._managed_is_dirty(layer, info)
            for info in self._managed_layers.values()
        ):
            QMessageBox.warning(self, "Còn chỉnh sửa chưa lưu", "Hãy lưu bản nháp trước khi đổi phiên.")
            return
        try:
            self._browser_login = BrowserLogin(self.server_url_input.text().strip() or "https://openvietgis.cloud")
            self._browser_deadline = time.monotonic() + 300
            self.browser_login_button.setEnabled(False)
            self.switch_project_button.setEnabled(False)
            if not webbrowser.open(self._browser_login.url):
                raise OSError("Không mở được trình duyệt.")
            self.status_label.setText("Đăng nhập trên trình duyệt, chọn dự án và cấp quyền cho QGIS.")
            self._browser_timer.start()
        except (OSError, ValueError) as exc:
            self._stop_browser_login()
            self.status_label.setText(str(exc))

    def _poll_browser_login(self) -> None:
        if self._browser_login is None:
            return
        code = self._browser_login.take()
        if code is not None:
            flow = self._browser_login
            self._browser_timer.stop()
            success = False
            try:
                self.pairing_code_input.setText(code)
                success = self._on_apply_pairing_code()
            finally:
                flow.complete(success)
                self._stop_browser_login()
                self._return_to_qgis()

        elif time.monotonic() >= self._browser_deadline:
            self._stop_browser_login()
            self.status_label.setText("Hết thời gian đăng nhập. Hãy thử lại.")
            self._return_to_qgis()

    def _return_to_qgis(self) -> None:
        # Qt first; Windows may decline foreground activation by design.
        window = self._host_window
        if window.isMinimized():
            window.showNormal()
        window.raise_()
        window.activateWindow()
        if sys.platform == "win32":
            user32 = ctypes.windll.user32
            user32.SetForegroundWindow(ctypes.c_void_p(int(window.winId())))
        QgsApplication.alert(window, 4000)

    def _on_apply_pairing_code(self) -> bool:
        if any(
            isinstance(layer := QgsProject.instance().mapLayer(info["map_layer_id"]), QgsVectorLayer)
            and (layer.isModified() or self._managed_is_dirty(layer, info))
            for info in self._managed_layers.values()
        ):
            QMessageBox.warning(
                self, "Còn chỉnh sửa chưa lưu",
                "Hãy lưu lên Web hoặc xuất bản sao dữ liệu từ QGIS "
                "trước khi thay mã kết nối."
            )
            return False
        raw = self.pairing_code_input.text().strip()
        if not raw:
            QMessageBox.warning(
                self, "OpenVietGIS Connector", "Hãy dán mã ghép nối ovg-pair-v1:... trước."
            )
            return False
        try:
            parsed = decode_pairing_code(raw)
        except OpenVietGisError as exc:
            QMessageBox.warning(self, "Mã ghép nối không hợp lệ", str(exc))
            return False
        self._disconnect_managed()
        self._stop_realtime_worker()
        self._auto_sync_timer.stop()
        self._layers = []
        self._sync_push_enabled = False
        self.layers_table.setRowCount(0)
        self._set_connection_state(False)
        QSettings("OpenVietGIS", "Connector").remove("protected_token")
        self.server_url_input.setText(parsed["server_url"])
        self.org_id_input.setText(parsed["organization_id"])
        self.project_id_input.setText(parsed["project_id"])
        self.bearer_key_input.setText(parsed["bearer_token"])
        self._preferred_layer_id = parsed.get("layer_id")
        self._preferred_binding_id = parsed.get("binding_id")
        self.pairing_code_input.clear()
        if not self._on_load_layers():
            return False
        if self._preferred_layer_id:
            for row_idx, lyr in enumerate(self._layers):
                if lyr.layer_id == self._preferred_layer_id:
                    self.layers_table.selectRow(row_idx)
                    break
        return True

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

    def _on_load_layers(self) -> bool:
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
            if self._browser_login is None:
                QMessageBox.warning(self, "OpenVietGIS Connector", str(exc))
            if isinstance(exc, ApiError) and exc.status_code in (401, 403):
                if self._managed_session is not None:
                    self._on_managed_session_revoked()
                self._set_connection_state(False)
            return False

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
        self._set_connection_state(True)
        self.ws_status.setText("Đã kết nối. Chọn lớp rồi bấm ‘Mở bản đồ trong QGIS’.")
        return True

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
            protected = _dpapi_protect(token)
            if protected:
                settings.setValue("protected_token", protected)
            else:
                settings.remove("protected_token")

    def _disconnect_managed(self) -> None:
        self._draft_timer.stop()
        self._managed_poll.stop()
        self._managed_delta_debounce.stop()
        self.managed_auto_save.setChecked(False)
        self._stop_managed_worker()
        for info in self._managed_layers.values():
            layer = QgsProject.instance().mapLayer(info["map_layer_id"])
            if isinstance(layer, QgsVectorLayer):
                if layer.isEditable() and not layer.isModified():
                    layer.rollBack()
                layer.setReadOnly(True)
                if not layer.name().endswith(" · Đã ngắt kết nối"):
                    layer.setName(layer.name() + " · Đã ngắt kết nối")
        self._managed_layers.clear()
        self._managed_session = None
        self._managed_config = None
        self.r029_save_button.setEnabled(False)

    def _on_clear_saved_session(self) -> None:
        self._checkpoint_managed_drafts()
        for info in self._managed_layers.values():
            layer = QgsProject.instance().mapLayer(info["map_layer_id"])
            if isinstance(layer, QgsVectorLayer) and (layer.isModified() or self._managed_is_dirty(layer, info)):
                QMessageBox.warning(
                    self, "Còn chỉnh sửa chưa lưu",
                    "Hãy lưu lên Web hoặc xuất bản sao lớp từ QGIS "
                    "trước khi đăng xuất để tránh mất dữ liệu."
                )
                return
        self._stop_browser_login()
        self._disconnect_managed()
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
        self._set_connection_state(False)
        self.status_label.setText("Đã đăng xuất. Đăng nhập để kết nối dự án.")

    def _try_auto_reconnect(self) -> None:
        settings = QSettings("OpenVietGIS", "Connector")
        server_url = str(settings.value("server_url", "") or "").strip()
        org_id = str(settings.value("organization_id", "") or "").strip()
        project_id = str(settings.value("project_id", "") or "").strip()
        protected_token = str(settings.value("protected_token", "") or "").strip()
        if not (server_url and org_id and project_id and protected_token):
            return
        try:
            token = _dpapi_unprotect(protected_token)
        except (ValueError, OSError):
            token = ""
        if not token:
            settings.remove("protected_token")
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
            self._set_connection_state(True)
            self.status_label.setText(
                f"🟢 Đã tự động kết nối lại ({len(self._layers)} lớp sẵn sàng)."
            )
            self.ws_status.setText(
                "Đã kết nối lại. Chọn lớp rồi bấm ‘Mở bản đồ trong QGIS’."
            )
        except OpenVietGisError:
            self.status_label.setText(
                "Không thể khôi phục phiên đã lưu — hãy đăng nhập lại."
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
        if not self._connected:
            return
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
                config, device_label="QGIS Desktop", client_version="1.0.0",
                binding_id=self._preferred_binding_id,
            )
            self._managed_session = sess
            self._managed_config = config
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
                    elif (page.content_revision, page.schema_revision, page.permission_epoch) != (
                        first_page.content_revision, first_page.schema_revision, first_page.permission_epoch
                    ):
                        raise ApiError("Lớp thay đổi trong lúc tải; hãy mở lại để tránh mất dữ liệu.")
                    all_feats.extend(page.features)
                    if not page.has_more or page.next_offset is None:
                        break
                    offset = page.next_offset
                if first_page is None or not first_page.crs_code:
                    continue
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
                if not vlayer.isValid() or not prov.addAttributes(qfields):
                    raise ApiError("Không thể tạo lớp hoặc trường QGIS.")
                vlayer.updateFields()

                qfeatures: list[QgsFeature] = []
                for item in all_feats:
                    qf = QgsFeature(vlayer.fields())
                    qf.setGeometry(self._geojson_to_qgs_geometry(item.get("geometry")))
                    props = item.get("properties") or {}
                    attrs = [str(item["feature_id"]), *(props.get(fn) for fn in field_names)]
                    qf.setAttributes(attrs)
                    qfeatures.append(qf)
                if qfeatures and not prov.addFeatures(qfeatures)[0]:
                    raise ApiError("Không thể nạp đầy đủ đối tượng vào QGIS.")
                vlayer.updateExtents()
                if not sess.can_edit:
                    vlayer.setReadOnly(True)
                QgsProject.instance().addMapLayer(vlayer)
                if sess.can_edit:
                    vlayer.startEditing()
                vlayer.setCustomProperty("openvietgis/managed_layer", first_page.layer_id)
                self._managed_layers[first_page.layer_id] = {
                    "map_layer_id": vlayer.id(),
                    "source_id": first_page.source_id,
                    "content_revision": first_page.content_revision,
                    "schema_revision": first_page.schema_revision,
                    "field_names": field_names,
                    "baseline": self._managed_snapshot(vlayer, field_names),
                    "cursor": first_page.cursor,
                    "permission_epoch": first_page.permission_epoch,
                }
                self._restore_managed_draft(vlayer, self._managed_layers[first_page.layer_id], first_page.layer_id)
                opened_names.append(first_page.name)

            self.r029_save_button.setEnabled(sess.can_edit and bool(self._managed_layers))
            worker = _ManagedRealtimeWorker(config, sess.id, self)
            worker.spatial_changed.connect(lambda: self._managed_delta_debounce.start())
            worker.session_revoked.connect(self._on_managed_session_revoked)
            worker.desktop_command.connect(self._on_managed_desktop_command)
            worker.status_changed.connect(self.realtime_status.setText)
            self._managed_worker = worker
            worker.start()
            self._managed_poll.start()
            self._draft_timer.start()
            self._managed_delta_debounce.start()
            mode_txt = "Có quyền chỉnh sửa" if sess.can_edit else "Chỉ xem"
            self.ws_status.setText(
                f"Đã mở {len(opened_names)} lớp ({mode_txt}). "
                "Chỉnh sửa trong QGIS rồi bấm ‘Lưu thay đổi lên Web’ ở công cụ này."
            )
        except (OpenVietGisError, OSError, ValueError) as exc:
            self._disconnect_managed()
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
        self._checkpoint_managed_drafts()
        self._managed_poll.stop()
        self.managed_auto_save.setChecked(False)
        self._managed_session = None
        self._stop_managed_worker()
        self.r029_save_button.setEnabled(False)
        has_unsent = False
        for info in self._managed_layers.values():
            layer = QgsProject.instance().mapLayer(info["map_layer_id"])
            if isinstance(layer, QgsVectorLayer):
                if layer.isModified() or self._managed_is_dirty(layer, info):
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
        self._set_connection_state(False)
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
            if isinstance(local_layer, QgsVectorLayer) and (
                local_layer.isModified() or self._managed_is_dirty(local_layer, info)
            ):
                self.ws_status.setText(
                    "Có thay đổi mới từ Web. Lưu bản chỉnh sửa trong công cụ "
                    "trước khi tải cập nhật để tránh xung đột."
                )
                return
        try:
            config = self._managed_config
            if config is None:
                raise ApiError("Phiên kết nối không hợp lệ.")
            delta = fetch_managed_delta(
                config,
                session_id=self._managed_session.id,
                cursor=self._managed_cursor,
            )
            if delta.reset_required:
                self._on_managed_session_revoked()
                self.ws_status.setText(
                    "🔔 Quyền hoặc cấu trúc lớp đã thay đổi. Lưu bản nháp trước khi mở lại."
                )
                return
            applied_count = 0
            for change in delta.changes:
                lid = str(change["layer_id"])
                info = self._managed_layers.get(lid)
                if info is None or info.get("recovery_blocked") or int(change["cursor"]) <= info["cursor"]:
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
                if was_edit and not layer.commitChanges():
                    raise ApiError("Không thể kết thúc phiên sửa cục bộ.")
                prov = layer.dataProvider()
                id_to_fid: dict[str, int] = {}
                for feat in layer.getFeatures():
                    id_to_fid[str(feat["ovg_feature_id"])] = feat.id()
                removals = [id_to_fid[str(oid)] for oid in change.get("removals", []) if str(oid) in id_to_fid]
                if removals:
                    if not prov.deleteFeatures(removals):
                        raise ApiError("Không thể áp dụng xóa từ Web.")
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
                if geom_updates and not prov.changeGeometryValues(geom_updates):
                    raise ApiError("Không thể áp dụng hình học từ Web.")
                if attr_updates and not prov.changeAttributeValues(attr_updates):
                    raise ApiError("Không thể áp dụng thuộc tính từ Web.")
                if to_add and not prov.addFeatures(to_add)[0]:
                    raise ApiError("Không thể áp dụng đối tượng mới từ Web.")
                layer.updateExtents()
                layer.triggerRepaint()
                info["content_revision"] = str(change["content_revision"])
                info["baseline"] = self._managed_snapshot(layer, field_names)
                info["cursor"] = int(change["cursor"])
                if was_edit:
                    layer.startEditing()
            self._managed_cursor = delta.next_cursor
            if delta.has_more:
                self._managed_delta_debounce.start()
            if applied_count > 0:
                self.ws_status.setText(
                    f"Đã nhận {applied_count} đối tượng cập nhật từ Web."
                )
        except OpenVietGisError as exc:
            if getattr(exc, "status_code", None) in (401, 403):
                self._on_managed_session_revoked()
            else:
                self.ws_status.setText(f"Tạm dừng nhận cập nhật: {exc}")
        except (TypeError, ValueError) as exc:
            self._managed_poll.stop()
            self.ws_status.setText(f"Dữ liệu cập nhật không hợp lệ: {exc}")

    @staticmethod
    def _managed_snapshot(layer: QgsVectorLayer, fields: list[str], assign_ids: bool = False) -> dict:
        if layer.fields().names() != ["ovg_feature_id", *fields]:
            raise ApiError("Cấu trúc lớp đã thay đổi; hãy xuất bản sao và mở lại.")
        result = {}
        for feat in layer.getFeatures():
            raw_id = feat["ovg_feature_id"]
            oid = "" if QgsVariantUtils.isNull(raw_id) else str(raw_id).strip()
            if not oid:
                if assign_ids:
                    oid = str(uuid4())
                    ok = (layer.changeAttributeValue(feat.id(), 0, oid) if layer.isEditable()
                          else layer.dataProvider().changeAttributeValues({feat.id(): {0: oid}}))
                    if not ok:
                        raise ApiError("Không thể gán mã đối tượng.")
                else:
                    oid = f"@local:{feat.id()}"
            if oid in result:
                raise ApiError("Mã đối tượng bị trùng; không thể đồng bộ.")
            props = {}
            for name in fields:
                value = feat[name]
                if QgsVariantUtils.isNull(value):
                    value = None
                elif isinstance(value, (QDate, QDateTime, QTime)):
                    value = value.toString(Qt.ISODate)
                props[name] = value
            result[oid] = {
                "geometry": json.loads(feat.geometry().asJson()) if not feat.geometry().isEmpty() else None,
                "properties": props,
            }
        # Reject unsupported field values before sending any write.
        json.dumps(result, allow_nan=False)
        return result

    def _managed_is_dirty(self, layer: QgsVectorLayer, info: dict) -> bool:
        try:
            return bool(info.get("pending")) or self._managed_snapshot(
                layer, info["field_names"]
            ) != info["baseline"]
        except (OpenVietGisError, TypeError, ValueError):
            return True

    def _draft_path(self, lid: str) -> Path:
        config = self._managed_config
        if config is None:
            raise ApiError("Không có phiên để lưu bản nháp.")
        root = Path(QgsApplication.qgisSettingsDirPath()) / "openvietgis" / "drafts"
        return draft_path(root, config.base_url, config.organization_id, config.project_id, lid)

    def _write_managed_draft(self, lid: str, info: dict, current: dict) -> None:
        path = self._draft_path(lid)
        if current == info["baseline"] and not info.get("pending"):
            path.unlink(missing_ok=True)
            return
        write_draft(path, {
            "version": 1, "source_id": info["source_id"],
            "schema_revision": info["schema_revision"],
            "permission_epoch": info["permission_epoch"],
            "baseline": info["baseline"], "local": current,
            "pending": info.get("pending"),
        })

    def _checkpoint_managed_drafts(self) -> None:
        if self._managed_config is None:
            return
        for lid, info in self._managed_layers.items():
            layer = QgsProject.instance().mapLayer(info["map_layer_id"])
            if not isinstance(layer, QgsVectorLayer) or info.get("recovery_blocked"):
                continue
            try:
                current = self._managed_snapshot(layer, info["field_names"], assign_ids=True)
                self._write_managed_draft(lid, info, current)
            except (OpenVietGisError, OSError, ValueError, TypeError) as exc:
                self.managed_auto_save.setChecked(False)
                self.ws_status.setText(f"Không thể lưu bản nháp: {exc}")

    def _restore_managed_draft(self, layer: QgsVectorLayer, info: dict, lid: str) -> None:
        path = self._draft_path(lid)
        try:
            draft = read_draft(path)
            if draft is None:
                return
            if (draft["source_id"] != info["source_id"] or draft["schema_revision"] != info["schema_revision"]
                    or draft.get("permission_epoch") != info["permission_epoch"]):
                raise ApiError("Quyền hoặc cấu trúc lớp đã đổi; cần đối chiếu bản nháp thủ công.")
            base = draft["baseline"]
            pending = draft.get("pending")
            if pending:
                try:
                    get_managed_commit_receipt(self._managed_config, operation_id=pending["operation_id"])
                except ApiError as exc:
                    if exc.status_code != 404:
                        raise
                    # Keep uncertain operations for explicit review, never replay across sessions.
                    raise ApiError("Giao dịch cũ chưa xác nhận; cần kiểm tra lịch sử trên Web.") from exc
                else:
                    base = pending["snapshot"]
            merged, conflicts = merge_snapshots(base, draft["local"], info["baseline"])
            if conflicts:
                raise ApiError("Xung đột đối tượng: " + ", ".join(conflicts[:10]))
            if merged == info["baseline"]:
                path.unlink(missing_ok=True)
                return
            if not self._managed_session.can_edit:
                raise ApiError("Tài khoản hiện tại chỉ có quyền xem.")
            if not layer.isEditable() and not layer.startEditing():
                raise ApiError("Không mở được chế độ sửa để khôi phục.")
            existing = {str(f["ovg_feature_id"]): f.id() for f in layer.getFeatures()}
            for oid in existing.keys() - merged.keys():
                if not layer.deleteFeature(existing[oid]):
                    raise ApiError("Không thể khôi phục thao tác xóa.")
            for oid, item in merged.items():
                geometry = self._geojson_to_qgs_geometry(item["geometry"])
                values = [oid, *(item["properties"].get(n) for n in info["field_names"])]
                if oid in existing:
                    fid = existing[oid]
                    if not layer.changeGeometry(fid, geometry):
                        raise ApiError("Không thể khôi phục hình học.")
                    for index, value in enumerate(values):
                        if not layer.changeAttributeValue(fid, index, value):
                            raise ApiError("Không thể khôi phục thuộc tính.")
                else:
                    feat = QgsFeature(layer.fields())
                    feat.setGeometry(geometry)
                    feat.setAttributes(values)
                    if not layer.addFeature(feat):
                        raise ApiError("Không thể khôi phục đối tượng.")
            self._write_managed_draft(lid, info, merged)
        except (OpenVietGisError, OSError, ValueError, TypeError, KeyError) as exc:
            # Preserve original journal and prevent automatic overwrite of unresolved edits.
            info["recovery_blocked"] = True
            if layer.isEditable():
                layer.rollBack()
            layer.setReadOnly(True)
            QMessageBox.warning(self, "Cần đối chiếu bản nháp", f"{exc}\nBản nháp: {path}")

    def _managed_tick(self) -> None:
        self._checkpoint_managed_drafts()
        if self.managed_auto_save.isChecked() and self._managed_session and self._managed_session.can_edit:
            dirty = any(
                isinstance(layer := QgsProject.instance().mapLayer(info["map_layer_id"]), QgsVectorLayer)
                and not info.get("recovery_blocked") and self._managed_is_dirty(layer, info)
                for info in self._managed_layers.values()
            )
            if dirty:
                self._on_commit_r029_managed(automatic=True)
        self._on_managed_fetch_delta()

    def _on_commit_r029_managed(self, checked: bool = False, *, automatic: bool = False) -> None:
        if self._managed_session is None or not self._managed_layers:
            return
        if not self._managed_session.can_edit or self._managed_config is None:
            return
        allow_deletes = self.allow_deletes_checkbox.isChecked() and not automatic
        total_ops = 0
        try:
            config = self._managed_config
            for lid, info in self._managed_layers.items():
                if info.get("recovery_blocked"):
                    continue
                layer = QgsProject.instance().mapLayer(info["map_layer_id"])
                if not isinstance(layer, QgsVectorLayer):
                    continue
                current = self._managed_snapshot(layer, info["field_names"], assign_ids=True)
                pending = info.get("pending")
                if pending:
                    try:
                        receipt = get_managed_commit_receipt(config, operation_id=pending["operation_id"])
                    except ApiError as exc:
                        if exc.status_code != 404:
                            raise
                        if automatic and pending["request"]["allow_deletes"]:
                            raise ApiError("Giao dịch chứa xóa; cần xác nhận lưu thủ công.")
                        if current != pending["snapshot"]:
                            raise ApiError("Lần lưu trước chưa xác nhận; bản nháp đã đổi. Hãy giữ bản sao để đối chiếu.")
                        receipt = commit_managed_changes(config, **pending["request"])
                    info["baseline"] = pending["snapshot"]
                    info["content_revision"] = receipt.resulting_content_revision
                    info.pop("pending", None)
                    total_ops += receipt.operation_count
                    self._write_managed_draft(lid, info, current)
                ops = diff_managed_snapshots(info["baseline"], current, allow_deletes=allow_deletes)
                if not ops:
                    if layer.isEditable() and layer.isModified():
                        if not layer.commitChanges():
                            raise ApiError("Không thể lưu bộ đệm cục bộ.")
                        layer.startEditing()
                    continue
                if len(ops) > 1000:
                    raise ApiError("Tối đa 1000 thay đổi/lần lưu trên một lớp. Hãy chia nhỏ chỉnh sửa.")
                request = {
                    "session_id": self._managed_session.id,
                    "source_id": info["source_id"],
                    "base_content_revision": info["content_revision"],
                    "base_schema_revision": info["schema_revision"],
                    "operation_id": str(uuid4()),
                    "operations": ops,
                    "summary": "QGIS Managed Realtime Edit",
                    "allow_deletes": allow_deletes,
                }
                info["pending"] = {"operation_id": request["operation_id"], "snapshot": current, "request": request}
                self._write_managed_draft(lid, info, current)
                try:
                    receipt = commit_managed_changes(config, **request)
                except ApiError as exc:
                    if exc.status_code in (400, 401, 403, 409, 422):
                        info.pop("pending", None)
                    raise
                info["baseline"] = current
                info["content_revision"] = receipt.resulting_content_revision
                info.pop("pending", None)
                total_ops += receipt.operation_count
                self._write_managed_draft(lid, info, current)
                if layer.isEditable() and not layer.commitChanges():
                    raise ApiError("Web đã lưu, nhưng QGIS chưa kết thúc phiên sửa cục bộ.")
                if not layer.isEditable():
                    layer.startEditing()
            self.ws_status.setText(
                f"Đã xác nhận {total_ops} thay đổi trên Web." if total_ops
                else "Không có chỉnh sửa mới cần lưu."
            )
            self._managed_delta_debounce.start()
        except (OpenVietGisError, OSError, TypeError, ValueError) as exc:
            self.ws_status.setText(f"Lưu chưa hoàn tất ({total_ops} thay đổi đã xác nhận): {exc}")
            self._checkpoint_managed_drafts()
            if automatic:
                self.managed_auto_save.setChecked(False)
            else:
                QMessageBox.warning(self, "Lưu thay đổi lên Web", str(exc))

    def _on_managed_desktop_command(self) -> None:
        if self._managed_session is None:
            return
        try:
            config = self._managed_config
            if config is None:
                return
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
        self._stop_browser_login()
        self._checkpoint_managed_drafts()
        self._draft_timer.stop()
        self._managed_poll.stop()
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