from __future__ import annotations

import re
from typing import Any
from uuid import UUID

from openvietgis_api.domains.audit.domain import AuditEvent
from openvietgis_api.domains.audit.ports import AuditRepository
from openvietgis_api.domains.authorization.context import AuthorizationContext

_REDACTED = "[REDACTED]"
_SENSITIVE_KEY_PARTS = (
    "secret",
    "token",
    "authorization",
    "cookie",
    "password",
    "code_verifier",
    "client_secret",
    "access_token",
    "refresh_token",
    "id_token",
    "bootstrap",
)
_SECRET_PATTERNS = (
    re.compile(r"^ovgs_[0-9a-fA-F]{32}\.s_[A-Za-z0-9_-]{16,}$"),
    re.compile(r"^inv_[A-Za-z0-9_-]{16,}$"),
    re.compile(r"^ovgsvc[0-9a-fA-F]{32}_[A-Za-z0-9_-]{16,}$"),
    re.compile(r"^Bearer\s+.+$", re.IGNORECASE),
    re.compile(r"^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$"),
)


def _sensitive_key(key: str) -> bool:
    normalized = key.lower()
    return any(part in normalized for part in _SENSITIVE_KEY_PARTS)


def _redact_value(value: Any, *, key: str | None = None) -> Any:
    if key is not None and _sensitive_key(key):
        return _REDACTED
    if isinstance(value, dict):
        return {
            str(child_key): _redact_value(child_value, key=str(child_key))
            for child_key, child_value in value.items()
        }
    if isinstance(value, list):
        return [_redact_value(item) for item in value]
    if isinstance(value, tuple):
        return [_redact_value(item) for item in value]
    if isinstance(value, str) and any(
        pattern.fullmatch(value.strip()) for pattern in _SECRET_PATTERNS
    ):
        return _REDACTED
    return value


def redact_audit_metadata(metadata: dict[str, Any]) -> dict[str, Any]:
    return {key: _redact_value(value, key=key) for key, value in metadata.items()}


class AuditService:
    def __init__(self, repository: AuditRepository) -> None:
        self._repository = repository

    def list_events(
        self,
        *,
        context: AuthorizationContext,
        organization_id: UUID,
        limit: int = 50,
    ) -> list[AuditEvent]:
        if limit < 1 or limit > 200:
            raise ValueError("audit limit must be between 1 and 200")
        return self._repository.list_for_organization(
            organization_id,
            context=context,
            limit=limit,
        )
