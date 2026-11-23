from __future__ import annotations

import hashlib
import hmac
import secrets
from uuid import UUID


def generate_secret(prefix: str, *, bytes_of_entropy: int = 32) -> str:
    if not prefix or "_" in prefix:
        raise ValueError("secret prefix must be non-empty and cannot contain underscore")
    return f"{prefix}_{secrets.token_urlsafe(bytes_of_entropy)}"


def digest_secret(secret: str) -> bytes:
    return hashlib.sha256(secret.encode("utf-8")).digest()


def verify_secret(secret: str, expected_digest: bytes) -> bool:
    return hmac.compare_digest(digest_secret(secret), expected_digest)


def safe_prefix(secret: str, *, visible_chars: int = 12) -> str:
    if visible_chars < 4:
        raise ValueError("visible_chars must be at least 4")
    return secret[:visible_chars]


SERVICE_CREDENTIAL_TOKEN_PREFIX = "ovgsvc"


def generate_service_credential_secret(credential_id: UUID) -> str:
    return generate_secret(f"{SERVICE_CREDENTIAL_TOKEN_PREFIX}{credential_id.hex}")


def parse_service_credential_id(secret: str) -> UUID:
    prefix, separator, _ = secret.partition("_")
    if not separator or not prefix.startswith(SERVICE_CREDENTIAL_TOKEN_PREFIX):
        raise ValueError("invalid service credential")
    raw_id = prefix[len(SERVICE_CREDENTIAL_TOKEN_PREFIX) :]
    if len(raw_id) != 32:
        raise ValueError("invalid service credential")
    try:
        return UUID(hex=raw_id)
    except ValueError as exc:
        raise ValueError("invalid service credential") from exc
