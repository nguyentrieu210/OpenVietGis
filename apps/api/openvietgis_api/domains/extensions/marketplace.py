"""Offline publisher signatures plus operator-signed reviewed public catalog."""

from __future__ import annotations

import base64
import json
import re
from pathlib import Path
from typing import Any

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

from openvietgis_api.domains.extensions.domain import validate_manifest_contract
from openvietgis_api.domains.platform_errors import PlatformValidationError

SCHEMA = "openvietgis.marketplace.v1"
MANIFEST_FIELDS = {
    "extension_key",
    "version",
    "api_compatibility",
    "display_name",
    "description",
    "permissions",
    "commands",
    "capabilities",
    "entity_schemas",
    "ui_contributions",
}


def canonical(value: Any) -> bytes:
    return json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False
    ).encode()


def verify(public: str, value: Any, signature: str) -> None:
    key = Ed25519PublicKey.from_public_bytes(base64.b64decode(public, validate=True))
    key.verify(base64.b64decode(signature, validate=True), canonical(value))


def validate_catalog(payload: dict[str, Any]) -> list[dict[str, Any]]:
    invalid = PlatformValidationError("invalid marketplace catalog")
    if (
        not isinstance(payload, dict)
        or set(payload) != {"schema", "publishers", "releases"}
        or payload["schema"] != SCHEMA
        or not isinstance(payload["publishers"], list)
        or not isinstance(payload["releases"], list)
        or len(payload["publishers"]) > 100
        or len(payload["releases"]) > 500
    ):
        raise invalid
    publishers = {}
    for pub in payload["publishers"]:
        if (
            not isinstance(pub, dict)
            or set(pub) != {"id", "name", "public_key"}
            or not isinstance(pub["id"], str)
            or not re.fullmatch(r"[a-z][a-z0-9_]{1,31}", pub["id"])
            or pub["id"] in publishers
            or not isinstance(pub["name"], str)
            or not pub["name"].strip()
            or len(pub["name"]) > 120
        ):
            raise invalid
        Ed25519PublicKey.from_public_bytes(base64.b64decode(pub["public_key"], validate=True))
        publishers[pub["id"]] = pub
    seen = set()
    releases = []
    for release in payload["releases"]:
        if not isinstance(release, dict) or set(release) != {
            "publisher_id",
            "manifest",
            "signature",
        }:
            raise invalid
        pid, manifest = release["publisher_id"], release["manifest"]
        if (
            pid not in publishers
            or not isinstance(manifest, dict)
            or set(manifest) != MANIFEST_FIELDS
        ):
            raise invalid
        key, version = manifest["extension_key"], manifest["version"]
        if (
            not isinstance(key, str)
            or not key.startswith("partner_" + pid + "_")
            or not isinstance(version, str)
            or (key, version) in seen
            or not isinstance(manifest["display_name"], str)
            or not manifest["display_name"].strip()
            or len(manifest["display_name"]) > 200
            or not isinstance(manifest["description"], str)
            or len(manifest["description"]) > 500
        ):
            raise invalid
        validate_manifest_contract(
            **{k: v for k, v in manifest.items() if k not in {"display_name", "description"}}
        )
        verify(
            publishers[pid]["public_key"],
            {"publisher_id": pid, "manifest": manifest},
            release["signature"],
        )
        seen.add((key, version))
        releases.append(
            {
                "publisher_id": pid,
                "publisher_name": publishers[pid]["name"],
                "manifest": manifest,
                "signature": release["signature"],
            }
        )
    return releases


def load_catalog(path: str | None, operator_public: str | None) -> list[dict[str, Any]]:
    if not path and not operator_public:
        return []
    if not path or not operator_public:
        raise PlatformValidationError("marketplace trust configuration incomplete")
    try:
        with Path(path).open("rb") as stream:
            data = stream.read(2_000_001)
        if len(data) > 2_000_000:
            raise ValueError("catalog too large")
        envelope = json.loads(data)
        if set(envelope) != {"payload", "signature"}:
            raise ValueError("invalid envelope")
        verify(operator_public, envelope["payload"], envelope["signature"])
        return validate_catalog(envelope["payload"])
    except (OSError, ValueError, TypeError, KeyError, AttributeError, InvalidSignature) as exc:
        raise PlatformValidationError("marketplace signature or catalog invalid") from exc
