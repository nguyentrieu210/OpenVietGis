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
