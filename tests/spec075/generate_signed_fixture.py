"""Reproducible Python→Rust fixture. These deterministic keys are TEST ONLY.
Never configure a real or shared deployment with their public keys."""
import base64
import hashlib
import json
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, NoEncryption, PrivateFormat, PublicFormat

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "packages/plugin-sdk"))
from marketplace_sign import canonical, public_key, signed
from contribution_sign import publisher, review

def sha(data):
    return hashlib.sha256(data).hexdigest()

def generate():
    pk = Ed25519PrivateKey.from_private_bytes(bytes([41])*32)
    op = Ed25519PrivateKey.from_private_bytes(bytes([42])*32)
    v1, v2 = [], []
    with tempfile.TemporaryDirectory() as directory:
        d=Path(directory)
        for path, key in ((d/"pub.pem",pk),(d/"op.pem",op)):
            path.write_bytes(key.private_bytes(Encoding.PEM,PrivateFormat.PKCS8,NoEncryption()))
            path.chmod(0o600)
        (d/"pub.key").write_text(public_key(pk))
        for name, cap in (("context","host_context"),("notice","host_notice"),("layers","map_view")):
            key="partner_fixture_"+name
            manifest={"extension_key":key,"version":"1.0.0","api_compatibility":"1.x",
                      "display_name":"Fixture "+name,"description":"Synthetic test only","permissions":[],"commands":[],
                      "capabilities":[cap],"entity_schemas":{},
                      "ui_contributions":[{"id":"partner_panel","slot":"project_panel","label":"Fixture "+name,
                                           "route_suffix":"/partner","runtime":"sandboxed_iframe","url":"https://example.com/"+name+"/","sdk_version":"1.0"}]}
            legacy={"publisher_id":"fixture","manifest":manifest}
            v1.append({**legacy,"signature":signed(pk,legacy)})
            doc=ROOT/"packages/plugin-sdk/examples/v2"/("host-"+name+".json")
            envelope={"schema_version":2,"publisher_id":"fixture","extension_key":key,"version":"1.0.0",
                      "manifest_digest":sha(canonical(manifest)),"host_api_range":"^1","platforms":["web"],
                      "permissions":["ext."+key+"."+cap],"contribution_digest":sha(doc.read_bytes()),"assets":[],
                      "issued_at":"2026-11-29T00:00:00Z","key_id":sha(pk.public_key().public_bytes(Encoding.Raw,PublicFormat.Raw))}
            (d/"manifest.json").write_text(json.dumps(manifest))
            (d/"envelope.json").write_text(json.dumps(envelope))
            bundle=d/(name+".bundle.json"); output=d/(name+".catalog.json")
            publisher(SimpleNamespace(envelope=d/"envelope.json",manifest=d/"manifest.json",contribution=doc,assets=d,private=d/"pub.pem",output=bundle))
            review(SimpleNamespace(bundle=bundle,publisher_public=d/"pub.key",private=d/"op.pem",epoch=2,output=output))
            v2.extend(json.loads(output.read_text())["releases"])
    catalog={"schema":"openvietgis.marketplace.v1","publishers":[{"id":"fixture","name":"Fixture publisher","public_key":public_key(pk)}],"releases":v1}
    return {"operator_public_key":public_key(op),"v1":{"payload":catalog,"signature":signed(op,catalog)},"v2":{"schema":"openvietgis.marketplace.v2","epoch":2,"releases":v2}}

if __name__=="__main__":
    data=json.dumps(generate(),ensure_ascii=False,sort_keys=True,indent=2)+"\n"
    target=ROOT/"fixtures/spec075/python-signed-catalog.json"
    if "--check" in sys.argv:
        assert target.read_text()==data, "fixture drift"
        print("SPEC075_PYTHON_SIGNING_PASS")
    else:
        target.write_text(data)
