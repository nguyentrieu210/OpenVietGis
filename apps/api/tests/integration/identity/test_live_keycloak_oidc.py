"""Opt-in backend acceptance using the real loopback Keycloak authorization-code flow.

This is not browser/Playwright E2E. The provider login form and authorization code
exchange are real; only the application HTTP transport is in-process TestClient.
"""

from __future__ import annotations

import io
import json
import logging
import os
import re
from collections.abc import Iterator, Mapping
from contextlib import contextmanager
from html.parser import HTMLParser
from http.cookiejar import DefaultCookiePolicy
from http.cookies import SimpleCookie
from pathlib import Path
from types import SimpleNamespace
from urllib.parse import parse_qs, quote, quote_plus, urljoin, urlsplit
from urllib.request import Request

import httpx
import pytest
from authlib.integrations.starlette_client.apps import (  # type: ignore[import-untyped]
    StarletteOAuth2App,
)
from fastapi.testclient import TestClient
from sqlalchemy import text
from tools.test_database import assert_safe_test_target

from openvietgis_api.config import get_settings
from openvietgis_api.main import create_app
from openvietgis_api.security.sessions import parse_session_token, verify_session_token
from tests.fixtures.db_admin import get_admin_session_factory

live_oidc = pytest.mark.skipif(
    os.environ.get("OPENVIETGIS_LIVE_OIDC") != "1",
    reason="opt-in loopback Keycloak acceptance; set OPENVIETGIS_LIVE_OIDC=1",
)


def _loopback_origin(name: str, default: int) -> str:
    raw = os.environ.get(name, str(default))
    if re.fullmatch(r"\d{1,5}", raw) is None or not 1024 <= int(raw) <= 65535:
        raise RuntimeError(f"{name} must designate an unprivileged loopback port")
    return f"http://127.0.0.1:{int(raw)}"


PROVIDER_ORIGIN = _loopback_origin("OPENVIETGIS_E2E_OIDC_PORT", 18080)
APP_ORIGIN = _loopback_origin("OPENVIETGIS_E2E_API_PORT", 8000)
WEB_ORIGIN = _loopback_origin("OPENVIETGIS_E2E_WEB_PORT", 5173)
if len({PROVIDER_ORIGIN, APP_ORIGIN, WEB_ORIGIN}) != 3:
    raise RuntimeError("live OIDC origins must use distinct loopback ports")
ISSUER = PROVIDER_ORIGIN + "/realms/openvietgis"
CALLBACK_PATH = "/api/v1/auth/callback/keycloak"
CALLBACK_URL = APP_ORIGIN + CALLBACK_PATH
CLIENT_ID = "openvietgis-web"
FIXTURE_USERNAME = "owner-a"
REDIRECT_CODES = {301, 302, 303, 307, 308}


def _require(condition: bool, reason: str) -> None:
    # Avoid pytest assertion introspection leaking credential-bearing URLs,
    # response bodies, cookie values, codes or token exchange data.
    if not condition:
        pytest.fail(reason, pytrace=False)


class _SecretEvidence:
    """Store real synthetic credentials only in memory; report fixed failure labels."""

    def __init__(self) -> None:
        self._canaries: dict[str, set[str]] = {}
        self._surfaces: list[tuple[str, frozenset[str]]] = []
        self.exchanges = 0

    def record(self, kind: str, value: object) -> None:
        _require(isinstance(value, str) and len(value) >= 16, "credential capture failed")
        if isinstance(value, str):
            self._canaries.setdefault(kind, set()).update(
                (value, quote(value, safe=""), quote_plus(value, safe=""))
            )

    def surfaces(self, value: str, *, allowed: tuple[str, ...] = ()) -> None:
        self._surfaces.append((value, frozenset(allowed)))

    def response(self, response: httpx.Response, *, stage: str = "") -> None:
        # Only the intentional protocol channels may carry their own credential
        # kinds; check every other response header and the complete response body.
        self.surfaces(response.text, allowed=("csrf",) if stage == "csrf" else ())
        for name, value in response.headers.multi_items():
            allowed: tuple[str, ...] = ()
            if stage == "begin" and name == "location":
                allowed = ("state", "nonce")
            elif name == "set-cookie":
                if stage == "begin" and value.startswith("ovg_oidc_state="):
                    allowed = ("oidc-cookie",)
                elif stage == "callback" and value.startswith(
                    get_settings().session_cookie_name + "="
                ):
                    allowed = ("session-cookie", "session-secret")
            self.surfaces(value, allowed=allowed)

    def has_leak(self) -> bool:
        return any(
            canary in surface
            for surface, allowed in self._surfaces
            for kind, canaries in self._canaries.items()
            if kind not in allowed
            for canary in canaries
        )


