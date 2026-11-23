from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any, Protocol, cast

from authlib.integrations.starlette_client import OAuth  # type: ignore[import-untyped]
from joserfc.errors import JoseError
from starlette.requests import Request
from starlette.responses import Response

from openvietgis_api.config import OidcProviderSettings, Settings
from openvietgis_api.domains.identity.service import ExternalIdentityClaims


@dataclass(frozen=True, slots=True)
class OidcProviderSummary:
    id: str
    display_name: str


class OidcProviderNotFoundError(LookupError):
    pass


class OidcInvalidResponseError(ValueError):
    pass


class _OidcClient(Protocol):
    async def authorize_redirect(
        self,
        request: Request,
        redirect_uri: str,
        *,
        state: str,
        **kwargs: Any,
    ) -> Response: ...

    async def authorize_access_token(self, request: Request) -> Mapping[str, Any]: ...


class OidcProviderRegistry:
    def __init__(self, settings: Settings) -> None:
        self._providers = settings.oidc_providers
        self._oauth = OAuth()
        for name, provider in self._providers.items():
            self._register(name, provider)

    def _register(self, name: str, provider: OidcProviderSettings) -> None:
        kwargs: dict[str, object] = {
            "client_id": provider.client_id,
            "server_metadata_url": (
                f"{provider.issuer.rstrip('/')}/.well-known/openid-configuration"
            ),
            "client_kwargs": {
                "scope": "openid email profile",
                "code_challenge_method": "S256",
            },
        }
        if provider.client_secret is not None:
            kwargs["client_secret"] = provider.client_secret.get_secret_value()
        self._oauth.register(name, **kwargs)

    def summaries(self) -> list[OidcProviderSummary]:
        return [
            OidcProviderSummary(id=name, display_name=provider.display_name)
            for name, provider in sorted(self._providers.items())
        ]

    def _get_client(self, name: str) -> _OidcClient:
        if name not in self._providers:
            raise OidcProviderNotFoundError(name)
        client = self._oauth.create_client(name)
        if client is None:
            raise OidcProviderNotFoundError(name)
        return cast(_OidcClient, client)

    async def begin_login(
        self,
        request: Request,
        *,
        provider_name: str,
        redirect_uri: str,
        state: str,
        prompt: str | None = None,
        action: str | None = None,
    ) -> Response:
        client = self._get_client(provider_name)
        params = {"prompt": "login"} if prompt == "login" else {}
        if action == "UPDATE_PASSWORD" and provider_name == "keycloak":
            params["kc_action"] = action
        return await client.authorize_redirect(request, redirect_uri, state=state, **params)

    async def complete_login(
        self,
        request: Request,
        *,
        provider_name: str,
    ) -> ExternalIdentityClaims:
        provider = self._providers.get(provider_name)
        if provider is None:
            raise OidcProviderNotFoundError(provider_name)
        client = self._get_client(provider_name)
        try:
            token = await client.authorize_access_token(request)
        except JoseError as exc:
            raise OidcInvalidResponseError("invalid OIDC ID token") from exc
        # Authlib supplies userinfo here only after parsing the OIDC ID token,
        # including the nonce stored for this authorization state. Do not fall back
        # to an OAuth userinfo request lacking the transaction-bound ID token.
        userinfo = token.get("userinfo")
        if not token.get("id_token") or not isinstance(userinfo, Mapping):
            raise OidcInvalidResponseError("missing validated OIDC identity")
        subject = userinfo.get("sub")
        if not isinstance(subject, str) or not subject:
            raise OidcInvalidResponseError("invalid provider subject")
        if userinfo.get("iss") != provider.issuer:
            raise OidcInvalidResponseError("invalid provider issuer")
        email = userinfo.get("email")
        display_name = (
            userinfo.get("name") or userinfo.get("preferred_username") or email or subject
        )
        return ExternalIdentityClaims(
            issuer=provider.issuer,
            subject=subject,
            display_name=str(display_name),
            email=str(email) if email is not None else None,
            email_verified=userinfo.get("email_verified") is True,
        )
