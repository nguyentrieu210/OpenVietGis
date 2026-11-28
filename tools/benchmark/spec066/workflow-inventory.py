"""Commit-pinned root script and GitHub workflow command matrix."""
from __future__ import annotations

import json
import subprocess
import sys

import yaml


def git(*args:str)->str:return subprocess.check_output(["git",*args],text=True)
ref=git("rev-parse","--verify",(sys.argv[1] if len(sys.argv)>1 else "HEAD")+"^{commit}").strip()
paths=set(git("ls-tree","-r","--name-only",ref).splitlines())
def read(p):return git("show",f"{ref}:{p}")
root=json.loads(read("package.json"))
workflows={}
for p in sorted(x for x in paths if x.startswith(".github/workflows/") and x.endswith((".yml",".yaml"))):
    raw=read(p); data=yaml.safe_load(raw)
    on=data.get("on",data.get(True,{}))
    jobs={}
    for jid,j in (data.get("jobs") or {}).items():
        commands=[];uses=[]
        for step in j.get("steps",[]) or []:
            if "run" in step:commands.append({"name":step.get("name"),"run":step["run"]})
            if "uses" in step:uses.append({"name":step.get("name"),"uses":step["uses"]})
        jobs[jid]={"name":j.get("name"),"needs":j.get("needs"),"if":j.get("if"),
                   "commands":commands,"uses":uses}
    workflows[p]={"name":data.get("name"),"on":on,"concurrency":data.get("concurrency"),"jobs":jobs}
report={"sha":ref,"root_scripts":root.get("scripts",{}),"workflows":workflows,
 "notes":["Branch/ruleset required checks are external repository configuration and recorded separately.",
          "This matrix preserves command bodies for duplicate/gate review; similarity is not proof that a job is redundant."]}
print(json.dumps(report,ensure_ascii=False,separators=(",",":")))
