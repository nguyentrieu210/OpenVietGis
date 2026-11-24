"""Unit tests for R029 Managed Realtime GIS Client backend & QGIS managed_client."""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from unittest.mock import patch

import pytest
from pydantic import ValidationError

from openvietgis_api.adapters.http.managed_sync import (
    ALLOWED_DESKTOP_TOOLS,
    MUTATING_DESKTOP_TOOLS,
    CreateDesktopCommandRequest,
    CreateManagedSessionRequest,
    ManagedCommitRequest,
)

REPO_ROOT = Path(__file__).resolve().parents[5]
PLUGIN_ROOT = REPO_ROOT / "integrations" / "qgis_plugin"
if str(PLUGIN_ROOT) not in sys.path:
    sys.path.insert(0, str(PLUGIN_ROOT))

from openvietgis_connector.client import OpenVietGisConfig  # noqa: E402
from openvietgis_connector.managed_client import (  # noqa: E402
    bootstrap_managed_layer,
    build_managed_events_url,
    commit_managed_changes,
    create_managed_session,
    fetch_managed_delta,
    list_desktop_commands,
    respond_desktop_command,
)


def test_r029_desktop_tool_allowlist_blocks_arbitrary_execution() -> None:
    assert "inspect_project" in ALLOWED_DESKTOP_TOOLS
    assert "select_features" in ALLOWED_DESKTOP_TOOLS
    assert "stage_edits" in MUTATING_DESKTOP_TOOLS
    for forbidden in ("exec_python", "run_shell", "powershell", "eval", "read_file", "com_exec"):
        assert forbidden not in ALLOWED_DESKTOP_TOOLS


def test_r029_request_models_forbid_extra_fields() -> None:
    req = CreateManagedSessionRequest(device_label="QGIS-Win11", client_version="0.9.0")
    assert req.mode == "managed"
    cmd = CreateDesktopCommandRequest(tool_name="inspect_project", arguments={})
    assert cmd.ttl_seconds == 120
    with pytest.raises(ValidationError):
        ManagedCommitRequest(
            session_id="00000000-0000-0000-0000-000000000001",
            source_id="00000000-0000-0000-0000-000000000002",
            base_content_revision="a" * 64,
            operation_id="op-1",
            operations=[],  # min_length=1
        )


def test_r029_migration_0041_chains_from_0040() -> None:
    mig_path = (
        REPO_ROOT
        / "apps"
        / "api"
        / "migrations"
        / "versions"
        / "0041_r029_managed_realtime_client.py"
    )
    spec = importlib.util.spec_from_file_location("mig_0041", mig_path)
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    assert mod.revision == "0041_r029_managed_realtime"
    assert mod.down_revision == "0040_r027_pairing_and_conflicts"


class _FakeResp:
    def __init__(self, payload: dict) -> None:
        self._raw = json.dumps(payload).encode("utf-8")

    def read(self) -> bytes:
        return self._raw

    def __enter__(self) -> _FakeResp:
        return self

    def __exit__(self, *args: object) -> None:
        return None


def test_managed_client_session_bootstrap_delta_commit_and_commands() -> None:
    cfg = OpenVietGisConfig(
        base_url="https://openvietgis.cloud",
        organization_id="11111111-1111-1111-1111-111111111111",
        project_id="22222222-2222-2222-2222-222222222222",
        bearer_token="ovgsvc_test_token_12345",
    )
    assert "/integration-hub/managed/events?session_id=sid-1" in build_managed_events_url(
        cfg, "sid-1"
    )

    with patch(
        "urllib.request.urlopen",
        return_value=_FakeResp(
            {
                "id": "sid-1",
                "organization_id": cfg.organization_id,
                "project_id": cfg.project_id,
                "state": "active",
                "permission_epoch": 1,
                "last_ack_cursor": 5,
                "current_cursor": 5,
                "expires_at": "2026-12-01T00:00:00Z",
                "capabilities": {"can_edit": True, "can_delete": False},
            }
        ),
    ):
        sess = create_managed_session(cfg)
        assert sess.id == "sid-1"
        assert sess.can_edit is True
        assert sess.can_delete is False

    with patch(
        "urllib.request.urlopen",
        return_value=_FakeResp(
            {
                "layer_id": "33333333-3333-3333-3333-333333333333",
                "source_id": "44444444-4444-4444-4444-444444444444",
                "name": "Parcels",
                "geometry_family": "Polygon",
                "dimensions": "XY",
                "crs_code": 4326,
                "content_revision": "b" * 64,
                "schema_revision": "c" * 64,
                "cursor": 5,
                "permission_epoch": 1,
                "fields": [{"name": "code", "data_type": "text", "nullable": True}],
                "features": [],
                "has_more": False,
                "next_offset": None,
            }
        ),
    ):
        page = bootstrap_managed_layer(
            cfg, layer_id="33333333-3333-3333-3333-333333333333", session_id="sid-1"
        )
        assert page.crs_code == 4326
        assert page.cursor == 5

    with patch(
        "urllib.request.urlopen",
        return_value=_FakeResp(
            {
                "reset_required": False,
                "next_cursor": 7,
                "permission_epoch": 1,
                "has_more": False,
                "changes": [
                    {
                        "cursor": 7,
                        "layer_id": "L1",
                        "source_id": "S1",
                        "content_revision": "d" * 64,
                        "upserts": [],
                        "removals": ["F1"],
                    }
                ],
            }
        ),
    ):
        delta = fetch_managed_delta(cfg, session_id="sid-1", cursor=5)
        assert delta.next_cursor == 7
        assert len(delta.changes) == 1

    with patch(
        "urllib.request.urlopen",
        return_value=_FakeResp(
            {
                "status": "accepted",
                "changeset_id": "cs-1",
                "operation_id": "op-1",
                "source_id": "44444444-4444-4444-4444-444444444444",
                "resulting_content_revision": "d" * 64,
                "operation_count": 1,
                "event_cursor": 8,
                "audit_event_id": "aud-1",
            }
        ),
    ):
        receipt = commit_managed_changes(
            cfg,
            session_id="sid-1",
            source_id="44444444-4444-4444-4444-444444444444",
            base_content_revision="b" * 64,
            base_schema_revision="c" * 64,
            operation_id="op-1",
            operations=[{"op_type": "update", "feature_id": "F1", "properties": {"code": "X"}}],
        )
        assert receipt.event_cursor == 8
        assert receipt.status == "accepted"

    with patch(
        "urllib.request.urlopen",
        return_value=_FakeResp({"items": [{"id": "cmd-1", "tool_name": "inspect_project"}]}),
    ):
        cmds = list_desktop_commands(cfg, session_id="sid-1")
        assert len(cmds) == 1

    with patch(
        "urllib.request.urlopen",
        return_value=_FakeResp({"id": "cmd-1", "status": "completed"}),
    ):
        resp = respond_desktop_command(
            cfg, session_id="sid-1", command_id="cmd-1", status="completed", result={"ok": True}
        )
        assert resp["status"] == "completed"


