import base64
import json
import urllib.error
import urllib.request

import pytest

from integrations.qgis_plugin.openvietgis_connector.browser_login import BrowserLogin
from integrations.qgis_plugin.openvietgis_connector.drafts import (
    draft_path,
    merge_snapshots,
    read_draft,
    write_draft,
)


def test_atomic_draft_and_project_isolation(tmp_path):
    path = draft_path(tmp_path, "https://example.com", "o", "p", "l")
    assert path != draft_path(tmp_path, "https://example.com", "o", "other", "l")
    data = {"version": 1, "local": {"a": {"geometry": None, "properties": {"v": 1}}}}
    write_draft(path, data)
    assert read_draft(path) == data
    assert not list(tmp_path.glob(".draft-*"))
    with pytest.raises(ValueError):
        write_draft(path, {"version": 1, "invalid": float("nan")})
    assert read_draft(path) == data


def test_three_way_merge_and_conflict():
    base = {"a": 1, "b": 2, "c": 3}
    merged, conflict = merge_snapshots(base, {"a": 4, "b": 2}, {"a": 1, "b": 5, "c": 3})
    assert merged == {"a": 4, "b": 5}
    assert conflict == []
    _, conflict = merge_snapshots(base, {"a": 4, "b": 2, "c": 3}, {"a": 9, "b": 2, "c": 3})
    assert conflict == ["a"]


def test_browser_handoff_state_origin_and_one_time():
    origin = "https://openvietgis.cloud"
    flow = BrowserLogin(origin)
    try:
        payload = {"v": 1, "server_url": origin, "organization_id": "00000000-0000-4000-8000-000000000001",
                   "project_id": "00000000-0000-4000-8000-000000000002", "bearer_token": "test-key"}
        code = "ovg-pair-v1:" + base64.urlsafe_b64encode(json.dumps(payload).encode()).decode()
        def send(state, request_origin):
            request = urllib.request.Request(
                f"http://127.0.0.1:{flow.port}/callback",
                data=urllib.parse.urlencode({"state": state, "pairing_code": code}).encode(),
                headers={"Origin": request_origin, "Content-Type": "application/x-www-form-urlencoded"},
            )
            with urllib.request.urlopen(request, timeout=3) as response:
                return response.status
        for state, request_origin in [("wrong", origin), (flow.state, "https://evil.example")]:
            with pytest.raises(urllib.error.HTTPError) as exc:
                send(state, request_origin)
            assert exc.value.code == 400
        assert flow.take() is None
        assert send(flow.state, origin) == 200
        assert flow.take() == code
        with pytest.raises(urllib.error.HTTPError):
            send(flow.state, origin)
    finally:
        flow.close()
