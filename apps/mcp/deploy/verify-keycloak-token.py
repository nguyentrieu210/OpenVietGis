"""Receive an ephemeral test token over stdin; print only non-sensitive validation status."""
import asyncio
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from remote_oauth import OidcConfig,KeycloakVerifier

async def main():
    config=OidcConfig("https://openvietgis.cloud/realms/openvietgis","https://openvietgis.cloud/mcp",frozenset({"openvietgis-mcp-claude"}))
    verifier=KeycloakVerifier(config,jwks_url="http://127.0.0.1:18080/realms/openvietgis/protocol/openid-connect/certs")
    raw=sys.stdin.read().strip()
    token=await verifier.verify_token(raw)
    if token is None:
        import jwt
        claims=jwt.decode(raw,options={"verify_signature":False})
        print("Rejected claim summary:",{key:claims.get(key) for key in ("iss","aud","azp","scope","typ")})
        print("Lifetime:",claims.get("exp",0)-claims.get("iat",0),"subject present:",isinstance(claims.get("sub"),str),"algorithm:",jwt.get_unverified_header(raw).get("alg"))
        try:
            jwt.decode(raw,verifier.signing_key(raw),algorithms=["RS256"],audience=config.resource,issuer=config.issuer,options={"require":["iss","sub","aud","exp","iat","azp","scope"]})
        except Exception as exc:
            print("Verifier error type:",type(exc).__name__,str(exc)[:150])
        raise SystemExit("Actual Keycloak token rejected")
    print("PASS actual Keycloak Authorization Code + PKCE, scope and signed resource audience verified")
asyncio.run(main())
