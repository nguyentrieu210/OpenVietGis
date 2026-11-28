"""Protect the dependency layer and superseded-PR cancellation policy."""
import json
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]


def test_frontend_dependency_layer_covers_all_workspace_manifests() -> None:
    docker = (ROOT / "infra/prod/Dockerfile.web").read_text()
    before, after = docker.split("RUN pnpm validate:runtime && pnpm install --frozen-lockfile", 1)
    workspace = yaml.safe_load((ROOT / "pnpm-workspace.yaml").read_text())
    manifests = {
        path.relative_to(ROOT).as_posix()
        for pattern in workspace["packages"]
        for path in ROOT.glob(pattern + "/package.json")
    }
    assert manifests
    for path in manifests:
        assert f"COPY {path} ./{path}" in before
        # An unexpected install hook may require application source.
        package = json.loads((ROOT / path).read_text())
        assert not {"preinstall", "install", "postinstall", "prepare"} & package.get("scripts", {}).keys()
    assert "COPY apps/web-next ./apps/web-next" in after
    assert "COPY packages ./packages" in after
    assert "COPY modules ./modules" in after
    assert "pnpm --filter @openvietgis/web-next build" in after


def test_pr_cancellation_cannot_cancel_main_or_manual_runs() -> None:
    for name in ("ci.yml", "e17-web-parity.yml", "rust-platform.yml"):
        workflow = yaml.safe_load((ROOT / ".github/workflows" / name).read_text())
        policy = workflow["concurrency"]
        assert policy["cancel-in-progress"] == "${{ github.event_name == 'pull_request' }}"
        assert "github.workflow" in policy["group"]
        assert "github.event_name" in policy["group"]
        assert "github.event.pull_request.number || github.run_id" in policy["group"]
