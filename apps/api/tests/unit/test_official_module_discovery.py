from pathlib import Path

import pytest

from openvietgis_api.adapters.http.official_modules import (
    discover_official_module_runtimes,
)


def test_no_modules_needed(tmp_path: Path) -> None:
    assert discover_official_module_runtimes(module_root=tmp_path / "missing") == []


def test_manifest_key_mismatch_rejected(tmp_path: Path) -> None:
    module = tmp_path / "fixture"
    module.mkdir()
    (module / "manifest.json").write_text('{"extension_key":"other"}')
    (module / "runtime.py").write_text("raise AssertionError('must not import')")
    with pytest.raises(RuntimeError, match="does not match manifest key"):
        discover_official_module_runtimes(module_root=tmp_path)


def test_shipped_modules_discovered() -> None:
    runtimes = discover_official_module_runtimes()
    assert len({runtime.extension_key for runtime in runtimes}) == len(runtimes)
    assert all(runtime.router.routes for runtime in runtimes)
    root = Path(__file__).resolve().parents[4] / "modules"
    expected = {
        path.parent.name
        for path in root.glob("*/manifest.json")
        if (path.parent / "runtime.py").is_file()
    }
    assert {runtime.extension_key for runtime in runtimes} == expected
