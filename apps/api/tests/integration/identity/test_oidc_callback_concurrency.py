"""Concurrent callbacks must not block the API loop or duplicate a verified identity."""

import asyncio
from collections.abc import Iterator
from contextlib import contextmanager
from urllib.parse import parse_qs, urlencode, urlparse
from uuid import UUID, uuid4

import httpx
import pytest
from fastapi.responses import RedirectResponse
from sqlalchemy import create_engine, text
from sqlalchemy.orm import Session
from tools.test_database import assert_safe_test_target

from openvietgis_api.adapters.http import auth
from openvietgis_api.adapters.http.security import get_db_session
from openvietgis_api.adapters.persistence.db import session_scope
from openvietgis_api.config import get_settings
from openvietgis_api.domains.identity.service import ExternalIdentityClaims
from openvietgis_api.main import create_app


def test_concurrent_first_login_same_identity_one_user_two_sessions(
    migrated_database: None,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    claims = ExternalIdentityClaims(
        issuer="https://issuer.example",
        subject=f"concurrent-{uuid4()}",
        display_name="Concurrent Login",
        email=None,
        email_verified=False,
    )

    async def exercise() -> str:
        arrived = 0
        both_verified = asyncio.Event()

        class Registry:
            async def begin_login(self, request, *, provider_name, redirect_uri, state):
                return RedirectResponse(
                    "https://issuer.example/authorize?" + urlencode({"state": state}),
                    status_code=302,
                )

            async def complete_login(self, request, *, provider_name):
                nonlocal arrived
                arrived += 1
                if arrived == 2:
                    both_verified.set()
                await asyncio.wait_for(both_verified.wait(), timeout=5)
                return claims

        monkeypatch.setattr(auth, "_registry", lambda _settings: Registry())
        app = create_app()

        # Retain the real restricted runtime transaction and commit/rollback
        # lifecycle, but bound lock waits so a regression cannot hang the suite.
        def bounded_session() -> Iterator[Session]:
            with contextmanager(session_scope)() as session:
                session.execute(text("SET LOCAL lock_timeout = '5s'"))
                yield session

        app.dependency_overrides[get_db_session] = bounded_session
        transport = httpx.ASGITransport(app=app)
        async with (
            httpx.AsyncClient(transport=transport, base_url="https://testserver") as first,
            httpx.AsyncClient(transport=transport, base_url="https://testserver") as second,
        ):

            async def login(client: httpx.AsyncClient) -> str:
                response = await client.get("/api/v1/auth/login/keycloak")
                assert response.status_code == 302
                state = parse_qs(urlparse(response.headers["location"]).query)["state"][0]
                callback = await client.get(
                    "/api/v1/auth/callback/keycloak",
                    params={"state": state, "code": "synthetic-code"},
                )
                assert callback.status_code == 302
                me = await client.get("/api/v1/me")
                assert me.status_code == 200
                return str(me.json()["id"])

            first_id, second_id = await asyncio.wait_for(
                asyncio.gather(login(first), login(second)), timeout=20
            )
        assert arrived == 2
        assert first_id == second_id
        return first_id

    user_id = UUID(asyncio.run(exercise()))
    migration_url = get_settings().migration_db_url
    assert_safe_test_target(migration_url)
    engine = create_engine(migration_url)
    try:
        with engine.connect() as conn:
            assert (
                conn.execute(
                    text(
                        "SELECT count(*) FROM identity.identity_links "
                        "WHERE issuer = :issuer AND subject = :subject AND user_id = :user_id"
                    ),
                    {
                        "issuer": claims.issuer,
                        "subject": claims.subject,
                        "user_id": user_id,
                    },
                ).scalar_one()
                == 1
            )
            assert (
                conn.execute(
                    text("SELECT count(*) FROM identity.web_sessions WHERE user_id = :user_id"),
                    {"user_id": user_id},
                ).scalar_one()
                == 2
            )
    finally:
        engine.dispose()
