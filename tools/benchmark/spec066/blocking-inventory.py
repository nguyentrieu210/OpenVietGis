"""Commit-pinned Rust blocking/worker admission inventory for Spec 066."""
from __future__ import annotations

import json
import re
import subprocess
import sys


def git(*args:str)->str:
    return subprocess.check_output(["git",*args],text=True)

ref=git("rev-parse","--verify",(sys.argv[1] if len(sys.argv)>1 else "HEAD")+"^{commit}").strip()
paths=sorted(p for p in git("ls-tree","-r","--name-only",ref).splitlines()
             if p.endswith(".rs") and p.startswith(("apps/api-rust/src/", "crates/")))
def read(p:str)->str:return git("show",f"{ref}:{p}")
rows={"spawn_blocking":[],"subprocess":[],"semaphore":[],"rayon":[]}
for p in paths:
    src=read(p)
    for kind,pattern in [
        ("spawn_blocking",r"\bspawn_blocking\s*\("),
        ("subprocess",r"(?:tokio::process::Command|std::process::Command|Command)::new\s*\("),
        ("semaphore",r"\bSemaphore(?:::new|::const_new|\s*<)"),
        ("rayon",r"(?:ThreadPoolBuilder|par_iter|into_par_iter|rayon::)")]:
        if re.search(pattern,src):
            rows[kind].append({"file":p,"occurrences":len(re.findall(pattern,src))})
report={
 "sha":ref,
 "inventory":rows,
 "executor_defaults":{"max_concurrent_jobs":2,"max_queue_depth":16,"worker_threads":2,"default_timeout_seconds":60},
 "known_worker_bounds":{
   "cad_worker":"Semaphore(WORKERS)",
   "transfer_worker":"Semaphore(WORKERS)",
   "field_offline_snapshot":"Semaphore(2)",
   "field_worker":"Semaphore(2)",
   "provider_worker":"Semaphore(4)",
   "raster_compute":"Semaphore(1)",
   "collaboration_listener":"Semaphore(40)",
   "managed_sync_listener":"Semaphore(40)"
 },
 "evidence_links":["evidence/wkb-benchmark.json"],
 "limitations":["Static source inventory does not prove runtime saturation behavior; executor unit tests are required separately.",
                "Subprocess entries without a shared semaphore can still be bounded by request/operation-specific admission and timeout; review each consumer before changing concurrency."]
}
print(json.dumps(report,separators=(",",":")))
