"""Keep the public production frontend covered by the main quality gate."""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def test_ci_validation_builds_canonical_production_frontend() -> None:
    root = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
    script = root["scripts"]["validate:ts"]
    assert "pnpm --filter @openvietgis/web-next build" in script
    docker = (ROOT / "infra/prod/Dockerfile.web").read_text(encoding="utf-8")
    assert "pnpm --filter @openvietgis/web-next build" in docker
    assert "/app/apps/web-next/dist" in docker


def test_canonical_frontend_has_build_script() -> None:
    app = json.loads((ROOT / "apps/web-next/package.json").read_text(encoding="utf-8"))
    assert app["name"] == "@openvietgis/web-next"
    assert "vite build" in app["scripts"]["build"]


def test_ci_runs_built_workspace_and_mcp_regression() -> None:
    ci = (ROOT / ".github/workflows/ci.yml").read_text()
    package = json.loads((ROOT / "package.json").read_text())
    assert "pnpm validate:canonical-smoke" in ci
    assert "working-directory: apps/mcp" in ci
    assert "run-canonical-smoke.mjs" in package["scripts"]["validate:canonical-smoke"]
    smoke = (ROOT / "apps/web-next/run-canonical-smoke.mjs").read_text()
    assert 'host:"127.0.0.1"' in smoke
    assert "spatial-mode-smoke.mjs" in smoke
    assert "core-toolkit-smoke.mjs" in smoke
