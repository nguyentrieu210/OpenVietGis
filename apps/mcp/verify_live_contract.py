"""Credential-free release gate for the public GIS / remote MCP contract.

Run with --expect-mcp only after the approved deployment. No login, tokens,
tenant data or mutable API calls are involved. The JSON-RPC probe is anonymous.
"""
from __future__ import annotations

import argparse
import json
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import Request, urlopen


def request(url: str, payload: dict | None = None) -> tuple[int, dict]:
    headers = {"Accept": "application/json, text/event-stream", "User-Agent": "OpenVietGIS-Release-Check/1.0"}
    data = None
    if payload is not None:
        data = json.dumps(payload).encode()
        headers["Content-Type"] = "application/json"
        headers["Mcp-Method"] = "tools/list"
    try:
        with urlopen(Request(url, data=data, headers=headers), timeout=12) as response:
            content = response.read(262144)
            return response.status, json.loads(content)
    except HTTPError as error:
        # Do not print error bodies, cookies or headers.
        return error.code, {}


def verify(base: str, expect_mcp: bool) -> None:
    base = base.rstrip("/")
    parsed = urlsplit(base)
    if parsed.scheme != "https" or not parsed.hostname or parsed.path or parsed.query or parsed.fragment or parsed.username or parsed.password:
        raise ValueError("Base must be a fixed HTTPS origin")
    issuer = base + "/realms/openvietgis"
    resource = base + "/mcp"
    status, discovery = request(issuer + "/.well-known/openid-configuration")
    assert status == 200 and discovery["issuer"] == issuer
    for key, suffix in {
        "authorization_endpoint": "/protocol/openid-connect/auth",
        "token_endpoint": "/protocol/openid-connect/token",
        "jwks_uri": "/protocol/openid-connect/certs",
    }.items():
        assert discovery[key] == issuer + suffix, key
    assert "S256" in discovery["code_challenge_methods_supported"]
    print("PASS canonical OIDC issuer and PKCE S256")

    status, health = request(base + "/api/v1/health/ready")
    assert status == 200 and health.get("status") == "ready"
    assert request(base + "/api/v1/me/identity-links")[0] == 401
    empty = "00000000-0000-0000-0000-000000000000"
    assert request(base + f"/api/v1/organizations/{empty}/projects/{empty}/access/mine")[0] == 401
    print("PASS GIS ready and identity / project scope fail closed")

    status, _ = request(resource, {"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}})
    if expect_mcp:
        assert status == 401, f"Expected MCP authentication challenge, got {status}"
        status, metadata = request(base + "/.well-known/oauth-protected-resource/mcp")
        assert status == 200 and metadata["resource"] == resource
        assert issuer in metadata["authorization_servers"]
        print("PASS protected MCP resource metadata and anonymous request denied")
    else:
        assert status == 404, f"MCP is already exposed or unexpectedly routed: {status}"
        print("PASS remote MCP remains gated")


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--base", default="https://openvietgis.cloud")
    cli.add_argument("--expect-mcp", action="store_true")
    settings = cli.parse_args()
    verify(settings.base, settings.expect_mcp)
