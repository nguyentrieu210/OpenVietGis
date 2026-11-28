#!/usr/bin/env python3
"""Generate a dual-signed publisher rotation request locally."""
import argparse
from marketplace_sign import private_key,public_key,signed,write
from uuid import UUID

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("user_id");p.add_argument("publisher_id");p.add_argument("expected_revision",type=int)
    p.add_argument("old_private");p.add_argument("new_private");p.add_argument("output")
    a=p.parse_args()
    old=private_key(a.old_private);new=private_key(a.new_private)
    old_pub=public_key(old);new_pub=public_key(new)
    if old_pub==new_pub or not 1<=a.expected_revision<9007199254740991:raise ValueError("new key and valid revision required")
    payload={"purpose":"openvietgis.publisher.rotate.v1","user_id":str(UUID(a.user_id)),"publisher_id":a.publisher_id,
        "expected_revision":a.expected_revision,"old_public_key":old_pub,"new_public_key":new_pub}
    write(a.output,{k:v for k,v in payload.items() if k not in ("purpose","user_id")}|{"old_proof":signed(old,payload),"new_proof":signed(new,payload)})
if __name__=="__main__":main()
