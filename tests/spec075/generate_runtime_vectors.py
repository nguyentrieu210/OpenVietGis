"""Deterministic wire fixtures; synthetic scope and approval, never credentials."""
import copy
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
def write():
    docs=ROOT/"packages/plugin-sdk/examples/v2"
    cases=[]
    request="30000000-0000-0000-0000-000000000001"
    for i,(name,cap) in enumerate([("context","host_context"),("notice","host_notice"),("layers","map_view"),("processing","processing_plan")],1):
        doc=json.loads((docs/f"host-{name}.json").read_text())
        key=f"partner_fixture_{name}"
        manifest=f"10000000-0000-0000-0000-{i:012d}"
        digest=f"{i:064x}"
        runtime={"document":doc,"manifest_id":manifest,"release_digest":digest,"activation_revision":5,"profile":"web_read_only","granted":True,"grant_revision":"12","permissions":[f"ext.{key}.{cap}"],"lock_digest":"b"*64,"catalog_epoch":2,"dependency_lock":[{"key":key,"version":"1.0.0","manifest_id":manifest,"release_digest":digest,"activation_id":f"20000000-0000-0000-0000-{i:012d}","activation_revision":5}]}
        action=doc["actions"][0]
        args={k:("fixture" if k!="request" else '{"algorithm_id":"buffer"}') for k,v in action["input"].items() if v["required"]}
        expected={k:runtime[k] for k in ("release_digest","activation_revision","grant_revision","lock_digest","catalog_epoch")}
        expected.update(request_id=request,action_id=action["id"],input=args)
        cases.append({"name":name,"runtime":runtime,"manifest":manifest,"action":action["id"],"input":args,"request_id":request,"expected":expected})
    base=cases[0]["runtime"]
    invalid=[]
    for name,patch in [
        ("unqualified_profile",{"profile":"native"}),
        ("zero_granted_revision",{"grant_revision":"0"}),
        ("overflow_revision",{"grant_revision":"9223372036854775808"}),
        ("noncanonical_revision",{"grant_revision":"012"}),
        ("duplicate_permission",{"permissions":base["permissions"]*2}),
        ("duplicate_dependency",{"dependency_lock":base["dependency_lock"]*2}),
        ("wrong_root_release",{"release_digest":"c"*64}),
        ("unexpected_field",{"credential":"not-a-real-credential"}),
    ]:
        invalid.append({"name":name,"manifest":base["manifest_id"],"runtime":copy.deepcopy(base)|patch})
    output={"schema":"openvietgis.fixture.runtime-wire.v1","valid":cases,"invalid":invalid}
    (ROOT/"fixtures/spec075/runtime-wire-vectors.json").write_text(json.dumps(output,ensure_ascii=False,indent=2)+"\n")
if __name__=="__main__":write()
