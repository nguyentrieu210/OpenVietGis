#!/usr/bin/env python3
"""Issue an authority-signed commercial certificate offline; never a payment receipt."""
import argparse
import hashlib
import json
from datetime import datetime,timezone
from pathlib import Path
from uuid import UUID
from marketplace_sign import private_key,public_key,signed,write
import base64

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("authority_private");p.add_argument("certificate_json");p.add_argument("output")
    a=p.parse_args()
    key=private_key(a.authority_private)
    g=json.loads(Path(a.certificate_json).read_text())
    fields={"schema","audience","authority_key_id","grant_id","organization_id","extension_key","release_digest","sequence","kind","issued_at","valid_from","valid_until","grace_until"}
    if set(g)!=fields or g["schema"]!="openvietgis.license.grant.v1" or g["kind"] not in ("trial","subscription","revoked"):
        raise ValueError("invalid certificate fields")
    if g["authority_key_id"]!=hashlib.sha256(base64.b64decode(public_key(key))).hexdigest():raise ValueError("authority mismatch")
    for field in ("grant_id","organization_id"):g[field]=str(UUID(g[field]))
    for field in ("issued_at","valid_from","valid_until","grace_until"):
        t=datetime.fromisoformat(g[field].replace("Z","+00:00"))
        if t.tzinfo is None or t.microsecond:raise ValueError("use timezone-aware whole-second times")
        g[field]=t.astimezone(timezone.utc).isoformat(timespec="seconds").replace("+00:00","Z")
    write(a.output,{"certificate":g,"signature":signed(key,g)})
if __name__=="__main__":main()
