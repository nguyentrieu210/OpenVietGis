"""Commit-pinned tracked-path, import and workspace dependency inventory."""
from __future__ import annotations

import json
import posixpath
import re
import subprocess
import sys
import tomllib
from collections import Counter, defaultdict


def git(*args:str)->str:
    return subprocess.check_output(["git",*args],text=True)

ref=git("rev-parse","--verify",(sys.argv[1] if len(sys.argv)>1 else "HEAD")+"^{commit}").strip()
raw=subprocess.check_output(["git","ls-tree","-rlz",ref])
rows=[]
paths=set()
for chunk in raw.decode().split("\0"):
    if not chunk: continue
    meta,path=chunk.split("\t",1); mode,kind,oid,size=meta.split()
    row={"path":path,"mode":mode,"type":kind,"oid":oid,"bytes":int(size) if size.isdigit() else 0}
    rows.append(row);paths.add(path)

def read(path:str)->str:
    return git("show",f"{ref}:{path}")

def owner(path:str)->str:
    for prefix,name in [
        ("apps/api-rust/","api-rust"),("apps/api/","api-python"),
        ("apps/web-next/","web-canonical"),("apps/web/","web-legacy"),
        ("apps/mcp-rust/","mcp-rust"),("apps/mcp/","mcp-python"),
        ("crates/","rust-shared"),("packages/","js-shared"),("modules/","modules"),
        ("integrations/","integrations"),(".github/","ci"),("infra/","infra"),
        ("tools/","tooling"),("tests/","tests"),("specs/","specs"),("docs/","docs")]:
        if path.startswith(prefix): return name
    return "root/other"

exts=(".ts",".tsx",".js",".mjs",".cjs")
edges=[]
unresolved=[]
for p in sorted(x for x in paths if x.endswith(exts)):
    src=read(p)
    specs=set(re.findall(r'(?:from\s*|import\s*\()\s*["\']([^"\']+)["\']',src))
    specs.update(re.findall(r'import\s*["\']([^"\']+)["\']',src))
    for spec in sorted(specs):
        if not spec.startswith("."): continue
        base=posixpath.normpath(posixpath.join(posixpath.dirname(p),spec))
        candidates=[base,*(base+e for e in exts),*(posixpath.join(base,"index"+e) for e in exts)]
        target=next((c for c in candidates if c in paths),None)
        if target: edges.append({"from":p,"to":target})
        else: unresolved.append({"from":p,"specifier":spec})

manifests={}
for p in sorted(paths):
    if p.endswith("Cargo.toml"):
        d=tomllib.loads(read(p))
        manifests[p]={"kind":"cargo","name":d.get("package",{}).get("name"),
          "workspace_members":d.get("workspace",{}).get("members",[]),
          "path_dependencies":{n:v.get("path") for sec in ("dependencies","dev-dependencies","build-dependencies")
            for n,v in d.get(sec,{}).items() if isinstance(v,dict) and v.get("path")}}
    elif p.endswith("package.json"):
        d=json.loads(read(p))
        manifests[p]={"kind":"pnpm","name":d.get("name"),
          "workspace_dependencies":{n:v for sec in ("dependencies","devDependencies","peerDependencies")
            for n,v in d.get(sec,{}).items() if isinstance(v,str) and v.startswith(("workspace:","file:","link:"))},
          "scripts":d.get("scripts",{})}
    elif p.endswith("pyproject.toml"):
        d=tomllib.loads(read(p)); manifests[p]={"kind":"python","name":d.get("project",{}).get("name"),
          "dependencies":d.get("project",{}).get("dependencies",[])}

reverse=Counter(e["to"] for e in edges)
bucket=defaultdict(lambda:{"files":0,"bytes":0})
for r in rows:
    o=owner(r["path"]);bucket[o]["files"]+=1;bucket[o]["bytes"]+=r["bytes"]
report={"sha":ref,"tracked":{"files":len(rows),"bytes":sum(r["bytes"] for r in rows),"rows":rows},
        "owners":dict(sorted(bucket.items())),"relative_import_edges":edges,
        "reverse_import_counts":dict(sorted(reverse.items())),
        "unresolved_relative_imports":unresolved,"manifests":manifests,
        "limitations":["Relative JS/TS static import resolution only; runtime/dynamic string construction is not proven absent.",
                       "Cargo/pnpm/Python manifest graph is declarative and does not prove runtime use.",
                       "A zero reverse-import count is not deletion authorization; Docker, include_str/include_bytes, tests, scripts and external consumers require separate inventory."]}
print(json.dumps(report,ensure_ascii=False,separators=(",",":")))
