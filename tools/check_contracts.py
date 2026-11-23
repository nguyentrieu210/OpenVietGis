from __future__ import annotations

from pathlib import Path

from openapi_spec_validator import validate
from openapi_spec_validator.readers import read_from_filename
from openapi_spec_validator.validation.exceptions import OpenAPIValidationError

ROOT = Path(__file__).resolve().parents[1]
CONTRACT_DIR = ROOT / "specs" / "002-identity-organization-tenancy" / "contracts"
SPATIAL_CONTRACT_DIR = ROOT / "specs" / "003-spatial-catalog-crs" / "contracts"
MAP_CONTRACT_DIR = ROOT / "specs" / "004-map-workspace" / "contracts"
TRANSFER_CONTRACT_DIR = ROOT / "specs" / "005-import-export" / "contracts"
OPENAPI_FILES = (
    CONTRACT_DIR / "auth-session-api.yaml",
    CONTRACT_DIR / "tenant-core-api.yaml",
    CONTRACT_DIR / "service-admin-api.yaml",
    SPATIAL_CONTRACT_DIR / "spatial-catalog-api.yaml",
    MAP_CONTRACT_DIR / "map-workspace-api.yaml",
    TRANSFER_CONTRACT_DIR / "import-export-api.yaml",
)
GENERATED_FILES = (
    ROOT / "packages" / "client-sdk" / "src" / "generated" / "auth-session.ts",
    ROOT / "packages" / "client-sdk" / "src" / "generated" / "tenant-core.ts",
    ROOT / "packages" / "client-sdk" / "src" / "generated" / "service-admin.ts",
    ROOT / "packages" / "client-sdk" / "src" / "generated" / "spatial-catalog.ts",
    ROOT / "packages" / "client-sdk" / "src" / "generated" / "map-workspace.ts",
    ROOT / "packages" / "client-sdk" / "src" / "generated" / "import-export.ts",
)


def validate_contracts() -> list[str]:
    errors: list[str] = []
    for path in OPENAPI_FILES:
        if not path.exists():
            errors.append(f"missing OpenAPI contract: {path.relative_to(ROOT)}")
            continue
        spec, base_uri = read_from_filename(str(path))
        try:
            validate(spec, base_uri=base_uri)
        except (
            OpenAPIValidationError
        ) as exc:  # pragma: no cover - library supplies details
            errors.append(f"{path.relative_to(ROOT)}: {exc}")
    permission_registry = CONTRACT_DIR / "permission-registry.md"
    if not permission_registry.exists():
        errors.append("missing permission-registry.md")
    for path in GENERATED_FILES:
        if not path.exists():
            errors.append(f"missing generated contract types: {path.relative_to(ROOT)}")
    return errors


def main() -> int:
    errors = validate_contracts()
    if errors:
        for error in errors:
            print(f"CONTRACT ERROR: {error}")
        return 1
    print("Spec 002, 003, 004 and 005 contracts: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
