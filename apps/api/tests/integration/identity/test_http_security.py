from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import uuid4

from sqlalchemy import text
from starlette.requests import Request

from openvietgis_api.adapters.http.security import authenticate_request, require_csrf
from openvietgis_api.adapters.persistence.db import get_session_factory
from openvietgis_api.config import get_settings
from openvietgis_api.security.csrf import make_csrf_token
from openvietgis_api.security.sessions import issue_session_token


def _request(*, method: str, cookie: str, csrf: str | None = None) -> Request:
    headers: list[tuple[bytes, bytes]] = [(b"cookie", f"ovg_session={cookie}".encode())]
    if csrf is not None:
        headers.append((b"x-openvietgis-csrf", csrf.encode()))
    return Request(
        {
            "type": "http",
            "method": method,
            "path": "/test",
            "headers": headers,
            "query_string": b"",
            "server": ("testserver", 80),
            "client": ("127.0.0.1", 12345),
            "scheme": "http",
        }
    )


def test_cookie_session_authentication_and_csrf(migrated_database: None) -> None:
    user_id = uuid4()
    identity_link_id = uuid4()
    session_id = uuid4()
    token, digest = issue_session_token(session_id)
    settings = get_settings()

    session = get_session_factory()()
    try:
        session.execute(
            text(
                """
                INSERT INTO identity.users (id, display_name, status, created_at, updated_at)
                VALUES (:id, 'Session User', 'active', now(), now())
                """
            ),
            {"id": user_id},
        )
        session.execute(
            text(
                """
                INSERT INTO identity.identity_links (
                    id, user_id, issuer, subject, email_verified, created_at
                )
                VALUES (
                    :id, :user_id, 'https://issuer.example', :subject, true, now()
                )
                """
            ),
            {
                "id": identity_link_id,
                "user_id": user_id,
                "subject": f"subject-{identity_link_id}",
            },
        )
        session.execute(
            text(
                """
                INSERT INTO identity.web_sessions (
                    id, user_id, identity_link_id, secret_hash,
                    issued_at, expires_at
                )
                VALUES (
                    :id, :user_id, :identity_link_id, :secret_hash,
                    :issued_at, :expires_at
                )
                """
            ),
            {
                "id": session_id,
                "user_id": user_id,
                "identity_link_id": identity_link_id,
                "secret_hash": digest,
                "issued_at": datetime.now(UTC),
                "expires_at": datetime.now(UTC) + timedelta(hours=1),
            },
        )
        session.commit()

        get_request = _request(method="GET", cookie=token)
        authentication = authenticate_request(get_request, session, settings)
        assert authentication.actor.actor_id == user_id
        assert authentication.identity_link_id == identity_link_id

        csrf_secret = settings.csrf_secret
        assert csrf_secret is not None
        csrf = make_csrf_token(token, csrf_secret.get_secret_value())
        post_request = _request(method="POST", cookie=token, csrf=csrf)
        verified = require_csrf(post_request, authentication, settings)
        assert verified.session_id == session_id
    finally:
        session.close()


def test_unlink_identity_invalid_payload_is_safe_422(migrated_database: None) -> None:
    from tests.fixtures.identity_tenancy import seed_access_world
    from tests.integration.rls.test_cross_tenant_api import client_for_user

    world = seed_access_world()
    with client_for_user(world.owner_user_id) as client:
        for payload in ({}, {"identity_link_id": "not-a-uuid"}, {"extra": "synthetic-input"}):
            response = client.request("DELETE", "/api/v1/me/identity-links", json=payload)
            assert response.status_code == 422
            assert "synthetic-input" not in response.text
            assert "not-a-uuid" not in response.text
