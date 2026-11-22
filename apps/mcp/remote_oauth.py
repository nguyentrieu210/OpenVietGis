"""Verify existing Keycloak access tokens; never accept provider login/id tokens as GIS grants."""
from __future__ import annotations
import asyncio
import time
from dataclasses import dataclass
from urllib.parse import urlsplit

import jwt
from mcp.server.auth.provider import AccessToken, TokenVerifier

SCOPE = "gis:live"

@dataclass(frozen=True)
class OidcConfig:
    issuer: str
    resource: str
    clients: frozenset[str]

    def __post_init__(self):
        for value in (self.issuer, self.resource):
            p = urlsplit(value)
            if p.scheme != "https" or not p.hostname or p.username or p.password or p.query or p.fragment:
                raise ValueError("OIDC issuer and MCP resource must be fixed HTTPS URLs")
        if not self.clients or any(not c or len(c)>200 for c in self.clients):
            raise ValueError("Explicit approved OAuth client allowlist required")


class KeycloakVerifier(TokenVerifier):
    def __init__(self, config: OidcConfig, signing_key=None):
        self.config = config
        if signing_key is None:
            client = jwt.PyJWKClient(config.issuer.rstrip("/") + "/protocol/openid-connect/certs", timeout=5, lifespan=300)
            signing_key = lambda token: client.get_signing_key_from_jwt(token).key
        self.signing_key = signing_key

    def _verify(self, token):
        if len(token)>12000:
            return None
        try:
            header=jwt.get_unverified_header(token)
            if header.get("alg")!="RS256" or not isinstance(header.get("kid"),str):
                return None
            claims=jwt.decode(token,self.signing_key(token),algorithms=["RS256"],
                audience=self.config.resource,issuer=self.config.issuer,
                options={"require":["iss","sub","aud","exp","iat","azp","scope"]})
            if (claims.get("typ")!="Bearer" or claims["azp"] not in self.config.clients
                or not isinstance(claims["sub"],str) or not claims["sub"]
                or not isinstance(claims["scope"],str) or SCOPE not in claims["scope"].split()
                or not isinstance(claims["iat"],int) or claims["exp"]-claims["iat"]>3600):
                return None
            return AccessToken(token=token,client_id=claims["azp"],scopes=[SCOPE],
                expires_at=int(claims["exp"]),subject=claims["sub"],resource=self.config.resource)
        except (jwt.PyJWTError,ValueError,TypeError,KeyError,TimeoutError,OSError):
            return None

    async def verify_token(self, token):
        return await asyncio.to_thread(self._verify,token)
