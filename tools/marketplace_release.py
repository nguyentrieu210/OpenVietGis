"""Developer/operator CLI for reviewed OpenVietGIS Marketplace releases."""
from __future__ import annotations

import argparse
import base64
import json
import os
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, NoEncryption, PrivateFormat, PublicFormat, load_pem_private_key

from openvietgis_api.domains.extensions.marketplace import SCHEMA, canonical, validate_catalog, verify


def read_json(path: str):
    return json.loads(Path(path).read_text())


def write_json(path: str, value):
    out=Path(path)
    temp=out.with_name(out.name+".tmp")
    with temp.open("x",encoding="utf-8") as stream:
        json.dump(value,stream,ensure_ascii=False,indent=2)
        stream.write("\n")
    os.replace(temp,out)


def sign(key_path: str, value) -> str:
    key=load_pem_private_key(Path(key_path).read_bytes(),password=None)
    if not isinstance(key,Ed25519PrivateKey):
        raise ValueError("Ed25519 key required")
    return base64.b64encode(key.sign(canonical(value))).decode()


def main():
    p=argparse.ArgumentParser(description=__doc__)
    sub=p.add_subparsers(dest="command",required=True)
    k=sub.add_parser("keygen")
    k.add_argument("private");k.add_argument("public")
    q=sub.add_parser("publisher-proof")
    q.add_argument("user_id");q.add_argument("name");q.add_argument("public")
    q.add_argument("private");q.add_argument("output")
    s=sub.add_parser("sign-release")
    s.add_argument("publisher");s.add_argument("manifest");s.add_argument("private");s.add_argument("output")
    m=sub.add_parser("merge-reviewed")
    m.add_argument("--base");m.add_argument("--operator-public")
    m.add_argument("--output",required=True);m.add_argument("entries",nargs="+")
    r=sub.add_parser("publish-reviewed")
    r.add_argument("catalog");r.add_argument("private");r.add_argument("output")
    r.add_argument("--reviewed",action="store_true",required=True)
    args=p.parse_args()
    if args.command=="keygen":
        key=Ed25519PrivateKey.generate()
        fd=os.open(args.private,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)
        with os.fdopen(fd,"wb") as stream:
            stream.write(key.private_bytes(Encoding.PEM,PrivateFormat.PKCS8,NoEncryption()))
        with Path(args.public).open("x") as stream:
            stream.write(base64.b64encode(key.public_key().public_bytes(Encoding.Raw,PublicFormat.Raw)).decode()+"\n")
    elif args.command=="publisher-proof":
        from uuid import UUID
        name=args.name.strip()
        public=Path(args.public).read_text().strip()
        registration={"name":name,"public_key":public}
        proof={"purpose":"openvietgis.publisher.v1","user_id":str(UUID(args.user_id)),**registration}
        signature=sign(args.private,proof)
        verify(public,proof,signature)
        write_json(args.output,{**registration,"proof":signature})
    elif args.command=="sign-release":
        value={"publisher_id":args.publisher,"manifest":read_json(args.manifest)}
        write_json(args.output,{**value,"signature":sign(args.private,value)})
    elif args.command=="merge-reviewed":
        payload={"schema":SCHEMA,"publishers":[],"releases":[]}
        if args.base:
            if not args.operator_public:
                raise ValueError("--operator-public required with --base")
            envelope=read_json(args.base)
            if set(envelope)!={"payload","signature"}:
                raise ValueError("invalid base catalog envelope")
            verify(Path(args.operator_public).read_text().strip(),envelope["payload"],envelope["signature"])
            payload=envelope["payload"]
            validate_catalog(payload)
        publishers={x["id"]:x for x in payload["publishers"]}
        releases={(x["manifest"]["extension_key"],x["manifest"]["version"]):x for x in payload["releases"]}
        for filename in args.entries:
            entry=read_json(filename);validate_catalog(entry)
            for pub in entry["publishers"]:
                if pub["id"] in publishers and publishers[pub["id"]]!=pub:
                    raise ValueError("publisher key/name conflict")
                publishers[pub["id"]]=pub
            for rel in entry["releases"]:
                key=(rel["manifest"]["extension_key"],rel["manifest"]["version"])
                if key in releases and releases[key]!=rel:
                    raise ValueError("immutable release conflict")
                releases[key]=rel
        merged={"schema":SCHEMA,"publishers":list(publishers.values()),"releases":list(releases.values())}
        validate_catalog(merged)
        write_json(args.output,merged)
    else:
        payload=read_json(args.catalog)
        validate_catalog(payload)
        write_json(args.output,{"payload":payload,"signature":sign(args.private,payload)})


if __name__=="__main__":
    main()
