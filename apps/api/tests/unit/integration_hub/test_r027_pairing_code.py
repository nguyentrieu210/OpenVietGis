from __future__ import annotations

import sys
from pathlib import Path
from uuid import uuid4

import pytest

from openvietgis_api.adapters.http.integration_bindings import _build_pairing_code

PLUGIN_DIR = (
    Path(__file__).resolve().parents[5]
    / "integrations"
    / "qgis_plugin"
    / "openvietgis_connector"
)
if str(PLUGIN_DIR) not in sys.path:
    sys.path.insert(0, str(PLUGIN_DIR))

from client import OpenVietGisError, decode_pairing_code  # noqa: E402


def test_pairing_code_roundtrip() -> None:
    org_id = uuid4()
    proj_id = uuid4()
    binding_id = uuid4()
    layer_id = uuid4()
    code = _build_pairing_code(
        server_url="https://openvietgis.cloud/",
        organization_id=org_id,
        project_id=proj_id,
        binding_id=binding_id,
        layer_id=layer_id,
        external_dataset_ref="parcel_sync_01",
        bearer_token="ovg_test_secret_token",
    )
    assert code.startswith("ovg-pair-v1:")
    parsed = decode_pairing_code(code)
    assert parsed["server_url"] == "https://openvietgis.cloud"
    assert parsed["organization_id"] == str(org_id)
    assert parsed["project_id"] == str(proj_id)
    assert parsed["binding_id"] == str(binding_id)
    assert parsed["layer_id"] == str(layer_id)
    assert parsed["external_dataset_ref"] == "parcel_sync_01"
    assert parsed["bearer_token"] == "ovg_test_secret_token"


def test_pairing_code_rejects_malformed() -> None:
    with pytest.raises(OpenVietGisError):
        decode_pairing_code("not-a-pairing-code")
    with pytest.raises(OpenVietGisError):
        decode_pairing_code("ovg-pair-v1:!!!")