@contextmanager
def _private_runtime_logs() -> Iterator[io.StringIO]:
    # Never forward a possible secret-bearing log to pytest or its CI report.
    root = logging.getLogger()
    previous_handlers, previous_level = root.handlers, root.level
    stream = io.StringIO()
    handler = logging.StreamHandler(stream)
    root.handlers = [handler]
    root.setLevel(logging.INFO)
    try:
        yield stream
    finally:
        root.handlers = previous_handlers
        root.setLevel(previous_level)
        handler.close()
        stream.close()


def _observe_real_exchange(
    monkeypatch: pytest.MonkeyPatch,
    evidence: _SecretEvidence,
) -> None:
    # This is a transparent observer, not a token endpoint mock. Authlib's real
    # HTTP token fetch runs unmodified, and its caller still performs ID-token
    # validation (including nonce). Record credentials before returning them.
    original = StarletteOAuth2App.fetch_access_token

    async def observed(client, *args, **kwargs):  # type: ignore[no-untyped-def]
        evidence.record("code", kwargs.get("code"))
        evidence.record("pkce-verifier", kwargs.get("code_verifier"))
        token = await original(client, *args, **kwargs)
        _require(isinstance(token, Mapping), "real exchange did not return a token mapping")
        for kind in ("access_token", "id_token", "refresh_token"):
            evidence.record(kind, token.get(kind))
        evidence.exchanges += 1
        return token

    monkeypatch.setattr(StarletteOAuth2App, "fetch_access_token", observed)


def test_live_oidc_secret_monitor_is_private_and_nonvacuous(
    caplog: pytest.LogCaptureFixture,
) -> None:
    evidence = _SecretEvidence()
    marker = "synthetic-fixture-marker-not-a-real-secret"
    evidence.record("code", marker)
    evidence.surfaces(marker, allowed=("code",))
    assert not evidence.has_leak()
    with _private_runtime_logs() as captured:
        logging.getLogger().warning("%s", marker)
        evidence.surfaces(captured.getvalue())
        assert evidence.has_leak()
    assert marker not in caplog.text


def _provider_url(value: str) -> bool:
    parts = urlsplit(value)
    return (
        parts.scheme == "http"
        and parts.netloc == urlsplit(PROVIDER_ORIGIN).netloc
        and parts.path.startswith("/realms/openvietgis/")
        and not parts.fragment
    )


class _LoopbackProviderCookiePolicy(DefaultCookiePolicy):
    def return_ok_secure(self, cookie, request):  # type: ignore[no-untyped-def]
        # Keycloak issues Secure transaction cookies even on trusted loopback
        # HTTP. Python CookieJar rejects them by default, unlike local browsers.
        # Emulate that browser exception only for the pinned synthetic realm.
        if cookie.secure and _provider_url(request.get_full_url()):
            return True
        return super().return_ok_secure(cookie, request)


def _check_loopback_cookie_policy() -> None:
    policy = _LoopbackProviderCookiePolicy()
    secure_cookie = SimpleNamespace(secure=True)
    # Test the precise exception without constructing or sending real cookies.
    _require(
        policy.return_ok_secure(secure_cookie, Request(ISSUER + "/login-actions/authenticate"))
        and not policy.return_ok_secure(secure_cookie, Request(APP_ORIGIN + "/realms/openvietgis/"))
        and not policy.return_ok_secure(secure_cookie, Request(PROVIDER_ORIGIN + "/realms/other/"))
        and not policy.return_ok_secure(
            secure_cookie, Request("http://example.test/realms/openvietgis/")
        )
        and not policy.return_ok_secure(secure_cookie, Request(APP_ORIGIN + CALLBACK_PATH)),
        "loopback provider Secure cookie policy was not narrowly confined",
    )


