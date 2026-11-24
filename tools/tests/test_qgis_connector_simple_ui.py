"""Static acceptance for the simplified QGIS connector (runtime QGIS QA remains mandatory)."""

from __future__ import annotations

import ast
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
PLUGIN = REPO_ROOT / "integrations/qgis_plugin/openvietgis_connector/plugin.py"
WEB = REPO_ROOT / "apps/web-next/src/QgisBindingManager.tsx"


def test_simple_plugin_has_one_primary_open_save_flow() -> None:
    source = PLUGIN.read_text(encoding="utf-8")
    tree = ast.parse(source)
    assert tree is not None
    start = source.index('workspace_group = QGroupBox("Làm việc với bản đồ")')
    advanced = source.index('self.advanced_toggle = QPushButton(')
    primary = source[start:advanced]
    assert 'QPushButton("Mở bản đồ trong QGIS")' in primary
    assert 'QPushButton("Lưu thay đổi lên Web")' in primary
    assert "ws_open_all_button" not in primary
    assert "ws_sync_now_button" not in primary
    assert "realtime_checkbox" not in primary
    assert '"R027' not in primary and '"R029' not in primary
    assert 'adv_layout.addWidget(legacy_group)' in source
    assert 'self.advanced_box.setVisible(False)' in source


def test_simple_plugin_preserves_edit_recovery_guards() -> None:
    source = PLUGIN.read_text(encoding="utf-8")
    assert 'if state.get("pending"):' in source
    assert 'self._managed_delta_debounce.start()' in source
    assert 'local_layer.isModified() or self._managed_is_dirty(local_layer, info)' in source
    assert 'def _on_recover_workspace(self)' in source
    assert 'if self._managed_layers:' in source
    assert '"Còn chỉnh sửa chưa lưu"' in source
    assert 'layer.rollBack()' in source  # only used for unchanged layers after revoke


def test_download_and_version_are_consistent() -> None:
    web = WEB.read_text(encoding="utf-8")
    meta = (PLUGIN.parent / "metadata.txt").read_text(encoding="utf-8")
    assert 'version=1.0.0' in meta
    assert 'openvietgis_connector-1.0.0.zip' in web
    assert '⬇ Tải công cụ QGIS' in web
    assert 'R029 Managed Realtime (Không .gpkg)' not in PLUGIN.read_text(encoding="utf-8")