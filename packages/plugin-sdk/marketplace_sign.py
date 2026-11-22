#!/usr/bin/env python3
"""OpenVietGIS developer signing CLI — no Core checkout or API connection required.

Install: python -m pip install cryptography
Never upload or commit the private PEM. Only send registration.json/release.json.
"""

from __future__ import annotations

import argparse
import base64
import json
import os
from pathlib import Path
from uuid import UUID

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import (
    Encoding,
    NoEncryption,
    PrivateFormat,
    PublicFormat,
    load_pem_private_key,
)


def canonical(value):
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
        allow_nan=False,
    ).encode()


def private_key(path):
    key = load_pem_private_key(Path(path).read_bytes(), password=None)
    if not isinstance(key, Ed25519PrivateKey):
        raise TypeError("Ed25519 private key required")
    return key


def public_key(key):
    return base64.b64encode(
        key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    ).decode()


def signed(key, value):
    return base64.b64encode(key.sign(canonical(value))).decode()


def write(path, value):
    with Path(path).open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    generate = sub.add_parser("keygen")
    generate.add_argument("private")
    generate.add_argument("public")
    proof = sub.add_parser("publisher-proof")
    for arg in ("user_id", "name", "public", "private", "output"):
        proof.add_argument(arg)
    release = sub.add_parser("sign-release")
    for arg in ("publisher", "manifest", "private", "output"):
        release.add_argument(arg)
    args = parser.parse_args()
    if args.command == "keygen":
        key = Ed25519PrivateKey.generate()
        fd = os.open(args.private, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        with os.fdopen(fd, "wb") as stream:
            stream.write(
                key.private_bytes(Encoding.PEM, PrivateFormat.PKCS8, NoEncryption())
            )
        with Path(args.public).open("x", encoding="utf-8") as stream:
            stream.write(public_key(key) + "\n")
    elif args.command == "publisher-proof":
        key = private_key(args.private)
        registration = {
            "name": args.name.strip(),
            "public_key": Path(args.public).read_text().strip(),
        }
        if not registration["name"] or public_key(key) != registration["public_key"]:
            raise ValueError("Name and matching public key required")
        body = {
            "purpose": "openvietgis.publisher.v1",
            "user_id": str(UUID(args.user_id)),
            **registration,
        }
        write(args.output, {**registration, "proof": signed(key, body)})
    else:
        manifest = json.loads(Path(args.manifest).read_text())
        if not manifest["extension_key"].startswith("partner_" + args.publisher + "_"):
            raise ValueError("Wrong publisher namespace")
        body = {"publisher_id": args.publisher, "manifest": manifest}
        write(
            args.output, {**body, "signature": signed(private_key(args.private), body)}
        )


if __name__ == "__main__":
    main()
