from __future__ import annotations

import asyncio
from uuid import uuid4

import pytest
from starlette.requests import Request

from openvietgis_api.adapters.oidc.provider import OidcInvalidResponseError, OidcProviderRegistry
from openvietgis_api.config import get_settings

ISSUER = "http://127.0.0.1:18080/realms/openvietgis"


def _complete(monkeypatch: pytest.MonkeyPatch, token):
    registry = OidcProviderRegistry(get_settings())

    class Client:
        async def authorize_access_token(self, request):
            return token

    monkeypatch.setattr(registry, "_get_client", lambda _name: Client())
    return asyncio.run(registry.complete_login(Request({"type": "http"}), provider_name="keycloak"))


@pytest.mark.parametrize(
    "token",
    [
        {"access_token": "synthetic", "userinfo": {"sub": "user", "iss": ISSUER}},
        {"id_token": "synthetic"},
        {"id_token": "synthetic", "userinfo": {"sub": "user", "iss": "wrong-issuer"}},
        {"id_token": "synthetic", "userinfo": {"sub": None, "iss": ISSUER}},
    ],
)
def test_oidc_registry_requires_transaction_validated_id_token(
    monkeypatch: pytest.MonkeyPatch,
    token,
) -> None:
    with pytest.raises(OidcInvalidResponseError):
        _complete(monkeypatch, token)


def test_oidc_registry_does_not_treat_string_false_as_verified(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    subject = str(uuid4())
    result = _complete(
        monkeypatch,
        {
            "id_token": "synthetic",
            "userinfo": {
                "sub": subject,
                "iss": ISSUER,
                "email": "user@example.test",
                "email_verified": "false",
            },
        },
    )
    assert result.subject == subject
    assert result.issuer == ISSUER
    assert result.email_verified is False
