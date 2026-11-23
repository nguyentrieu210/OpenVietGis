from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
REGISTRY = (
    ROOT / "specs" / "002-identity-organization-tenancy" / "contracts" / "permission-registry.md"
)


def test_permission_registry_has_no_cross_org_transfer_permission() -> None:
    text = REGISTRY.read_text(encoding="utf-8").lower()
    assert "project.transfer" not in text


def test_permission_registry_has_no_configurable_deny_or_custom_role_surface() -> None:
    text = REGISTRY.read_text(encoding="utf-8").lower()
    assert "deny-policy" not in text
    assert "custom role builder" not in text
    assert "custom-role builder" not in text


def test_resource_permissions_are_additive_and_credential_scope_only_restricts() -> None:
    text = REGISTRY.read_text(encoding="utf-8")
    assert "set union" in text
    assert "intersects" in text
    assert "No grant subtracts permissions" in text
