"""Compare independently built APK payloads, excluding signing metadata only."""
import argparse,json,pathlib
p=argparse.ArgumentParser();p.add_argument("first",type=pathlib.Path);p.add_argument("second",type=pathlib.Path);a=p.parse_args()
x,y=[json.loads(p.read_text()) for p in (a.first,a.second)]
if x["candidate_sha"]!=y["candidate_sha"] or x["profile"]!=y["profile"]:raise SystemExit("candidate mismatch")
a1={v["path"]:v["sha256"] for v in x["payload"]};a2={v["path"]:v["sha256"] for v in y["payload"]}
diff=sorted(k for k in a1.keys()|a2.keys() if a1.get(k)!=a2.get(k))
print(json.dumps({"equal":not diff,"different":diff},indent=2));raise SystemExit(bool(diff))
