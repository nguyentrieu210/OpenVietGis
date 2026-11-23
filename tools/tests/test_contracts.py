import json
from pathlib import Path

from tools import check_contracts

ROOT = Path(__file__).resolve().parents[2]
CONTRACTS = {
    "auth-session": "002-identity-organization-tenancy",
    "tenant-core": "002-identity-organization-tenancy",
    "service-admin": "002-identity-organization-tenancy",
    "spatial-catalog": "003-spatial-catalog-crs",
    "map-workspace": "004-map-workspace",
}


def test_contract_validation_covers_all_integrated_contracts() -> None:
    expected_specs = {
        ROOT / "specs" / spec / "contracts" / f"{name}-api.yaml"
        for name, spec in CONTRACTS.items()
    }
    expected_outputs = {
        ROOT / "packages/client-sdk/src/generated" / f"{name}.ts" for name in CONTRACTS
    }
    assert expected_specs <= set(check_contracts.OPENAPI_FILES)
    assert expected_outputs <= set(check_contracts.GENERATED_FILES)
    assert check_contracts.validate_contracts() == []


def test_sdk_generator_covers_all_integrated_contracts() -> None:
    package = json.loads(
        (ROOT / "packages/client-sdk/package.json").read_text(encoding="utf-8")
    )
    commands = package["scripts"]["generate:contracts"].split(" && ")
    for name, spec in CONTRACTS.items():
        assert (
            f"openapi-typescript ../../specs/{spec}/contracts/{name}-api.yaml "
            f"-o src/generated/{name}.ts"
        ) in commands
