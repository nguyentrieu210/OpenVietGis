"""Receive an ephemeral test token over stdin; print only non-sensitive validation status."""

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from remote_oauth import KeycloakVerifier, OidcConfig


async def main():
    import os

    client = os.environ.get("OVG_TEST_MCP_CLIENT", "openvietgis-mcp-claude")
    assert client in {"openvietgis-mcp-claude", "openvietgis-mcp-chatgpt"}
    config = OidcConfig(
        "https://openvietgis.cloud/realms/openvietgis",
        "https://openvietgis.cloud/mcp",
        frozenset({client}),
    )
    verifier = KeycloakVerifier(
        config,
        jwks_url="http://127.0.0.1:18080/realms/openvietgis/protocol/openid-connect/certs",
    )
    raw = sys.stdin.read().strip()
    token = await verifier.verify_token(raw)
    if token is None:
        import jwt

        claims = jwt.decode(raw, options={"verify_signature": False})
        print(
            "Rejected claim summary:",
            {key: claims.get(key) for key in ("iss", "aud", "azp", "scope", "typ")},
        )
        print(
            "Lifetime:",
            claims.get("exp", 0) - claims.get("iat", 0),
            "subject present:",
            isinstance(claims.get("sub"), str),
            "algorithm:",
            jwt.get_unverified_header(raw).get("alg"),
        )
        try:
            jwt.decode(
                raw,
                verifier.signing_key(raw),
                algorithms=["RS256"],
                audience=config.resource,
                issuer=config.issuer,
                options={
                    "require": ["iss", "sub", "aud", "exp", "iat", "azp", "scope"]
                },
            )
        except Exception as exc:
            print("Verifier error type:", type(exc).__name__, str(exc)[:150])
        raise SystemExit("Actual Keycloak token rejected")
    print(
        "PASS actual Keycloak Authorization Code + PKCE, scope and signed resource audience verified"
    )
    import json
    import subprocess

    request = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": {},
            }
        },
    }
    headers = f"Authorization: Bearer {raw}\nAccept: application/json, text/event-stream\nMcp-Method: tools/list\nMCP-Protocol-Version: 2026-07-28\n"
    response = subprocess.run(
        [
            "curl",
            "--silent",
            "--show-error",
            "--fail",
            "--resolve",
            "openvietgis.cloud:443:127.0.0.1",
            "--header",
            "@-",
            "--header",
            "Content-Type: application/json",
            "--data-binary",
            json.dumps(request),
            config.resource,
        ],
        input=headers,
        text=True,
        capture_output=True,
        check=True,
    )
    names = {tool["name"] for tool in json.loads(response.stdout)["result"]["tools"]}
    import os

    from workspace_tools import METHODS

    expected = {"gis_live_view", "gis_live_selection", "gis_live_navigate"}
    if os.getenv("OVG_EXPECT_FULL_TOOLS") == "1":
        expected |= METHODS | {"gis_operation_status"}
    assert names == expected, names
    print("PASS production HTTPS MCP tools/list:", ", ".join(sorted(names)))


asyncio.run(main())
