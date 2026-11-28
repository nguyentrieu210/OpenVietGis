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


def test_ci_builds_native_binary_once_and_verifies_both_consumers() -> None:
    workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
    jobs = workflow["jobs"]
    steps = jobs["native-api"]["steps"]
    assert sum("cargo build --locked" in s.get("run", "") for s in steps) == 1
    assert any(s.get("uses") == "actions/upload-artifact@v4" for s in steps)
    for consumer in ("backend", "validate"):
        assert jobs[consumer]["needs"] == "native-api"
        assert jobs[consumer]["runs-on"] == jobs["native-api"]["runs-on"]
        steps = jobs[consumer]["steps"]
        assert not any("cargo build --locked" in s.get("run", "") for s in steps)
        assert any(s.get("uses") == "actions/download-artifact@v4" for s in steps)
        assert any("sha256sum --check" in s.get("run", "") for s in steps)


def test_every_workflow_has_one_pr_only_concurrency_policy() -> None:
    workflows = sorted((ROOT / ".github/workflows").glob("*.yml"))
    workflows += sorted((ROOT / ".github/workflows").glob("*.yaml"))
    assert workflows
    for path in workflows:
        text = path.read_text()
        # PyYAML accepts duplicate keys silently, so inspect the top-level key too.
        assert text.splitlines().count("concurrency:") == 1, path.name
        workflow = yaml.load(text, Loader=yaml.BaseLoader)
        policy = workflow["concurrency"]
        assert policy["group"] == (
            "${{ github.workflow }}-${{ github.event_name }}-"
            "${{ github.event.pull_request.number || github.run_id }}"
        ), path.name
        assert policy["cancel-in-progress"] == "${{ github.event_name == 'pull_request' }}", path.name


def test_full_ci_is_not_path_filtered_for_prs_or_main() -> None:
    ci = yaml.load((ROOT / ".github/workflows/ci.yml").read_text(), Loader=yaml.BaseLoader)
    triggers = ci["on"]
    assert triggers["pull_request"] in (None, "")
    assert "main" in triggers["push"]["branches"]


def test_native_postgis_qualification_runs_for_rust_source_changes() -> None:
    for name in ("e06-e08-retention.yml", "e11-outbox.yml"):
        workflow = yaml.load((ROOT / ".github/workflows" / name).read_text(), Loader=yaml.BaseLoader)
        triggers = workflow["on"]
        for event in ("push", "pull_request"):
            paths = triggers[event]["paths"]
            assert "Cargo.toml" in paths, (name, event)
            assert "Cargo.lock" in paths, (name, event)
            assert "rust-toolchain.toml" in paths, (name, event)
            assert "apps/api-rust/**" in paths, (name, event)
            assert "crates/**" in paths, (name, event)
        assert "main" in triggers["push"]["branches"], name


def test_spec065_processing_reference_runs_for_integrated_qualification_branch() -> None:
    workflow = yaml.safe_load((ROOT / ".github/workflows/rust-platform.yml").read_text())
    branch_guard = "github.head_ref == '066-t039-integrated-qualification'"
    for job in ("qgis-reference", "spec065-processing-comparison"):
        condition = workflow["jobs"][job]["if"]
        assert "startsWith(github.head_ref, '065-')" in condition
        assert "startsWith(github.ref_name, '065-')" in condition
        assert branch_guard in condition
        assert "github.ref_name == '066-t039-integrated-qualification'" in condition
    processing = next(
        step for step in workflow["jobs"]["postgis-parity"]["steps"]
        if step.get("name") == "Spec 065 measured Rust processing reference"
    )
    assert branch_guard in processing["if"]
    assert "github.ref_name == '066-t039-integrated-qualification'" in processing["if"]