def _callback_url(value: str) -> bool:
    parts = urlsplit(value)
    return (
        parts.scheme == "http"
        and parts.netloc == urlsplit(APP_ORIGIN).netloc
        and parts.path == CALLBACK_PATH
        and not parts.fragment
    )


def _redirect(response: httpx.Response) -> str:
    _require(response.status_code in REDIRECT_CODES, "expected local provider redirect")
    location = response.headers.get("location")
    _require(bool(location), "provider redirect had no location")
    return urljoin(str(response.url), location or "")


def _fixture_password() -> str:
    fixture = Path(__file__).resolve().parents[5] / "infra/dev/keycloak/realm-openvietgis.json"
    data = json.loads(fixture.read_text(encoding="utf-8"))
    user = next(item for item in data["users"] if item["username"] == FIXTURE_USERNAME)
    password = next(item for item in user["credentials"] if item["type"] == "password")
    _require(
        data["realm"] == "openvietgis"
        and user["enabled"] is True
        and user["email"] == "owner-a@example.test"
        and password["temporary"] is False
        and isinstance(password["value"], str)
        and password["value"].startswith("openvietgis-test-"),
        "expected checked-in synthetic fixture identity",
    )
    return password["value"]


def _provider_request(
    client: httpx.Client,
    method: str,
    url: str,
    *,
    data: dict[str, str] | None = None,
    headers: dict[str, str] | None = None,
) -> httpx.Response:
    _require(_provider_url(url), "refusing unexpected provider request")
    # Client.build_request copies cookies into a new default CookieJar, losing
    # the custom policy; make the request directly and apply only the pinned jar.
    request = httpx.Request(method, url, data=data, headers=headers)
    client.cookies.set_cookie_header(request)
    return client.send(request, follow_redirects=False)


def _check_discovery(client: httpx.Client) -> None:
    response = _provider_request(client, "GET", ISSUER + "/.well-known/openid-configuration")
    _require(response.status_code == 200, "Keycloak loopback discovery is not healthy")
    metadata = response.json()
    protocol = ISSUER + "/protocol/openid-connect/"
    _require(
        metadata.get("issuer") == ISSUER
        and metadata.get("authorization_endpoint") == protocol + "auth"
        and metadata.get("token_endpoint") == protocol + "token"
        and metadata.get("jwks_uri") == protocol + "certs",
        "unexpected issuer or non-local provider endpoint",
    )


