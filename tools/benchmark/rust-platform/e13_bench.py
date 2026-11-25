"""Paired validation microbenchmark (preparsed payload; cold = first validation)."""
import json
import resource
from pathlib import Path
import subprocess
import sys
from time import perf_counter_ns
from uuid import UUID

ROOT = Path(__file__).resolve().parents[3]
if len(sys.argv) > 1 and sys.argv[1] == "--python-worker":
    from openvietgis_api.adapters.formats.offline_snapshot import validate_offline_ops
    data = json.load(sys.stdin)
    samples, errors = [], 0
    for _ in range(data["runs"]):
        t = perf_counter_ns()
        try:
            validate_offline_ops(data["snapshot"], data["ops"])
        except Exception:
            errors += 1
        samples.append(perf_counter_ns() - t)
    print(json.dumps(dict(samples_ns=samples, errors=errors, peak_rss_kib=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss)))
    sys.exit(0)

def pct(values, p):
    import math
    return sorted(values)[max(0, math.ceil(len(values)*p)-1)]

fixture = json.loads((ROOT/"specs/064-rust-platform/evidence/e13-fixtures.json").read_text())
snapshot = fixture["snapshot"]
props = fixture["cases"][0]["ops"][0]["properties"]
results = []
for n in (1,100,1000,2000):
    ops = [dict(op_type="create",feature_id=f"capture-{i}",properties=props,
                client_operation_id=str(UUID(int=i+100)),geometry=dict(type="Point",coordinates=[105.5,20.5])) for i in range(n)]
    payload = json.dumps(dict(snapshot=snapshot,ops=ops,runs=101))
    for engine,cmd in (
        ("python",[sys.executable,__file__,"--python-worker"]),
        ("rust-dev",[str(ROOT/"target/debug/examples/e13_validate")])):
        run = subprocess.run(cmd,input=payload,text=True,capture_output=True,check=True)
        row = json.loads(run.stdout)
        samples = row["samples_ns"]
        warm = samples[1:]
        row.update(engine=engine,operations=n,
                   cold_first_ns=samples[0],warm_p50_ns=pct(warm,.50),warm_p95_ns=pct(warm,.95),warm_p99_ns=pct(warm,.99))
        results.append(row)
        print(engine,n,row["warm_p50_ns"],row["warm_p95_ns"],row["warm_p99_ns"],row["errors"],flush=True)
        if row["errors"]: raise RuntimeError("invalid benchmark fixture")
out = dict(method="101 validation calls; first cold, 100 warm; JSON decode excluded, process RSS includes runtime; Rust dev profile; shared VPS; not HTTP or DB", python=sys.version,rows=results)
(ROOT/"specs/064-rust-platform/evidence/e13-benchmark.json").write_text(json.dumps(out,indent=2)+"\n")
