"""Read-only VPS retention inventory. Never deletes or inspects environment secrets."""
from __future__ import annotations

import json
import subprocess
import time
from pathlib import Path


def run(*args:str)->str:
    p=subprocess.run(args,text=True,capture_output=True,check=False)
    return p.stdout.strip()

def du(path:str)->int|None:
    out=run("du","-sb",path)
    try:return int(out.split()[0])
    except (ValueError,IndexError):return None

worktrees=[]
raw=run("git","worktree","list","--porcelain").splitlines()
cur={}
for line in raw+[""]:
    if not line:
        if cur:worktrees.append(cur);cur={}
    else:
        k,*rest=line.split(" ",1);cur[k]=rest[0] if rest else True
for w in worktrees:
    path=str(w.get("worktree",""))
    w["bytes"]=du(path)
    w["retention"]="protect-active" if any(x in path for x in ("ovg-066-work","ovg-065-work","openvietgis.new")) else "review-owner-before-delete"

containers=[]
for cid in run("docker","ps","-aq").splitlines():
    if not cid:continue
    out=run("docker","inspect","--format",'{{json .Name}}|{{json .Image}}|{{json .State.Status}}|{{json .Config.Image}}',cid)
    if not out:continue
    name,image,status,image_ref=[json.loads(x) for x in out.split("|")]
    containers.append({"name":name,"image_id":image,"status":status,"image_ref":image_ref,
      "retention":"protect-live" if name.startswith("/openvietgis-live") else "review-owner-before-delete"})
images=[]
for line in run("docker","image","ls","--no-trunc","--format",'{{json .Repository}}|{{json .Tag}}|{{json .ID}}|{{json .Size}}').splitlines():
    if not line:continue
    repo,tag,iid,size=[json.loads(x) for x in line.split("|")]
    ref=f"{repo}:{tag}"
    retention="protect-live-or-rollback" if repo.startswith("openvietgis-live") else "review-owner-before-delete"
    images.append({"ref":ref,"id":iid,"size":size,"retention":retention})
volumes=[]
for name in run("docker","volume","ls","--format","{{.Name}}").splitlines():
    if not name:continue
    volumes.append({"name":name,"retention":"protect-live-data" if name.startswith("openvietgis-live") else "review-owner-before-delete"})
opt=[]
for p in sorted(Path("/opt").iterdir()):
    if p.name.startswith(("ovg","openvietgis")):
        opt.append({"path":str(p),"bytes":du(str(p)),
                    "retention":"protect-active" if p.name in {"ovg-066-work","ovg-065-work","openvietgis.new"} else "review-owner-before-delete"})
report={"observed_unix":int(time.time()),"worktrees":worktrees,"containers":containers,"images":images,"volumes":volumes,
        "opt_candidates":opt,"docker_df":run("docker","system","df"),
        "policy":["No deletion is authorized by this inventory.","Protect all live containers, live/rollback images and live volumes.",
                  "Protect active 065/066 worktrees and current deployment checkout.",
                  "Unknown ownership or inactive-looking cache/artifact requires explicit owner review before deletion."]}
print(json.dumps(report,ensure_ascii=False,separators=(",",":")))
