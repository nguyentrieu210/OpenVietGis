"""R025 response never implies a real V8i connection from fixture-only proof."""

from openvietgis_api.adapters.http.v8i_bridge import capability_status


def test_v8i_capabilities_fail_closed():
    body = capability_status()
    assert body["connection_status"] == "not_paired"
    assert body["live_connection"] is False
    assert body["read_only"] is True
    assert body["coordinate_units_verified"] is False
    assert body["capabilities"]["write_back_to_dgn"] == "unavailable"
    assert body["capabilities"]["sync_to_openvietgis"] == "unavailable"
    assert "remote_preview" in body["capabilities"]