class _LoginForm(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.found = False
        self.inside = False
        self.action: str | None = None
        self.method: str | None = None
        self.names: set[str] = set()
        self.hidden: dict[str, str] = {}

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attributes = dict(attrs)
        if tag == "form":
            if attributes.get("id") == "kc-form-login":
                if self.found:
                    raise ValueError("duplicate login form")
                self.found = True
                self.inside = True
                self.action = attributes.get("action")
                self.method = attributes.get("method")
            elif self.inside:
                raise ValueError("nested login form")
        elif tag == "input" and self.inside:
            name = attributes.get("name")
            if name:
                self.names.add(name)
                if attributes.get("type") == "hidden":
                    self.hidden[name] = attributes.get("value") or ""

    def handle_endtag(self, tag: str) -> None:
        if tag == "form" and self.inside:
            self.inside = False


def _login_page(client: httpx.Client, url: str) -> httpx.Response:
    for _ in range(8):
        _require(_provider_url(url), "refusing unexpected provider navigation")
        response = _provider_request(client, "GET", url)
        if response.status_code == 200:
            return response
        url = _redirect(response)
    pytest.fail("provider authorization redirect limit exceeded", pytrace=False)


def _provider_callback(client: httpx.Client, authorization_url: str) -> str:
    page = _login_page(client, authorization_url)
    form = _LoginForm()
    form.feed(page.text)
    form.close()
    _require(
        form.found
        and form.action is not None
        and (form.method or "").lower() == "post"
        and {"username", "password"} <= form.names,
        "Keycloak synthetic login form was not found",
    )
    action = urljoin(str(page.url), form.action or "")
    _require(
        _provider_url(action)
        and urlsplit(action).path == "/realms/openvietgis/login-actions/authenticate",
        "refusing to send synthetic credentials to an unexpected action",
    )
    # The password is sent only to Keycloak's standard browser login action.
    # No direct password grant and no token endpoint mocking.
    response = _provider_request(
        client,
        "POST",
        action,
        data={
            **form.hidden,
            "username": FIXTURE_USERNAME,
            "password": _fixture_password(),
        },
        headers={"Origin": PROVIDER_ORIGIN, "Referer": str(page.url)},
    )
    for _ in range(8):
        _require(
            response.status_code in {301, 302, 303},
            "synthetic login did not yield a safe redirect",
        )
        location = _redirect(response)
        if _callback_url(location):
            return location
        _require(_provider_url(location), "refusing unexpected post-login navigation")
        response = _provider_request(client, "GET", location)
    pytest.fail("post-login provider redirect limit exceeded", pytrace=False)


def _login_once(
    app: TestClient,
    provider: httpx.Client,
    evidence: _SecretEvidence,
) -> tuple[str, str]:
    begin = app.get(
        "/api/v1/auth/login/keycloak",
        params={"return_to": "/organizations"},
        follow_redirects=False,
    )
    evidence.response(begin, stage="begin")
    _require(begin.status_code == 302, "application did not start authorization")
    authorization_url = begin.headers.get("location", "")
    _require(_provider_url(authorization_url), "unexpected authorization URL")
    authorization = parse_qs(urlsplit(authorization_url).query)
    _require(
        urlsplit(authorization_url).path == "/realms/openvietgis/protocol/openid-connect/auth"
        and authorization.get("response_type") == ["code"]
        and authorization.get("client_id") == [CLIENT_ID]
        and authorization.get("redirect_uri") == [CALLBACK_URL]
        and authorization.get("code_challenge_method") == ["S256"]
        and len(authorization.get("state", [])) == 1
        and len(authorization.get("nonce", [])) == 1
        and len(authorization.get("code_challenge", [])) == 1
        and bool(re.fullmatch(r"[A-Za-z0-9_-]{43}", authorization.get("code_challenge", [""])[0])),
        "authorization code, state, nonce or PKCE contract is invalid",
    )
    evidence.record("state", authorization["state"][0])
    evidence.record("nonce", authorization["nonce"][0])
    evidence.record("oidc-cookie", app.cookies.get("ovg_oidc_state"))
    callback_url = _provider_callback(provider, authorization_url)
    callback = urlsplit(callback_url)
    query = parse_qs(callback.query)
    _require(
        query.get("state") == authorization.get("state")
        and len(query.get("code", [])) == 1
        and not {"error", "access_token", "id_token", "refresh_token"} & query.keys(),
        "provider callback did not carry the expected authorization code and state",
    )

    evidence.record("code", query["code"][0])

    # Do not connect to a real app server: route Keycloak's actual callback via
    # TestClient, preserving the app's original signed OIDC transaction cookie.
    result = app.get(callback.path + "?" + callback.query, follow_redirects=False)
    evidence.response(result, stage="callback")
    _require(result.status_code == 302, "real authorization code exchange failed")
    _require(
        result.headers.get("location") == WEB_ORIGIN + "/organizations" and not result.content,
        "callback leaked data or did not redirect to the expected app path",
    )
    cookies = SimpleCookie()
    for header in result.headers.get_list("set-cookie"):
        cookies.load(header)
    cookie_name = get_settings().session_cookie_name
    _require(
        set(cookies) <= {cookie_name, "ovg_oidc_state"} and cookie_name in cookies,
        "callback returned unexpected cookies",
    )
    _require(
        bool(cookies[cookie_name]["httponly"])
        and cookies[cookie_name]["samesite"].lower() == "lax"
        and cookies[cookie_name]["path"] == "/",
        "application session cookie flags are invalid",
    )
    token = app.cookies.get(cookie_name, "")
    _require(
        bool(re.fullmatch(r"ovgs_[0-9a-f]{32}\.s_[A-Za-z0-9_-]{43}", token))
        and set(app.cookies.keys()) == {cookie_name},
        "expected only an opaque application session cookie after OIDC callback",
    )
    evidence.record("session-cookie", token)
    evidence.record("session-secret", parse_session_token(token).secret)
    me = app.get("/api/v1/me")
    evidence.response(me)
    _require(me.status_code == 200, "first-party session could not resolve /me")
    user = me.json()
    _require(
        set(user) == {"id", "display_name", "email_snapshot", "status"}
        and user.get("email_snapshot") == "owner-a@example.test"
        and user.get("status") == "active"
        and isinstance(user.get("id"), str),
        "current user does not match the synthetic Keycloak identity",
    )
    # The browser-facing token is our own opaque session, not a provider JWT or
    # refresh token. Prove its stored representation is only a digest.
    parsed = parse_session_token(token)
    with get_admin_session_factory()() as admin:
        row = (
            admin.execute(
                text(
                    """
                SELECT s.secret_hash, s.user_id, s.revoked_at, l.issuer,
                       l.email_snapshot, l.email_verified
                FROM identity.web_sessions s
                JOIN identity.identity_links l
                  ON l.id = s.identity_link_id AND l.user_id = s.user_id
                WHERE s.id = :id
                """
                ),
                {"id": parsed.session_id},
            )
            .mappings()
            .one_or_none()
        )
        _require(
            row is not None,
            "application session was not persisted",
        )
        if row is not None:
            verify_session_token(token, bytes(row["secret_hash"]))
            _require(
                len(bytes(row["secret_hash"])) == 32
                and str(row["user_id"]) == user["id"]
                and row["revoked_at"] is None
                and row["issuer"] == ISSUER
                and row["email_snapshot"] == "owner-a@example.test"
                and row["email_verified"] is True,
                "persisted session or stable OIDC identity is invalid",
            )

    return user["id"], token


def _logout(app: TestClient, token: str, evidence: _SecretEvidence) -> None:
    csrf = app.get("/api/v1/auth/csrf")
    evidence.response(csrf, stage="csrf")
    _require(csrf.status_code == 200, "session did not provide CSRF token")
    csrf_body = csrf.json()
    _require(isinstance(csrf_body, dict) and set(csrf_body) == {"token"}, "unexpected CSRF body")
    csrf_token = csrf_body.get("token")
    _require(isinstance(csrf_token, str) and bool(csrf_token), "missing CSRF token")
    evidence.record("csrf", csrf_token)
    response = app.post(
        "/api/v1/auth/logout",
        headers={"X-OpenVietGIS-CSRF": csrf_token},
    )
    evidence.response(response)
    cookie_name = get_settings().session_cookie_name
    _require(
        response.status_code == 204 and not app.cookies.get(cookie_name),
        "logout did not clear the first-party session",
    )
    logged_out = app.get("/api/v1/me")
    evidence.response(logged_out)
    _require(logged_out.status_code == 401, "logout left /me authenticated")
    replay = app.get("/api/v1/me", headers={"Cookie": f"{cookie_name}={token}"})
    evidence.response(replay)
    _require(replay.status_code == 401, "revoked session cookie was still usable")
    with get_admin_session_factory()() as admin:
        revoked = admin.execute(
            text("SELECT revoked_at FROM identity.web_sessions WHERE id = :id"),
            {"id": parse_session_token(token).session_id},
        ).scalar_one_or_none()
        _require(revoked is not None, "logout revocation was not durable")


@live_oidc
def test_live_keycloak_code_pkce_login_logout_and_stable_identity(
    migrated_database: None,
    monkeypatch: pytest.MonkeyPatch,
    caplog: pytest.LogCaptureFixture,
    capfd: pytest.CaptureFixture[str],
) -> None:
    evidence = _SecretEvidence()
    with _private_runtime_logs() as captured:
        try:
            _exercise_live_oidc(monkeypatch, caplog, evidence)
        finally:
            # Drain on failures too: no diagnostic surface can enter test reports.
            output = capfd.readouterr()
            for surface in (captured.getvalue(), output.out, output.err):
                evidence.surfaces(surface)
            _require(not evidence.has_leak(), "live OIDC credential leaked to a captured surface")


def _exercise_live_oidc(
    monkeypatch: pytest.MonkeyPatch,
    caplog: pytest.LogCaptureFixture,
    evidence: _SecretEvidence,
) -> None:
    stage = "local-only configuration"
    try:
        _check_loopback_cookie_policy()
        settings = get_settings()
        assert_safe_test_target(settings.migration_db_url)
        assert_safe_test_target(settings.runtime_db_url)
        provider = settings.oidc_providers.get("keycloak")
        _require(
            settings.environment == "test"
            and settings.api_public_base_url == APP_ORIGIN
            and settings.web_base_url == WEB_ORIGIN
            and not settings.session_cookie_secure
            and provider is not None
            and provider.issuer == ISSUER
            and provider.client_id == CLIENT_ID
            and provider.client_secret is None,
            "live OIDC requires the dedicated test DB and checked-in loopback Keycloak profile",
        )
        # Prevent environment proxy settings from redirecting the loopback IdP calls.
        monkeypatch.setenv("NO_PROXY", "127.0.0.1,localhost")
        monkeypatch.setenv("no_proxy", "127.0.0.1,localhost")
        # Preserve the previous test's suppression of credential-bearing third
        # party transport/SQL diagnostics. Capture normal application runtime
        # output here, not provider server logs or browser/Playwright artifacts.
        for logger in ("httpx", "httpcore", "authlib", "sqlalchemy.engine"):
            caplog.set_level(logging.CRITICAL, logger=logger)
        evidence.record("fixture-password", _fixture_password())
        _observe_real_exchange(monkeypatch, evidence)
        with TestClient(
            create_app(),
            base_url=APP_ORIGIN,
            raise_server_exceptions=False,
        ) as app:
            stage = "first real Keycloak login"
            with httpx.Client(follow_redirects=False, trust_env=False, timeout=20) as provider_http:
                provider_http.cookies.jar.set_policy(_LoopbackProviderCookiePolicy())
                _check_discovery(provider_http)
                first_id, first_token = _login_once(app, provider_http, evidence)
            stage = "first application logout"
            _logout(app, first_token, evidence)

            stage = "second real Keycloak login"
            # Fresh provider cookies force another real login form and code exchange.
            with httpx.Client(follow_redirects=False, trust_env=False, timeout=20) as provider_http:
                provider_http.cookies.jar.set_policy(_LoopbackProviderCookiePolicy())
                second_id, second_token = _login_once(app, provider_http, evidence)
            _require(
                first_id == second_id
                and parse_session_token(first_token).session_id
                != parse_session_token(second_token).session_id,
                "repeat authentication did not preserve the stable internal user",
            )
            stage = "second application logout"
            _logout(app, second_token, evidence)
        stage = "credential non-disclosure evidence"
        _require(evidence.exchanges == 2, "expected two observed real token exchanges")
        # Login/logout do not require audit rows. Scan complete persisted audit
        # records in this guarded synthetic DB, regardless of actor or event type.
        with get_admin_session_factory()() as admin:
            audit_rows = admin.execute(text("SELECT * FROM audit.audit_events")).mappings().all()
            evidence.surfaces(json.dumps([dict(row) for row in audit_rows], default=str))
    except Exception:
        # Provider URLs/HTML, authorization codes, raw tokens and exception details
        # must never escape into test output. Stage names are fixed and safe.
        pytest.fail(f"Live OIDC failed during {stage}", pytrace=False)
