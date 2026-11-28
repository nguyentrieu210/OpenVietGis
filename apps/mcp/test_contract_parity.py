"""Cross-language MCP discovery parity required before retiring the Python oracle."""
from __future__ import annotations

import copy
import json
from pathlib import Path

from live_bridge import BridgeStore
from remote_server import create_remote
from remote_workspace import OwnedBridge
from starlette.testclient import TestClient
from test_remote import CONFIG, ORIGIN, VERIFIER, FakeIdentity, signed

ROOT = Path(__file__).resolve().parents[2]
RUST_CONTRACT = ROOT / "apps/mcp-rust/contracts/workspace-tools.json"


def _canonical(value):
    if isinstance(value, dict):
        return {
            key: _canonical(item)
            for key, item in sorted(value.items())
            if key not in {"title", "description", "default"}
        }
    if isinstance(value, list):
        return [_canonical(item) for item in value]
    return value


def _python_tools():
    store = OwnedBridge(BridgeStore(allow_workspace=True))
    headers = {
        "Accept": "application/json, text/event-stream",
        "MCP-Protocol-Version": "2026-07-28",
        "Authorization": "Bearer " + signed(),
        "Mcp-Method": "tools/list",
    }
    meta = {
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
    }
    rpc = {"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {"_meta": meta}}
    with TestClient(
        create_remote(
            CONFIG,
            identity=FakeIdentity(),
            workspace=store,
            verifier=VERIFIER,
            full_tools=True,
        ),
        base_url=ORIGIN,
    ) as client:
        response = client.post("/mcp", json=rpc, headers=headers)
        assert response.status_code == 200, response.text
        return {row["name"]: row for row in response.json()["result"]["tools"]}


def test_python_and_rust_tool_discovery_constraints_match():
    python = _python_tools()
    frozen = {row["name"]: row for row in json.loads(RUST_CONTRACT.read_text())}
    assert len(python) == 39
    assert len(frozen) == 32
    assert set(frozen) <= set(python)

    for name, contract in frozen.items():
        expected = copy.deepcopy(contract["inputSchema"])
        expected.setdefault("properties", {})["session_id"] = {
            "anyOf": [{"type": "string", "format": "uuid"}, {"type": "null"}],
            "default": None,
        }
        expected["additionalProperties"] = False
        assert _canonical(python[name]["inputSchema"]) == _canonical(expected), name
        for key in ("readOnlyHint", "destructiveHint"):
            assert python[name]["annotations"].get(key) == contract["annotations"].get(key), name

    inline = {
        "gis_live_view": ({"session_id"}, set(), True, False),
        "gis_live_selection": ({"session_id"}, set(), True, False),
        "gis_live_navigate": (
            {"longitude", "latitude", "zoom", "session_id"},
            {"longitude", "latitude", "zoom"},
            False,
            False,
        ),
        "gis_operation_status": (
            {"operation_ticket", "session_id"},
            {"operation_ticket"},
            True,
            False,
        ),
        "cad_state": ({"session_id"}, set(), True, False),
        "cad_select_handle": (
            {"document_id", "version", "handle", "session_id"},
            {"document_id", "version", "handle"},
            False,
            False,
        ),
        "gis_workspaces": (set(), set(), True, False),
    }
    assert set(python) - set(frozen) == set(inline)
    for name, (properties, required, readonly, destructive) in inline.items():
        tool = python[name]
        schema = tool["inputSchema"]
        assert schema.get("additionalProperties") is False
        assert set(schema.get("properties", {})) == properties
        assert set(schema.get("required", [])) == required
        assert tool["annotations"].get("readOnlyHint") is readonly
        assert tool["annotations"].get("destructiveHint") is destructive
        if name != "gis_workspaces":
            variants = schema["properties"]["session_id"]["anyOf"]
            assert {item.get("type") for item in variants} == {"string", "null"}


def test_offline_parity_evidence_command_runs_the_same_contract_gate(capsys):
    import runpy
    import subprocess

    # Run the exact CLI entry point in this process. Starting a second MCP/RSA
    # runtime made a functional contract check depend on host paging pressure.
    runpy.run_path(str(ROOT / "tools/benchmark/spec066/mcp-contract-parity.py"), run_name="__main__")
    report = json.loads(capsys.readouterr().out)
    assert report["workspace_schema_mismatches"] == []
    assert report["python_tools"] == 39
    assert report["candidate_sha"] == subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True,
    ).strip()
