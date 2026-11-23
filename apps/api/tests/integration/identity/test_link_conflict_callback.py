from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
from urllib.parse import parse_qs, urlencode, urlparse
from uuid import uuid4

import pytest
from fastapi.responses import RedirectResponse
from fastapi.testclient import TestClient
from sqlalchemy import create_engine, text
from sqlalchemy.orm import Session
from tools.test_database import assert_safe_test_target

from openvietgis_api.adapters.http import auth
from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.identity_repository import SqlIdentityRepository
from openvietgis_api.adapters.persistence.rls import set_request_context
from openvietgis_api.config import get_settings
from openvietgis_api.domains.identity.service import (
    ExternalIdentityClaims,
    IdentityLinkConflictError,
    IdentityService,
)
from openvietgis_api.main import create_app


def _claims() -> ExternalIdentityClaims:
    return ExternalIdentityClaims(
        issuer="https://issuer.example",
        subject=f"subject-{uuid4()}",
        display_name="Link Conflict Test",
        email=f"test-{uuid4()}@example.test",
        email_verified=True,
    )


def _service(session: Session) -> IdentityService:
    return IdentityService(SqlIdentityRepository(session), SqlAuditRepository(session))


def test_oidc_callback_preserves_denied_audit_and_does_not_change_links(
    migrated_database: None,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    settings = get_settings()
    migration_url = settings.migration_db_url
    assert_safe_test_target(migration_url)
    engine = create_engine(migration_url)
    try:
        with Session(engine) as setup:
            service = _service(setup)
            actor = service.login(_claims(), session_ttl_seconds=3600)
            foreign_claims = _claims()
            foreign = service.login(foreign_claims, session_ttl_seconds=3600)
            setup.commit()

        # Replace only provider I/O, not authentication, persistence or request
        # transaction lifecycle. The callback still checks a real active session.
        class Registry:
            async def begin_login(self, request, *, provider_name, redirect_uri, state):
                return RedirectResponse(
                    "https://issuer.example/authorize?" + urlencode({"state": state}),
                    status_code=302,
                )

            async def complete_login(self, request, *, provider_name):
                return foreign_claims

        monkeypatch.setattr(auth, "_registry", lambda _settings: Registry())
        with TestClient(create_app()) as client:
            client.cookies.set(settings.session_cookie_name, actor.session_token)
            response = client.get(
                "/api/v1/auth/login/keycloak",
                params={"mode": "link"},
                headers={
                    "Sec-Fetch-Site": "same-origin",
                    "Sec-Fetch-Mode": "navigate",
                    "Sec-Fetch-User": "?1",
                },
                follow_redirects=False,
            )
            assert response.status_code == 302
            state = parse_qs(urlparse(response.headers["location"]).query)["state"][0]
            callback = client.get(
                "/api/v1/auth/callback/keycloak",
                params={"state": state, "code": "synthetic-code"},
                follow_redirects=False,
            )
            assert callback.status_code == 400
            assert callback.json() == {"detail": "external identity cannot be linked"}
            replay = client.get(
                "/api/v1/auth/callback/keycloak",
                params={"state": state, "code": "synthetic-code"},
                follow_redirects=False,
            )
            assert replay.status_code == 400

        with Session(engine) as verify:
            repository = SqlIdentityRepository(verify)
            assert repository.list_identity_links(actor.user.id) == [actor.identity_link]
            assert repository.list_identity_links(foreign.user.id) == [foreign.identity_link]
            audit = (
                verify.execute(
                    text(
                        """
                    SELECT organization_id, actor_id, actor_display_snapshot, outcome,
                           resource_id, metadata
                    FROM audit.audit_events
                    WHERE actor_id = :actor_id AND action = 'identity.link'
                    """
                    ),
                    {"actor_id": actor.user.id},
                )
                .mappings()
                .one()
            )
            assert audit["organization_id"] is None
            assert audit["actor_id"] == actor.user.id
            assert audit["actor_display_snapshot"] == actor.user.display_name
            assert audit["outcome"] == "denied"
            assert audit["resource_id"] is None
            assert audit["metadata"] == {"reason": "identity_conflict"}
            assert (
                verify.execute(
                    text("SELECT count(*) FROM identity.web_sessions WHERE user_id = :user_id"),
                    {"user_id": actor.user.id},
                ).scalar_one()
                == 1
            )
    finally:
        engine.dispose()


def test_concurrent_distinct_users_linking_one_identity_audits_loser(
    migrated_database: None,
) -> None:
    migration_url = get_settings().migration_db_url
    assert_safe_test_target(migration_url)
    engine = create_engine(migration_url)
    try:
        with Session(engine) as setup:
            first = _service(setup).login(_claims(), session_ttl_seconds=3600)
            second = _service(setup).login(_claims(), session_ttl_seconds=3600)
            setup.commit()

        claimed = _claims()
        barrier = Barrier(2)

        def attempt(user_id):
            with Session(engine) as session:
                session.execute(text("SET LOCAL ROLE openvietgis_runtime"))
                set_request_context(session, organization_id=None, actor_user_id=user_id)
                barrier.wait(timeout=10)
                try:
                    _service(session).link_identity(user_id=user_id, claims=claimed)
                except IdentityLinkConflictError:
                    # Mirrors the callback's intentional, denial-only commit.
                    session.commit()
                    return user_id, "denied"
                session.commit()
                return user_id, "success"

        with ThreadPoolExecutor(max_workers=2) as pool:
            futures = [pool.submit(attempt, user.id) for user in (first.user, second.user)]
            results = [future.result(timeout=30) for future in futures]

        assert sorted(outcome for _user_id, outcome in results) == ["denied", "success"]
        winner = next(user_id for user_id, outcome in results if outcome == "success")
        loser = next(user_id for user_id, outcome in results if outcome == "denied")
        with Session(engine) as verify:
            current = SqlIdentityRepository(verify).find_by_external_identity(
                issuer=claimed.issuer,
                subject=claimed.subject,
            )
            assert current is not None
            assert current[0].id == winner
            audit = (
                verify.execute(
                    text(
                        """
                    SELECT outcome, resource_id, metadata
                    FROM audit.audit_events
                    WHERE actor_id = :actor_id AND action = 'identity.link'
                    """
                    ),
                    {"actor_id": loser},
                )
                .mappings()
                .one()
            )
            assert audit["outcome"] == "denied"
            assert audit["resource_id"] is None
            assert audit["metadata"] == {"reason": "identity_conflict"}
    finally:
        engine.dispose()
