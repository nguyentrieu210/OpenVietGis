import json
from pathlib import Path
from urllib.parse import urlsplit

import yaml

ROOT = Path(__file__).resolve().parents[2]


def test_root_validation_manifest_contains_all_mandatory_categories() -> None:
    package = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
    scripts = package["scripts"]

    assert "validate" in scripts
    for category in (
        "validate:runtime",
        "validate:ts",
        "validate:py",
        "validate:contracts",
        "validate:scope",
        "validate:arch",
        "validate:docs",
        "validate:integration",
        "validate:e2e",
        "validate:e2e:bootstrap",
    ):
        assert category in scripts
        assert category in scripts["validate"]
    assert "tools/check_tenant_timing.py" in scripts["validate"]


def test_ci_uses_the_dedicated_database_and_reference_identity_fixture() -> None:
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    )
    job = workflow["jobs"]["validate"]
    env = job["env"]
    owner = urlsplit(env["OPENVIETGIS_MIGRATION_DATABASE_URL"])
    runtime = urlsplit(env["OPENVIETGIS_RUNTIME_DATABASE_URL"])

    assert env["OPENVIETGIS_ENVIRONMENT"] == "test"
    assert (
        env["OPENVIETGIS_TEST_DATABASE_URL"]
        == env["OPENVIETGIS_MIGRATION_DATABASE_URL"]
    )
    assert env["OPENVIETGIS_DATABASE_URL"] == env["OPENVIETGIS_TEST_DATABASE_URL"]
    assert owner.hostname == runtime.hostname == "127.0.0.1"
    assert owner.port == runtime.port == 54330
    assert owner.path == runtime.path == "/openvietgis_test"
    assert owner.username == "openvietgis"
    assert runtime.username == "openvietgis_runtime"

    compose = yaml.safe_load(
        (ROOT / "infra/dev/compose.yaml").read_text(encoding="utf-8")
    )
    database = compose["services"]["postgres-test"]
    assert database["environment"]["POSTGRES_DB"] == "openvietgis_test"
    assert database["ports"] == ["127.0.0.1:54330:5432"]
    assert any("001-runtime-role.sql" in volume for volume in database["volumes"])

    identity_path = ROOT / "infra/dev/compose.identity.yaml"
    identity = yaml.safe_load(identity_path.read_text(encoding="utf-8"))["services"][
        "keycloak"
    ]
    assert identity["ports"] == ["127.0.0.1:18080:8080"]
    assert identity["environment"]["KC_HEALTH_ENABLED"] == "true"
    assert "--import-realm" in identity["command"]
    assert any("realm-openvietgis.json" in volume for volume in identity["volumes"])

    commands = [step["run"] for step in job["steps"] if "run" in step]
    start = next(command for command in commands if "up --detach --wait" in command)
    assert start.endswith("postgres-test keycloak")
    assert "-f infra/dev/compose.yaml -f infra/dev/compose.identity.yaml" in start
    realm = next(
        command
        for command in commands
        if "/.well-known/openid-configuration" in command
    )
    migration = next(
        command for command in commands if "tools.run_with_test_database" in command
    )
    assert "get_engine().connect()" in migration
    assert commands.index(start) < commands.index(realm) < commands.index(migration)
    frontend_validation = next(
        command for command in commands if command == "pnpm validate"
    )
    assert commands.index(migration) < commands.index(frontend_validation)
    assert "pnpm install --frozen-lockfile" in commands
    assert "uv sync --project apps/api --frozen" in commands
    timing = next(
        command for command in commands if "tools/check_tenant_timing.py" in command
    )
    assert commands.index(frontend_validation) < commands.index(timing)


def test_backend_ci_is_independent_of_javascript_dependency_install() -> None:
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    )
    backend = workflow["jobs"]["backend"]
    frontend = workflow["jobs"]["validate"]
    # Both consumers depend only on the shared native build, never each other.
    assert backend["needs"] == frontend["needs"] == "native-api"
    native = workflow["jobs"]["native-api"]
    assert "needs" not in native
    native_commands = "\n".join(step.get("run", "") for step in native["steps"])
    assert "pnpm" not in native_commands
    assert "npm " not in native_commands
    assert not any(
        "setup-node" in step.get("uses", "") or "pnpm/" in step.get("uses", "")
        for step in native["steps"] + backend["steps"]
    )
    assert "cargo build --locked" in native_commands
    assert backend["env"] == frontend["env"]
    commands = [step["run"] for step in backend["steps"] if "run" in step]
    all_commands = "\n".join(commands)
    assert "pnpm" not in all_commands
    for required in (
        "uv sync --project apps/api --frozen",
        "ruff check",
        "mypy",
        "pytest apps/api/tests/unit",
        "tools/check_contracts.py",
        "tools/check_scope.py",
        "tools/check_architecture.py",
        "tools/check_docs.py",
        "up --detach --wait",
        "get_engine().connect()",
        "pytest apps/api/tests/integration apps/api/tests/performance",
        "pytest apps/api/tests/integration/identity/test_live_keycloak_oidc.py",
    ):
        assert required in all_commands
    assert any(
        step.get("env", {}).get("OPENVIETGIS_LIVE_OIDC") == "1"
        for step in backend["steps"]
    )
