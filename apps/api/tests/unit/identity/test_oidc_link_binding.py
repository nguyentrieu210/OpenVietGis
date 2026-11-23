from __future__ import annotations

from types import SimpleNamespace
from urllib.parse import parse_qs, urlencode, urlparse
from uuid import UUID, uuid4

import pytest
from fastapi import HTTPException, Request
from fastapi.responses import RedirectResponse
from fastapi.testclient import TestClient

from openvietgis_api.adapters.http import auth
from openvietgis_api.adapters.http.security import AuthenticatedRequest, get_db_session
from openvietgis_api.domains.authorization.context import Actor
from openvietgis_api.domains.identity.service import (
    ExternalIdentityClaims,
    IdentityConflictError,
    IdentityLinkConflictError,
)
from openvietgis_api.main import create_app


@pytest.fixture
def oidc_client(monkeypatch: pytest.MonkeyPatch):
    first_user = uuid4()
    first_session = uuid4()
    sessions = {
        "first": (first_user, first_session),
        "same-user-new-session": (first_user, uuid4()),
        "other-user": (uuid4(), uuid4()),
    }
    linked: list[UUID] = []
    exchanged: list[str] = []
    active = [True]
    error = [None]
    transactions: list[str] = []

    class Registry:
        async def begin_login(self, request, *, provider_name, redirect_uri, state):
            return RedirectResponse(
                "https://issuer.example/authorize?" + urlencode({"state": state}),
                status_code=302,
            )

        async def complete_login(self, request, *, provider_name):
            exchanged.append(provider_name)
            return ExternalIdentityClaims(
                issuer="https://issuer.example",
                subject="second-provider-subject",
                display_name="Linked Identity",
                email=None,
                email_verified=False,
            )

    class Service:
        def link_identity(self, *, user_id, claims):
            if error[0] is not None:
                raise error[0]
            linked.append(user_id)

        def login(self, *_args, **_kwargs):
            raise AssertionError("A rejected link must never fall back to account login")

    class Repository:
        def __init__(self, session):
            pass

        def lock_active_web_session(self, *, user_id, session_id):
            assert (user_id, session_id) == sessions["first"]
            return active[0]

    def authenticate(request: Request, _session, _settings) -> AuthenticatedRequest:
        identity = sessions.get(request.cookies.get("ovg_session", ""))
        if identity is None:
            raise HTTPException(status_code=401, detail="invalid session")
        return AuthenticatedRequest(
            actor=Actor(actor_type="human", actor_id=identity[0], display_name="First User"),
            session_id=identity[1],
            raw_session_token=request.cookies["ovg_session"],
        )

    def fake_session():
        try:
            yield object()
        except Exception:
            transactions.append("rolled_back")
            raise
        else:
            transactions.append("committed")

    monkeypatch.setattr(auth, "_registry", lambda _settings: Registry())
    monkeypatch.setattr(auth, "_identity_service", lambda _session: Service())
    monkeypatch.setattr(auth, "authenticate_request", authenticate)
    monkeypatch.setattr(auth, "SqlIdentityRepository", Repository)
    app = create_app()
    app.dependency_overrides[get_db_session] = fake_session
    with TestClient(app) as client:
        client.cookies.set("ovg_session", "first")
        yield SimpleNamespace(
            client=client,
            user=first_user,
            linked=linked,
            exchanged=exchanged,
            active=active,
            sessions=sessions,
            error=error,
            transactions=transactions,
        )


def _begin(client: TestClient, *, mode: str = "link") -> str:
    response = client.get(
        "/api/v1/auth/login/keycloak",
        params={"mode": mode, "return_to": "/organizations"},
        headers={
            "Sec-Fetch-Site": "same-origin",
            "Sec-Fetch-Mode": "navigate",
            "Sec-Fetch-User": "?1",
        },
        follow_redirects=False,
    )
    assert response.status_code == 302
    return parse_qs(urlparse(response.headers["location"]).query)["state"][0]


def _callback(client: TestClient, state: str, *, provider: str = "keycloak"):
    return client.get(
        f"/api/v1/auth/callback/{provider}",
        params={"state": state, "code": "synthetic-code"},
        follow_redirects=False,
    )


def test_link_reauthenticates_exact_session_and_consumes_state(oidc_client) -> None:
    state = _begin(oidc_client.client)
    result = _callback(oidc_client.client, state)
    assert result.status_code == 302
    assert oidc_client.linked == [oidc_client.user]
    assert _callback(oidc_client.client, state).status_code == 400
    assert oidc_client.linked == [oidc_client.user]


@pytest.mark.parametrize("replacement", ["same-user-new-session", "other-user", "logged-out"])
def test_link_rejects_changed_or_revoked_session(oidc_client, replacement: str) -> None:
    state = _begin(oidc_client.client)
    oidc_client.client.cookies.set("ovg_session", replacement)
    assert _callback(oidc_client.client, state).status_code == 400
    assert oidc_client.linked == []


def test_link_rejects_session_revoked_before_mutation(oidc_client) -> None:
    state = _begin(oidc_client.client)
    oidc_client.active[0] = False
    assert _callback(oidc_client.client, state).status_code == 400
    assert oidc_client.linked == []


def test_link_rejects_wrong_state_or_provider_without_provider_exchange(oidc_client) -> None:
    state = _begin(oidc_client.client)
    assert _callback(oidc_client.client, "wrong-state").status_code == 400
    assert _callback(oidc_client.client, state, provider="different").status_code == 400
    assert oidc_client.exchanged == []
    assert oidc_client.linked == []
    assert _callback(oidc_client.client, state).status_code == 302


def test_superseded_link_cannot_complete_as_login_or_link(oidc_client) -> None:
    old_state = _begin(oidc_client.client)
    new_state = _begin(oidc_client.client, mode="login")
    assert old_state != new_state
    assert _callback(oidc_client.client, old_state).status_code == 400
    assert oidc_client.exchanged == []
    assert oidc_client.linked == []


def test_confirmed_link_conflict_returns_sanitized_denial_and_commits(oidc_client) -> None:
    state = _begin(oidc_client.client)
    oidc_client.error[0] = IdentityLinkConflictError("must not expose details")
    response = _callback(oidc_client.client, state)
    assert response.status_code == 400
    assert response.json() == {"detail": "external identity cannot be linked"}
    assert oidc_client.linked == []
    assert oidc_client.transactions == ["committed", "committed"]


def test_other_identity_conflicts_still_rollback(oidc_client) -> None:
    state = _begin(oidc_client.client)
    oidc_client.error[0] = IdentityConflictError("user not found")
    response = _callback(oidc_client.client, state)
    assert response.status_code == 400
    assert oidc_client.linked == []
    assert oidc_client.transactions == ["committed", "rolled_back"]


@pytest.mark.parametrize(
    "headers",
    [
        {},
        {"Sec-Fetch-Site": "cross-site", "Sec-Fetch-Mode": "navigate", "Sec-Fetch-User": "?1"},
        {"Sec-Fetch-Site": "same-origin", "Sec-Fetch-Mode": "navigate"},
    ],
)
def test_link_requires_explicit_same_origin_browser_intent(oidc_client, headers) -> None:
    response = oidc_client.client.get(
        "/api/v1/auth/login/keycloak",
        params={"mode": "link"},
        headers=headers,
        follow_redirects=False,
    )
    assert response.status_code == 403
    assert oidc_client.linked == []
    assert oidc_client.exchanged == []
