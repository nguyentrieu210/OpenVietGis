"""Commit-pinned HTTP/MCP/migration/test/runtime contract inventory."""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import PurePosixPath


def git(*args:str)->str:return subprocess.check_output(["git",*args],text=True)
ref=git("rev-parse","--verify",(sys.argv[1] if len(sys.argv)>1 else "HEAD")+"^{commit}").strip()
paths=set(git("ls-tree","-r","--name-only",ref).splitlines())
def read(p:str)->str:return git("show",f"{ref}:{p}")

rust=[]
for p in sorted(x for x in paths if x.startswith("apps/api-rust/src/") and x.endswith(".rs")):
    s=read(p)
    for m in re.finditer(r'\.route\(\s*"([^"]+)"\s*,',s):
        tail=s[m.end():m.end()+360]
        methods=sorted(set(re.findall(r'\b(get|post|put|patch|delete|head)\s*\(',tail)))
        rust.append({"path":m.group(1),"methods":methods or ["unknown"],"file":p})
python=[]
for p in sorted(x for x in paths if x.startswith("apps/api/openvietgis_api/") and x.endswith(".py")):
    s=read(p)
    for m in re.finditer(r'@(?:\w+\.)?router\.(get|post|put|patch|delete|head)\(\s*"([^"]+)"',s):
        python.append({"path":m.group(2),"methods":[m.group(1)],"file":p})

def route_keys(rows):
    return {method.upper()+" "+r["path"] for r in rows for method in r["methods"]}
rk,pk=route_keys(rust),route_keys(python)

mcp_contract="apps/mcp-rust/contracts/workspace-tools.json"
mcp=[]
if mcp_contract in paths:
    mcp=json.loads(read(mcp_contract))
for p in ["apps/mcp-rust/src/tools.rs"]:
    if p in paths:
        for name in re.findall(r'json!\(\{"name":"([^"]+)"',read(p)):
            if not any(x.get("name")==name for x in mcp):mcp.append({"name":name,"source":"inline-rust"})
legacy_tools=[]
for p in sorted(x for x in paths if x.startswith("apps/mcp/") and x.endswith(".py")):
    s=read(p)
    legacy_tools += [{"name":n,"file":p} for n in re.findall(r'(?m)^def\s+(gis_[A-Za-z0-9_]+)\s*\(',s)]

migrations=[]
for p in sorted(x for x in paths if x.startswith("apps/api/migrations/versions/") and x.endswith(".py")):
    s=read(p)
    rev=re.search(r'(?m)^revision\s*=\s*"([^"]+)"',s);down=re.search(r'(?m)^down_revision\s*=\s*(?:"([^"]+)"|None)',s)
    migrations.append({"file":p,"revision":rev.group(1) if rev else None,"down_revision":down.group(1) if down and down.group(1) else None})

tests=sorted(p for p in paths if p.startswith("tests/") or "/tests/" in p or re.search(r'\.(?:test|spec)\.',p))
runtime=sorted(p for p in paths if p.startswith(("infra/","deploy/")) and ("Dockerfile" in PurePosixPath(p).name or "compose" in p.lower() or p.endswith((".sh",".yml",".yaml"))))
report={"sha":ref,"http":{"rust":rust,"python":python,
 "rust_keys":sorted(rk),"python_keys":sorted(pk),"common":sorted(rk&pk),
 "rust_only":sorted(rk-pk),"python_only":sorted(pk-rk)},
 "mcp":{"rust_tools":sorted(mcp,key=lambda x:x.get("name","")),"legacy_tool_functions":legacy_tools},
 "migrations":migrations,"tests":{"count":len(tests),"files":tests},"runtime_profiles":runtime,
 "unknowns":["Axum nested prefixes and dynamically built routers may make literal route paths incomplete.",
             "Python routers can be mounted under prefixes not represented in decorator literals.",
             "MCP dynamic registrations outside the checked registry require protocol tests.",
             "Route-set difference is triage evidence, not automatic parity failure or deletion authorization."]}
print(json.dumps(report,ensure_ascii=False,separators=(",",":")))
