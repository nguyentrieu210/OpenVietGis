#!/usr/bin/env python3
"""SPEC 075 V2 publisher/operator signing. Never share the private PEM."""
import argparse
import base64
import hashlib
import json
from pathlib import Path

from marketplace_sign import canonical, private_key, signed, write
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

def sha(data):
    return hashlib.sha256(data).hexdigest()

def read_bounded(path, limit):
    with Path(path).open("rb") as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError("input size limit exceeded")
    return data

def load_json(path, limit):
    def unique(pairs):
        obj = {}
        for key, value in pairs:
            if key in obj:
                raise ValueError("duplicate JSON member")
            obj[key] = value
        return obj
    return json.loads(read_bounded(path, limit), object_pairs_hook=unique)

def publisher(args):
    envelope = load_json(args.envelope, 256 * 1024)
    if envelope.get("dependencies") == []:
        envelope.pop("dependencies")
    manifest = load_json(args.manifest, 128 * 1024)
    data = read_bounded(args.contribution, 256 * 1024)
    key = private_key(args.private)
    raw = key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    if envelope["schema_version"] != 2 or envelope["key_id"] != sha(raw):
        raise ValueError("version or key ID mismatch")
    if envelope["manifest_digest"] != sha(canonical(manifest)):
        raise ValueError("manifest digest mismatch")
    if (envelope["extension_key"], envelope["version"]) != (manifest["extension_key"], manifest["version"]):
        raise ValueError("manifest identity mismatch")
    if not envelope["extension_key"].startswith("partner_" + envelope["publisher_id"] + "_"):
        raise ValueError("publisher namespace mismatch")
    allowed = set(manifest["permissions"])
    if "map_view" in manifest.get("capabilities", []):
        allowed.add("ext." + envelope["extension_key"] + ".processing_plan")
    allowed.update("ext." + envelope["extension_key"] + "." + cap for cap in ("host_context", "host_notice", "map_view") if cap in manifest.get("capabilities", []))
    if not set(envelope["permissions"]).issubset(allowed):
        raise ValueError("permission escalation")
    if envelope["contribution_digest"] != sha(data):
        raise ValueError("contribution digest mismatch")
    assets = {}
    total = 0
    root = Path(args.assets).resolve(strict=True)
    for asset in envelope["assets"]:
        name = asset["id"]
        if not name or name in (".", "..") or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-" for c in name) or name in assets:
            raise ValueError("invalid asset identifier")
        path = (root / name).resolve(strict=True)
        if path.parent != root:
            raise ValueError("asset escapes directory")
        raw = read_bounded(path, 10 * 1024 * 1024 - total)
        total += len(raw)
        if len(raw) != asset["size"] or sha(raw) != asset["sha256"]:
            raise ValueError("asset mismatch")
        assets[name] = base64.b64encode(raw).decode("ascii")
    write(args.output, {"envelope":envelope, "publisher_signature":signed(key,envelope),
                        "contribution":base64.b64encode(data).decode("ascii"),"assets":assets})

def signed_catalog(operator, epoch, releases):
    if not 0 < epoch <= 9007199254740991 or len(releases) > 1000:
        raise ValueError("invalid catalog limits")
    members = [{"envelope_digest": sha(canonical(r["envelope"])),
                "publisher_signature":r["publisher_signature"],
                "operator_signature":r["operator_signature"]} for r in releases]
    membership = {"schema":"openvietgis.marketplace.membership.v2","epoch":epoch,"members":members}
    return {"schema":"openvietgis.marketplace.v2","epoch":epoch,"releases":releases,
            "catalog_signature":signed(operator,membership)}

