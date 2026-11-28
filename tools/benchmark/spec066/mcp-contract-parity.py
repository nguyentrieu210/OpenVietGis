"""Offline Python/Rust MCP contract parity report for Spec 066.

Uses the retained adapter's in-process OAuth/test identity fixture. No network,
production credentials or live GIS session is used.
"""
from __future__ import annotations

import copy
import json
import subprocess
import sys
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/"apps/mcp"))

from live_bridge import BridgeStore
from remote_server import create_remote
from remote_workspace import OwnedBridge
from starlette.testclient import TestClient
from test_remote import CONFIG, ORIGIN, VERIFIER, FakeIdentity, signed

contract={x["name"]:x for x in json.loads((ROOT/"apps/mcp-rust/contracts/workspace-tools.json").read_text())}
headers={"Accept":"application/json, text/event-stream","MCP-Protocol-Version":"2026-07-28",
         "Authorization":"Bearer "+signed(),"Mcp-Method":"tools/list"}
meta={"io.modelcontextprotocol/protocolVersion":"2026-07-28",
      "io.modelcontextprotocol/clientCapabilities":{}}
with TestClient(create_remote(CONFIG,identity=FakeIdentity(),
        workspace=OwnedBridge(BridgeStore(allow_workspace=True)),
        verifier=VERIFIER,full_tools=True),base_url=ORIGIN) as client:
    response=client.post("/mcp",json={"jsonrpc":"2.0","id":1,"method":"tools/list",
        "params":{"_meta":meta}},headers=headers)
    response.raise_for_status()
    tools={x["name"]:x for x in response.json()["result"]["tools"]}

mismatches=[]
for name,expected in contract.items():
    expected=copy.deepcopy(expected)
    # tools::Registry applies this fail-closed rule to every frozen schema.
    expected["inputSchema"]["additionalProperties"]=False
    actual=tools[name]
    schema=json.loads(json.dumps(actual["inputSchema"]))
    schema.get("properties",{}).pop("session_id",None)
    if "required" in schema:
        schema["required"]=[x for x in schema["required"] if x!="session_id"]
    if (schema!=expected["inputSchema"] or
        actual.get("description")!=expected.get("description") or
        actual.get("annotations")!=expected.get("annotations")):
        mismatches.append(name)

report={
    "candidate_sha":subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),
    "python_tools":len(tools),
    "rust_registry_tools":39,
    "workspace_contract_tools":len(contract),
    "workspace_schema_mismatches":mismatches,
    "shared_optional_scope_field":"session_id",
    "runtime_schema_rule":"Rust Registry and Python both set root additionalProperties=false.",
    "extra_shared_tools":sorted(set(tools)-set(contract)),
    "protocols":{"current":"2026-07-28","legacy":"2025-11-25"},
    "method":"In-process retained Python MCP tools/list with signed fixture OAuth; compared to checked-in Rust workspace contracts with the Registry root additionalProperties=false rule and after removing the shared optional root session_id field.",
    "live_network":False,
}
if report["python_tools"]!=39 or mismatches:
    raise SystemExit(json.dumps(report,indent=2))
print(json.dumps(report,indent=2))
