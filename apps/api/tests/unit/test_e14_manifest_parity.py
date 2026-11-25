"""Shared golden manifest fixtures consumed by the Python reference and Rust SDK."""
import json
from pathlib import Path

import pytest

from openvietgis_api.domains.extensions.domain import validate_manifest_contract
from openvietgis_api.domains.platform_errors import PlatformValidationError

CASES = json.loads(
    (Path(__file__).resolve().parents[4] / "fixtures/rust-platform/e14/manifest-contract.json")
    .read_text(encoding="utf-8")
)

@pytest.mark.parametrize("case", CASES, ids=[case["id"] for case in CASES])
def test_shared_manifest_contract(case):
    manifest = {k: v for k, v in case["manifest"].items() if k not in {"display_name", "description"}}
    if case["valid"]:
        validate_manifest_contract(**manifest)
    else:
        with pytest.raises(PlatformValidationError):
            validate_manifest_contract(**manifest)
