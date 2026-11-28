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
    for name in ("ci.yml", "e17-web-parity.yml", "rust-platform.yml",
                 "rust-mcp.yml", "e16-mcp-retained.yml", "qgis-connector.yml"):
        workflow = yaml.safe_load((ROOT / ".github/workflows" / name).read_text())
        policy = workflow["concurrency"]
        assert policy["cancel-in-progress"] == "${{ github.event_name == 'pull_request' }}"
        assert "github.workflow" in policy["group"]
        assert "github.event_name" in policy["group"]
        assert "github.event.pull_request.number || github.run_id" in policy["group"]


def test_mcp_contract_changes_trigger_both_parity_implementations() -> None:
    retained = yaml.load(
        (ROOT / ".github/workflows/e16-mcp-retained.yml").read_text(), Loader=yaml.BaseLoader,
    )
    assert "main" in retained["on"]["push"]["branches"]
    for event in ("push", "pull_request"):
        assert "apps/mcp-rust/contracts/**" in retained["on"][event]["paths"]
    runs = "\n".join(step.get("run", "") for step in retained["jobs"]["adapter-contracts"]["steps"])
    assert "uv run --locked pytest -q" in runs
    assert "tools/benchmark/spec066/mcp-contract-parity.py" in runs
    assert "git rev-parse HEAD" in runs


def test_qgis_sync_contracts_are_part_of_package_ci() -> None:
    workflow = yaml.load(
        (ROOT / ".github/workflows/qgis-connector.yml").read_text(), Loader=yaml.BaseLoader,
    )
    runs = next(step["run"] for step in workflow["jobs"]["qgis-package"]["steps"]
                if step.get("name") == "Test manual bilateral sync and deterministic ZIP")
    for name in ("test_managed_snapshot_contract", "test_managed_delta_contract",
                 "test_managed_sync_cancellation"):
        path = f"tools/tests/{name}.py"
        assert path in workflow["on"]["pull_request"]["paths"]
        assert path in runs
    managed = "apps/api/tests/unit/integration_hub/test_r029_managed_sync.py"
    assert managed in workflow["on"]["pull_request"]["paths"]
    assert managed in runs
    assert "native-qgis-smoke" in workflow["jobs"]
