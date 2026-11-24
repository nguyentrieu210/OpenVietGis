"""Discovery for trusted official modules shipped beside the source tree."""

from __future__ import annotations

import importlib
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from fastapi import APIRouter


@dataclass(frozen=True, slots=True)
class OfficialModuleRuntime:
    extension_key: str
    manifest: dict[str, Any]
    router: APIRouter


def discover_official_module_runtimes(
    *, module_root: Path | None = None
) -> list[OfficialModuleRuntime]:
    repo_root = Path(__file__).resolve().parents[5]
    root = module_root or repo_root / "modules"
    if not root.is_dir():
        return []
    if str(repo_root) not in sys.path:
        sys.path.insert(0, str(repo_root))

    runtimes: list[OfficialModuleRuntime] = []
    for directory in sorted(path for path in root.iterdir() if path.is_dir()):
        manifest_path = directory / "manifest.json"
        runtime_path = directory / "runtime.py"
        if not manifest_path.is_file() or not runtime_path.is_file():
            continue
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        key = manifest.get("extension_key")
        if key != directory.name:
            raise RuntimeError(
                f"official module directory {directory.name!r} does not match manifest key {key!r}"
            )
        module = importlib.import_module(f"modules.{directory.name}.runtime")
        router = getattr(module, "router", None)
        if not isinstance(router, APIRouter):
            raise RuntimeError(
                f"official module {directory.name!r} does not expose APIRouter router"
            )
        runtimes.append(OfficialModuleRuntime(directory.name, manifest, router))
    return runtimes


def official_manifest(extension_key: str) -> dict[str, Any] | None:
    for runtime in discover_official_module_runtimes():
        if runtime.extension_key == extension_key:
            return dict(runtime.manifest)
    return None
