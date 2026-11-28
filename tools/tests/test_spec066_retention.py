import importlib.util
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
SPEC=importlib.util.spec_from_file_location("retention",ROOT/"tools/retention_dry_run.py")
assert SPEC and SPEC.loader
M=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(M)

def test_retention_plan_never_auto_deletes_and_protects_live_objects():
    data={
      "worktrees":[{"worktree":"/opt/ovg-066-work","retention":"protect-active"},{"worktree":"/opt/old","retention":"review-owner-before-delete"}],
      "containers":[{"name":"/openvietgis-live-api-1","retention":"protect-live"}],
      "images":[{"ref":"openvietgis-live-api:latest","retention":"protect-live-or-rollback"}],
      "volumes":[{"name":"openvietgis-live_pgdata","retention":"protect-live-data"}],
      "opt_candidates":[{"path":"/opt/stale","retention":"review-owner-before-delete"}],
    }
    p=M.classify(data)
    assert p["automatic_delete"]==[]
    assert {x["section"] for x in p["protected"]}=={"worktrees","containers","images","volumes"}
    assert {x.get("worktree") or x.get("path") for x in p["manual_review"]}=={"/opt/old","/opt/stale"}

def test_unknown_or_missing_retention_is_manual_review():
    p=M.classify({"worktrees":[{"worktree":"/x"}],"containers":[],"images":[],"volumes":[],"opt_candidates":[]})
    assert p["automatic_delete"]==[]
    assert p["manual_review"][0]["worktree"]=="/x"
