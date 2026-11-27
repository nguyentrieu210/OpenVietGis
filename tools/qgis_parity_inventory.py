"""Static candidate inventory; this does not assert route reachability or parity."""
import argparse
import json
import re
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]

def scan(root,glob,pattern):
    found=[]
    for p in sorted(root.rglob(glob)):
        if any(part in {"node_modules","dist","target"} for part in p.parts):
            continue
        for n,line in enumerate(p.read_text(encoding="utf-8").splitlines(),1):
            for m in re.finditer(pattern,line):
                found.append({"id":m.group(1),"path":str(p.relative_to(ROOT)),"line":n})
    return found

def inventory():
    manifest=json.loads((ROOT/"specs/065-qgis-ux-parity/evidence/manifest.json").read_text())
    return {
        "kind":"static candidate inventory; reachability/permissions need runtime tests",
        "commands":scan(ROOT/"apps/web-next/src","*.tsx",r'\balias\s*:\s*"([A-Z][A-Z_0-9]+)"'),
        "routes":scan(ROOT/"apps/api-rust/src","*.rs",r'\.route\s*\(\s*"([^"]+)"'),
        "route_templates":scan(ROOT/"apps/api-rust/src","*.rs",r'\.route\s*\(\s*&format!\(\s*"([^"]+)"'),
        "capability_references":scan(ROOT/"apps/web-next/src","*.tsx",r'\.name\s*===\s*"([a-z_][a-z_0-9]+)"'),
        "matrix_candidates":[{"id":r["id"],"task":r["task"],"status":r["status"],"paths":r["candidate_paths"],"blocker":r["blocker"]} for r in manifest["rows"]]
    }

if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--out",type=Path,required=True)
    args=parser.parse_args()
    data=inventory()
    args.out.parent.mkdir(parents=True,exist_ok=True)
    args.out.write_text(json.dumps(data,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    print("Candidate inventory:",len(data["commands"]),"command occurrences;",len(data["routes"])+len(data["route_templates"]),"literal route occurrences;",len(data["matrix_candidates"]),"matrix rows")