def review(args):
    bundle = load_json(args.bundle, 16 * 1024 * 1024)
    if set(bundle) != {"envelope","publisher_signature","contribution","assets"}:
        raise ValueError("unexpected bundle fields")
    if not 0 < args.epoch <= 9007199254740991:
        raise ValueError("invalid catalog epoch")
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
    public = base64.b64decode(Path(args.publisher_public).read_text().strip(), validate=True)
    operator = private_key(args.private)
    if public == operator.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw):
        raise ValueError("operator and publisher keys must be distinct")
    if sha(public) != bundle["envelope"]["key_id"]:
        raise ValueError("publisher key mismatch")
    Ed25519PublicKey.from_public_bytes(public).verify(
        base64.b64decode(bundle["publisher_signature"],validate=True), canonical(bundle["envelope"]))
    envelope = bundle["envelope"]
    contribution = base64.b64decode(bundle["contribution"], validate=True)
    if len(contribution) > 256 * 1024 or sha(contribution) != envelope["contribution_digest"]:
        raise ValueError("contribution digest mismatch")
    descriptors = {a["id"]: a for a in envelope["assets"]}
    if len(descriptors) != len(envelope["assets"]) or set(descriptors) != set(bundle["assets"]):
        raise ValueError("asset list mismatch")
    total = 0
    for name, encoded in bundle["assets"].items():
        raw = base64.b64decode(encoded, validate=True)
        total += len(raw)
        if total > 10 * 1024 * 1024 or len(raw) != descriptors[name]["size"] or sha(raw) != descriptors[name]["sha256"]:
            raise ValueError("asset digest mismatch")
    payload = {"schema":"openvietgis.review.v2","envelope_digest":sha(canonical(bundle["envelope"])),"epoch":args.epoch}
    bundle["operator_signature"] = signed(operator,payload)
    write(args.output,signed_catalog(operator,args.epoch,[bundle]))

def assemble(args):
    operator = private_key(args.private)
    releases = []
    seen = set()
    for path in args.catalogs:
        catalog = load_json(path,16*1024*1024)
        if catalog["schema"] != "openvietgis.marketplace.v2" or catalog["epoch"] != args.epoch:
            raise ValueError("catalog epoch mismatch")
        expected = signed_catalog(operator,args.epoch,catalog["releases"])
        if expected["catalog_signature"] != catalog["catalog_signature"]:
            raise ValueError("invalid input membership")
        for release in catalog["releases"]:
            e = release["envelope"]
            identity = (e["extension_key"],e["version"])
            if identity in seen:
                raise ValueError("duplicate release")
            seen.add(identity)
            payload = {"schema":"openvietgis.review.v2","envelope_digest":sha(canonical(e)),"epoch":args.epoch}
            operator.public_key().verify(base64.b64decode(release["operator_signature"],validate=True),canonical(payload))
            if sha(base64.b64decode(release["contribution"],validate=True))!=e["contribution_digest"]:
                raise ValueError("contribution digest mismatch")
            descriptors = {a["id"]:a for a in e["assets"]}
            if len(descriptors)!=len(e["assets"]) or set(descriptors)!=set(release["assets"]):
                raise ValueError("asset list mismatch")
            total=0
            for name,encoded in release["assets"].items():
                raw=base64.b64decode(encoded,validate=True);total+=len(raw)
                if total>10*1024*1024 or len(raw)!=descriptors[name]["size"] or sha(raw)!=descriptors[name]["sha256"]:
                    raise ValueError("asset digest mismatch")
            releases.append(release)
    write(args.output,signed_catalog(operator,args.epoch,releases))

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest="mode",required=True)
    p=sub.add_parser("publisher")
    for name in ("envelope","manifest","contribution","assets","private","output"): p.add_argument(name)
    r=sub.add_parser("review")
    for name in ("bundle","publisher_public","private"): r.add_argument(name)
    r.add_argument("epoch",type=int)
    r.add_argument("output")
    a=sub.add_parser("assemble")
    a.add_argument("private");a.add_argument("epoch",type=int);a.add_argument("output")
    a.add_argument("catalogs",nargs="*")
    args=parser.parse_args()
    # This command is a signing aid, not a substitute for operator moderation.
    {"publisher":publisher,"review":review,"assemble":assemble}[args.mode](args)
if __name__=="__main__":
    main()
