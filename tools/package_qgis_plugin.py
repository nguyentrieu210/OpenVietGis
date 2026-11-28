"""Deterministic, installable OpenVietGIS QGIS plugin ZIP for QGIS 3.34+.

Only the explicitly reviewed plugin source/metadata/docs is included. This tool
never copies local tokens, user projects, GeoPackage data or hidden workspace
files into distributable packages.
"""

from __future__ import annotations

import argparse
import hashlib
import zipfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
PLUGIN_ROOT = REPO_ROOT / "integrations/qgis_plugin/openvietgis_connector"
REQUIRED = (
    "__init__.py",
    "metadata.txt",
    "client.py",
    "managed_client.py",
    "snapshot_cache.py",
    "drafts.py",
    "browser_login.py",
    "plugin.py",
    "workspace.py",
    "README.md",
    "R027-MANUAL-SYNC.md",
)
FIXED_ZIP_TIME = (2026, 11, 23, 0, 0, 0)


def package_qgis_plugin(output: Path, *, plugin_root: Path = PLUGIN_ROOT) -> str:
    plugin_root = plugin_root.resolve()
    output = output.expanduser().resolve()
    if output.is_relative_to(plugin_root):
        raise ValueError("Output ZIP cannot live inside QGIS plugin source directory.")
    for filename in REQUIRED:
        path = plugin_root / filename
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"Missing QGIS plugin file: {filename}")
    metadata = (plugin_root / "metadata.txt").read_text(encoding="utf-8")
    if "[general]" not in metadata or "qgisMinimumVersion=" not in metadata:
        raise ValueError("Invalid QGIS plugin metadata.")
    if "experimental=True" not in metadata:
        raise ValueError("Plugin remains experimental pending native and live acceptance.")
    if "qgisMinimumVersion=3.34" not in metadata:
        raise ValueError("Python 3.10+ plugin requires reviewed QGIS 3.34+ profile.")

    # Only reviewed entries, not every file with an approved extension.
    files = sorted(plugin_root / filename for filename in REQUIRED)
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as zf:
        for path in files:
            arcname = "openvietgis_connector/" + path.name
            info = zipfile.ZipInfo(arcname, date_time=FIXED_ZIP_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (0o100644 << 16)
            zf.writestr(info, path.read_bytes(), compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
    sha = hashlib.sha256(output.read_bytes()).hexdigest()
    (output.parent / (output.name + ".sha256")).write_text(
        f"{sha}  {output.name}\n", encoding="ascii"
    )
    return sha


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    sha = package_qgis_plugin(args.output)
    print(f"QGIS plugin ZIP: {args.output.resolve()} (SHA-256 {sha})")


if __name__ == "__main__":
    main()
