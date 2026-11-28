"""Create a fail-closed retention plan from a read-only runtime inventory.

This command never deletes images, volumes, containers, logs or worktrees.
Anything not explicitly protected is emitted for manual owner review only.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

PROTECTED_PREFIXES=("protect-",)

def load(path:Path)->dict:
    raw=path.read_bytes();data=json.loads(raw)
    if not isinstance(data,dict): raise TypeError("inventory must be an object")
    return data,hashlib.sha256(raw).hexdigest()

def classify(data:dict)->dict:
    protected=[];manual=[]
    for section in ("worktrees","containers","images","volumes","opt_candidates"):
        rows=data.get(section,[])
        if not isinstance(rows,list): raise TypeError(f"{section} must be a list")
        for row in rows:
            if not isinstance(row,dict): raise TypeError(f"invalid {section} row")
            retention=row.get("retention")
            item={"section":section,**{k:v for k,v in row.items() if k not in {"image_id"}}}
            if isinstance(retention,str) and retention.startswith(PROTECTED_PREFIXES):
                protected.append(item)
            else:
                manual.append(item)
    return {
        "schema":1,
        "mode":"dry-run-only",
        "protected":protected,
        "manual_review":manual,
        "automatic_delete":[],
        "rules":[
            "No automatic deletion is permitted.",
            "Live/rollback images, active containers, volumes and active worktrees remain protected.",
            "Manual-review entries require explicit owner approval and a fresh runtime inventory before any deletion.",
            "Reclaimable Docker bytes are not evidence that an object is safe to delete.",
        ],
    }

def main()->int:
    p=argparse.ArgumentParser()
    p.add_argument("inventory",type=Path)
    p.add_argument("--output",type=Path)
    a=p.parse_args()
    data,digest=load(a.inventory);plan=classify(data);plan["inventory_sha256"]=digest
    text=json.dumps(plan,ensure_ascii=False,indent=2)+"\n"
    if a.output:a.output.write_text(text)
    else:print(text,end="")
    return 0
if __name__=="__main__": raise SystemExit(main())
