"""Ensure local Keycloak accepts the isolated E2E callback/origin.

This tool is intentionally localhost-only and uses the fixed development bootstrap
administrator from infra/dev/compose.identity.yaml unless overridden by environment.
"""

from __future__ import annotations

import argparse
import os
import time
from urllib.parse import urlparse

import httpx

_ALLOWED_HOSTS = {"127.0.0.1", "localhost"}


def _local_url(value: str, *, label: str) -> str:
    parsed = urlparse(value)
    if parsed.scheme != "http" or parsed.hostname not in _ALLOWED_HOSTS:
        raise RuntimeError(f"{label} must be an http localhost URL")
    return value.rstrip("/")


def configure(base_url: str, redirect_uri: str, web_origin: str) -> None:
    base = _local_url(base_url, label="base_url")
    redirect = _local_url(redirect_uri, label="redirect_uri")
    origin = _local_url(web_origin, label="web_origin")
    username = os.environ.get(
        "OPENVIETGIS_E2E_KEYCLOAK_ADMIN_USER", "openvietgis-admin"
    )
    password = os.environ.get(
        "OPENVIETGIS_E2E_KEYCLOAK_ADMIN_PASSWORD", "openvietgis-development-only"
    )

    last_error: Exception | None = None
    for _ in range(60):
        try:
            with httpx.Client(timeout=5.0) as client:
                token_response = client.post(
                    f"{base}/realms/master/protocol/openid-connect/token",
                    data={
                        "client_id": "admin-cli",
                        "username": username,
                        "password": password,
                        "grant_type": "password",
                    },
                )
                token_response.raise_for_status()
                token = token_response.json()["access_token"]
                headers = {"Authorization": f"Bearer {token}"}
                clients_response = client.get(
                    f"{base}/admin/realms/openvietgis/clients",
                    params={"clientId": "openvietgis-web"},
                    headers=headers,
                )
                clients_response.raise_for_status()
                clients = clients_response.json()
                if len(clients) != 1:
                    raise RuntimeError("expected exactly one openvietgis-web client")
                record = clients[0]
                redirect_uris = list(record.get("redirectUris") or [])
                web_origins = list(record.get("webOrigins") or [])
                if redirect not in redirect_uris:
                    redirect_uris.append(redirect)
                if origin not in web_origins:
                    web_origins.append(origin)
                record["redirectUris"] = sorted(set(redirect_uris))
                record["webOrigins"] = sorted(set(web_origins))
                update = client.put(
                    f"{base}/admin/realms/openvietgis/clients/{record['id']}",
                    json=record,
                    headers=headers,
                )
                update.raise_for_status()
                print("Local Keycloak E2E redirect/origin verified.")
                return
        except (httpx.HTTPError, KeyError, RuntimeError) as exc:
            last_error = exc
            time.sleep(1)
    raise RuntimeError("local Keycloak did not become configurable") from last_error


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default="http://127.0.0.1:18080")
    parser.add_argument("--redirect-uri", required=True)
    parser.add_argument("--web-origin", required=True)
    args = parser.parse_args()
    configure(args.base_url, args.redirect_uri, args.web_origin)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