def test_r029_metadata_writes_use_read_scope_but_gis_commits_require_edit(monkeypatch) -> None:
    """Regression: PROJECT_READ with mutation=True always fails the GIS scope gate."""
    from uuid import uuid4

    from openvietgis_api.adapters.http import managed_sync
    from openvietgis_api.domains.authorization.permissions import Permission

    class ScopeObserved(Exception):
        pass

    calls = []

    def check_scope(session, auth, org, proj, permission, *, mutation):
        calls.append((permission, mutation))
        raise ScopeObserved

    monkeypatch.setattr(managed_sync, "_prep_scope", check_scope)
    org, proj, sid, cmd = (uuid4() for _ in range(4))
    auth, session = object(), object()
    for endpoint, arguments in [
        (managed_sync.create_managed_session, (org, proj, object(), auth, session)),
        (managed_sync.revoke_managed_session, (org, proj, sid, auth, session)),
        (managed_sync.enqueue_desktop_command, (org, proj, sid, object(), auth, session)),
        (managed_sync.respond_desktop_command, (org, proj, sid, cmd, object(), auth, session)),
    ]:
        with pytest.raises(ScopeObserved):
            endpoint(*arguments)
    assert calls == [(Permission.PROJECT_READ, False)] * 4
    with pytest.raises(ScopeObserved):
        managed_sync.commit_managed_changes(org, proj, object(), auth, session)
    assert calls[-1] == (Permission.PROJECT_EDIT, True)


def test_edit_capability_does_not_clear_authenticated_read_scope(monkeypatch) -> None:
    """A rejected EDIT probe must never invalidate the managed session INSERT RLS pin."""
    from uuid import uuid4

    from openvietgis_api.adapters.http import managed_sync
    from openvietgis_api.domains.authorization.permissions import Permission

    org, proj = uuid4(), uuid4()
    session = object()
    actor = object()
    observed = []

    def forbid_rebinding(*args, **kwargs):
        raise AssertionError("capability probe must not rebind or clear RLS")

    class FakeProvider:
        def __init__(self, given_session):
            assert given_session is session

        def resolve(self, context):
            assert context.organization_id == org
            assert context.project_id == proj
            observed.append(True)
            return frozenset({Permission.PROJECT_READ, Permission.PROJECT_EDIT})

    monkeypatch.setattr(managed_sync, "_prep_scope", forbid_rebinding)
    monkeypatch.setattr(managed_sync, "SqlEffectivePermissionProvider", FakeProvider)

    def fake_context(actor, *, organization_id, project_id):
        return type("Scope", (), {"organization_id": organization_id, "project_id": project_id})()

    monkeypatch.setattr(managed_sync, "context_for", fake_context)
    assert managed_sync._can_edit_project(session, actor, org, proj) is True
    assert observed == [True]

    class ReadOnlyProvider(FakeProvider):
        def resolve(self, context):
            return frozenset({Permission.PROJECT_READ})

    monkeypatch.setattr(managed_sync, "SqlEffectivePermissionProvider", ReadOnlyProvider)
    assert managed_sync._can_edit_project(session, actor, org, proj) is False
