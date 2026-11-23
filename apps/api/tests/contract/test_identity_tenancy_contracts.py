import ast
from pathlib import Path

from tools.check_contracts import validate_contracts

ROOT = Path(__file__).resolve().parents[4]


def test_spec_002_openapi_and_generated_contracts_are_valid() -> None:
    assert validate_contracts() == []


def test_permission_registry_contract_exists_and_is_product_permission_based() -> None:
    registry = (
        ROOT
        / "specs"
        / "002-identity-organization-tenancy"
        / "contracts"
        / "permission-registry.md"
    ).read_text(encoding="utf-8")

    for permission in (
        "organization.read",
        "organization.manage_members",
        "organization.lifecycle",
        "workspace.read",
        "workspace.manage",
        "project.read",
        "project.edit",
        "authorization.manage_grants",
        "invitation.manage",
        "service_principal.manage",
        "audit.read",
    ):
        assert f"`{permission}`" in registry


def test_historical_migrations_do_not_import_mutable_runtime_contracts() -> None:
    versions = ROOT / "apps" / "api" / "migrations" / "versions"
    for migration in versions.glob("*.py"):
        tree = ast.parse(migration.read_text(encoding="utf-8"))
        for node in ast.walk(tree):
            if isinstance(node, ast.ImportFrom):
                assert not (node.module or "").startswith("openvietgis_api"), migration
            elif isinstance(node, ast.Import):
                assert all(not alias.name.startswith("openvietgis_api") for alias in node.names), (
                    migration
                )
