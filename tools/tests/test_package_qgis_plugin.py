"""QGIS plugin release artifact guardrails."""

from __future__ import annotations

import hashlib
import zipfile
from pathlib import Path

import pytest

from tools.package_qgis_plugin import package_qgis_plugin


def test_plugin_zip_installable_deterministic_and_no_secrets(tmp_path: Path) -> None:
    output = tmp_path / "openvietgis_connector-r027.zip"
    sha = package_qgis_plugin(output)
    assert sha == hashlib.sha256(output.read_bytes()).hexdigest()
    assert (tmp_path / (output.name + ".sha256")).read_text().startswith(sha)
    names_expected = {
        "openvietgis_connector/__init__.py",
        "openvietgis_connector/metadata.txt",
        "openvietgis_connector/client.py",
        "openvietgis_connector/managed_client.py",
        "openvietgis_connector/drafts.py",
        "openvietgis_connector/browser_login.py",
        "openvietgis_connector/plugin.py",
        "openvietgis_connector/workspace.py",
        "openvietgis_connector/README.md",
        "openvietgis_connector/R027-MANUAL-SYNC.md",
    }
    with zipfile.ZipFile(output) as archive:
        assert set(archive.namelist()) == names_expected
        metadata = archive.read("openvietgis_connector/metadata.txt")
        assert metadata.startswith(b"[general]")
        assert b"qgisMinimumVersion=3.34" in metadata
        assert all("/." not in name and not name.endswith(".pyc") for name in archive.namelist())
    other = tmp_path / "nested" / "openvietgis_connector-r027.zip"
    second_sha = package_qgis_plugin(other)
    assert second_sha == sha
    assert other.read_bytes() == output.read_bytes()


def test_plugin_zip_refuses_missing_source_and_unreviewed_metadata(tmp_path: Path) -> None:
    plugin = tmp_path / "plugin"
    plugin.mkdir()
    with pytest.raises(ValueError, match="Missing"):
        package_qgis_plugin(tmp_path / "out.zip", plugin_root=plugin)
