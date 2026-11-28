#!/usr/bin/env python3
"""Offline SPEC075 trust authority tool. Root private key never belongs on API hosts."""
import argparse
import base64
import hashlib
import re
from datetime import datetime, timedelta, timezone
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat
from contribution_sign import load_json
from marketplace_sign import canonical, private_key, signed, write

MAX_SAFE=9007199254740991
FIELDS=("revoked_release_digests","revoked_publisher_keys","revoked_operator_keys")
def sha(data):return hashlib.sha256(data).hexdigest()
def public(key):return key.public_bytes(Encoding.Raw,PublicFormat.Raw)
def decode(value,length):
    raw=base64.b64decode(value,validate=True)
    if len(raw)!=length or base64.b64encode(raw).decode("ascii")!=value:
        raise ValueError("invalid canonical base64")
    return raw
def timestamp(value):
    result=datetime.fromisoformat(value.replace("Z","+00:00"))
    if result.tzinfo is None:raise ValueError("timezone required")
    return result.astimezone(timezone.utc)
def sign_policy(args):
    root=private_key(args.root_private)
    op_raw=decode(Path(args.operator_public).read_text().strip(),32)
    if public(root.public_key())==op_raw:raise ValueError("distinct trust root required")
    op=Ed25519PublicKey.from_public_bytes(op_raw)
    catalog=load_json(args.catalog,16*1024*1024)
    epoch=catalog["epoch"]
    if catalog["schema"]!="openvietgis.marketplace.v2" or type(epoch) is not int or not 1<=epoch<=MAX_SAFE or len(catalog["releases"])>100:
        raise ValueError("invalid catalog")
    members=[]
    for r in catalog["releases"]:
        digest=sha(canonical(r["envelope"]))
        op.verify(decode(r["operator_signature"],64),canonical({"schema":"openvietgis.review.v2","envelope_digest":digest,"epoch":epoch}))
        members.append({"envelope_digest":digest,"publisher_signature":r["publisher_signature"],"operator_signature":r["operator_signature"]})
    membership={"schema":"openvietgis.marketplace.membership.v2","epoch":epoch,"members":members}
    op.verify(decode(catalog["catalog_signature"],64),canonical(membership))
    revocations=load_json(args.revocations,1024*1024)
    if set(revocations)!=set(FIELDS):raise ValueError("invalid revocation fields")
    for k in FIELDS:
        values=revocations[k]
        if not isinstance(values,list) or len(values)>4096 or any(not isinstance(v,str) or not re.fullmatch("[a-f0-9]{64}",v) for v in values) or len(set(values))!=len(values):
            raise ValueError("invalid revocation list")
        revocations[k]=sorted(values)
    issued=timestamp(args.issued);expires=timestamp(args.expires)
    if not 1<=args.sequence<=MAX_SAFE or not timedelta(0)<expires-issued<=timedelta(days=30):raise ValueError("invalid sequence or validity")
    policy={"schema":"openvietgis.trust.policy.v1","sequence":args.sequence,"catalog_epoch":epoch,
        "catalog_membership_digest":sha(canonical(membership)),"operator_key_id":sha(op_raw),"issued_at":args.issued,"expires_at":args.expires,**revocations}
    if policy["operator_key_id"] in policy["revoked_operator_keys"]:raise ValueError("operator revoked")
    commercial=getattr(args,"commercial",None)
    if commercial:
        c=load_json(commercial,1024*1024)
        if set(c)!={"audience","authority_key_id","licensed_release_digests"} or not re.fullmatch("[A-Za-z0-9_.-]{1,128}",c["audience"]):
            raise ValueError("invalid commercial policy")
        ids=c["licensed_release_digests"]
        if not isinstance(ids,list) or not 1<=len(ids)<=4096 or len(set(ids))!=len(ids) or any(not re.fullmatch("[a-f0-9]{64}",v) for v in ids):
            raise ValueError("invalid licensed releases")
        if not re.fullmatch("[a-f0-9]{64}",c["authority_key_id"]) or c["authority_key_id"] in (sha(op_raw),sha(public(root.public_key()))):
            raise ValueError("independent license authority required")
        c["licensed_release_digests"]=sorted(ids)
        policy["commercial"]=c
    if args.previous:
        previous=load_json(args.previous,1024*1024)
        old=previous["policy"]
        root.public_key().verify(decode(previous["signature"],64),canonical(old))
        if policy["sequence"]<=old["sequence"] or epoch<old["catalog_epoch"] or (epoch==old["catalog_epoch"] and policy["catalog_membership_digest"]!=old["catalog_membership_digest"]):
            raise ValueError("policy rollback or reused catalog epoch")
        if old.get("commercial"):
            new=policy.get("commercial")
            if not new or new["audience"]!=old["commercial"]["audience"] or not set(old["commercial"]["licensed_release_digests"]).issubset(new["licensed_release_digests"]):
                raise ValueError("commercial classification removal")
        if any(not set(old[k]).issubset(policy[k]) for k in FIELDS):raise ValueError("revocation removal")
        if policy["operator_key_id"]!=old["operator_key_id"] and (epoch<=old["catalog_epoch"] or old["operator_key_id"] not in policy["revoked_operator_keys"]):raise ValueError("old operator must be revoked")
    elif args.sequence!=1:
        raise ValueError("supply previous signed policy when advancing sequence")
    result={"policy":policy,"signature":signed(root,policy)}
    write(args.output,result)
    return result
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root_private");parser.add_argument("operator_public");parser.add_argument("catalog");parser.add_argument("revocations")
    parser.add_argument("sequence",type=int);parser.add_argument("issued");parser.add_argument("expires");parser.add_argument("output")
    parser.add_argument("--previous");parser.add_argument("--commercial")
    sign_policy(parser.parse_args())
if __name__=="__main__":main()
