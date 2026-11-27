"""Generate deterministic R013 intent fixtures using the actual Python reference."""
import json
from copy import deepcopy
from pathlib import Path
from uuid import UUID

from openvietgis_api.adapters.formats.offline_snapshot import validate_offline_ops
from openvietgis_api.domains.field.domain import offline_core_feature_id
from openvietgis_api.domains.platform_errors import PlatformValidationError

ROOT = Path(__file__).resolve().parents[3]
scope = [UUID(int=i) for i in range(1, 7)]
s = {"organization_id": str(scope[1]), "project_id": str(scope[2]), "source_id": str(scope[4]),
         "schema_revision": "a"*64, "geometry_family": "Point",
         "aoi_bbox": {"min_x": 105,"min_y": 20,"max_x": 106,"max_y": 21},
         "fields": [{"field_id": k,"name": k,"type": t,"nullable": n} for k,t,n in
                 [("v","decimal",False),("i","integer",False),("b","boolean",False),
                  ("t","timestamp",False),("s","text",True)]],
         "features": [{"feature_id": "p1"}]}
p = {"v": "123456789.123456789","i": 42,"b": True,"t": "2026-11-21T00:00:00.123000Z","s": None}
u = {"op_type": "update","feature_id": "p1","properties": p,"client_operation_id": str(UUID(int=8))}
c = {"op_type": "create","feature_id": "p2","properties": p,"client_operation_id": str(UUID(int=9)),
         "geometry": {"type": "Point","coordinates": [105.5,20.5]}}
cases = []
def add(name, ops):
    try:
        validate_offline_ops(s, ops)
        expected = "OK"
    except PlatformValidationError as e:
        expected = e.code
    cases.append({"id": name,"ops": deepcopy(ops),"expected": expected})
add("valid-update-create",[u,c])
add("legacy",[ {k:v for k,v in u.items() if k!="client_operation_id"} ])
add("edge-point",[{**c,"geometry":{"type":"Point","coordinates":[105,21]}}])
add("missing-scope",[{**u,"feature_id":"other"}])
add("geometry-update",[{**u,"geometry":c["geometry"]}])
add("outside-aoi",[{**c,"geometry":{"type":"Point","coordinates":[107,20]}}])
add("boolean-coordinate",[{**c,"geometry":{"type":"Point","coordinates":[True,20]}}])
add("unsupported-delete",[{**u,"op_type":"delete"}])
add("duplicate-feature",[u,u])
add("duplicate-op-id",[u,{**c,"client_operation_id":u["client_operation_id"]}])
add("nil-id",[{**u,"client_operation_id":str(UUID(int=0))}])
add("invalid-feature-id",[{**c,"feature_id":"bad/id"}])
add("unknown-key",[{**u,"injected":True}])
add("empty",[])
add("missing-field",[{**u,"properties":{"v":"1"}}])
for key,values in {"v":["12.50",12.5,"-0","0.1234567890123456789",None],
                   "i":[True,2147483648,1.0],"b":[1,"true"],
                   "t":["2026-11-21T00:00:00.123Z","2026-11-21T00:00:60Z"],
                   "s":["bad\x00text","x"*4097]}.items():
    for idx,value in enumerate(values):
        add(f"invalid-{key}-{idx}",[{**u,"properties":{**p,key:value}}])
keys=("actor_id","organization_id","project_id","package_id","source_id","client_operation_id")
data={"snapshot": s,"cases": cases,"scope": list(map(str,scope)),
          "core_id": offline_core_feature_id(**dict(zip(keys,scope)))}
out=ROOT/"specs/064-rust-platform/evidence/e13-fixtures.json"
out.write_text(json.dumps(data,indent=2,ensure_ascii=False)+"\n")
print(len(cases),"reference cases",sorted({x["expected"] for x in cases}))
