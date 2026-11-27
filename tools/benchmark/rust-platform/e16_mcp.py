"""Isolated baseline/candidate benchmark; no live service or real credentials."""
import argparse
import asyncio
import json
import resource
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path


def percentile(samples, p):
    samples = sorted(samples)
    return round(samples[max(0, (len(samples) * p + 99) // 100 - 1)], 3)

def worker(source, count):
    start = time.perf_counter_ns()
    sys.path.insert(0, str(Path(source).resolve()))
    from bridge_http import body
    from live_bridge import BridgeStore
    from starlette.requests import Request
    from test_remote import VERIFIER, signed
    token = signed()
    def parse():
        async def receive():
            return {"type": "http.request", "body": b'{"longitude":106.6,"latitude":10.6,"zoom":15}', "more_body": False}
        request = Request({"type":"http", "headers":[(b"content-type",b"application/json")]}, receive)
        return asyncio.run(body(request))
    def cycle():
        store = BridgeStore()
        session = store.create()
        sid, browser, ai = (session[k] for k in ("session_id","browser_token","ai_token"))
        cid = store.submit(sid,ai,"map_navigate",{"longitude":106.6,"latitude":10.6,"zoom":15})["command_id"]
        assert store.poll(sid,browser)["command"]["id"] == cid
        store.ack(sid,browser,cid,True,None)
        assert store.result(sid,ai,cid) == {"state":"done","result":None}
    def auth():
        assert VERIFIER._verify(token).subject == "alice"
    def request():
        assert parse()["zoom"] == 15
    auth(); request(); cycle()
    cold_ms = (time.perf_counter_ns()-start)/1e6
    rows=[]
    for name, fn in (("signed_token",auth),("request_json",request),("relay_lifecycle",cycle)):
        for concurrency in (1,4):
            def sample(_, operation=fn):
                t=time.perf_counter_ns()
                try:
                    operation()
                    return (time.perf_counter_ns()-t)/1e6,False
                except Exception:  # noqa: BLE001 -- benchmark counts all tool failures
                    return (time.perf_counter_ns()-t)/1e6,True
            with ThreadPoolExecutor(max_workers=concurrency) as pool:
                results=list(pool.map(sample, range(count)))
            samples=[t for t,_ in results]
            rows.append({"operation":name,"concurrency":concurrency,"samples":count,
                "p50_ms":percentile(samples,50),"p95_ms":percentile(samples,95),
                "p99_ms":percentile(samples,99),"errors":sum(err for _,err in results),
                "raw_ms":[round(t,6) for t in samples]})
    print(json.dumps({"cold_import_and_first_operations_ms":round(cold_ms,3),
                      "peak_rss_kib":resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                      "measurements":rows}))

if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--worker")
    parser.add_argument("--baseline")
    parser.add_argument("--candidate")
    parser.add_argument("--count",type=int,default=100)
    args=parser.parse_args()
    if args.worker:
        worker(args.worker,args.count)
    else:
        result={}
        for label,source in (("baseline",args.baseline),("candidate",args.candidate)):
            result[label]=[json.loads(subprocess.check_output([sys.executable,__file__,"--worker",source,"--count",str(args.count)])) for _ in range(3)]
        print(json.dumps(result,indent=2))
