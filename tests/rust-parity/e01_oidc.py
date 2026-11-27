"""E01 OIDC differential parity: login, callback, PKCE, cancel, account password action, and link conflict."""

from __future__ import annotations

import argparse
import base64
import contextlib
import hashlib
import http.server
import json
import os
import secrets
import sys
import tempfile
import threading
import time
from pathlib import Path
from urllib.parse import parse_qs, urlencode, urlsplit
from uuid import uuid4

import psycopg
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import padding, rsa

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "rust-parity"))
sys.path.insert(0, str(ROOT / "tools" / "benchmark" / "rust-platform"))

import benchmark_runner as baseline
import run as parity

API = "/api/v1"


def require(cond: bool, msg: str) -> None:
    if not cond:
        raise AssertionError(msg)


def _b64u(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode("ascii")


def _int_b64u(value: int) -> str:
    length = max(1, (value.bit_length() + 7) // 8)
    return _b64u(value.to_bytes(length, "big"))


class MockOidcIdp:
    def __init__(self) -> None:
        self._key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        pub_numbers = self._key.public_key().public_numbers()
        self.kid = "mock-rsa-1"
        self.jwks = {
            "keys": [
                {
                    "kty": "RSA",
                    "use": "sig",
                    "alg": "RS256",
                    "kid": self.kid,
                    "n": _int_b64u(pub_numbers.n),
                    "e": _int_b64u(pub_numbers.e),
                }
            ]
        }
        self.codes: dict[str, dict[str, object]] = {}
        self.server: http.server.ThreadingHTTPServer | None = None
        self.thread: threading.Thread | None = None
        self.issuer = ""

    def sign_jwt(self, payload: dict[str, object]) -> str:
        header = {"alg": "RS256", "typ": "JWT", "kid": self.kid}
        signing_input = (
            _b64u(json.dumps(header, separators=(",", ":")).encode("utf-8"))
            + "."
            + _b64u(json.dumps(payload, separators=(",", ":")).encode("utf-8"))
        ).encode("ascii")
        sig = self._key.sign(signing_input, padding.PKCS1v15(), hashes.SHA256())
        return signing_input.decode("ascii") + "." + _b64u(sig)

    def issue_code(
        self,
        *,
        client_id: str,
        redirect_uri: str,
        nonce: str,
        code_challenge: str,
        subject: str,
        email: str,
        name: str,
        issuer_override: str | None = None,
    ) -> str:
        code = "code-" + secrets.token_urlsafe(16)
        self.codes[code] = {
            "client_id": client_id,
            "redirect_uri": redirect_uri,
            "nonce": nonce,
            "code_challenge": code_challenge,
            "subject": subject,
            "email": email,
            "name": name,
            "issuer_override": issuer_override,
        }
        return code

    @contextlib.contextmanager
    def running(self):
        idp = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, _fmt: str, *args: object) -> None:
                return

            def _send_json(self, status: int, body: dict[str, object]) -> None:
                raw = json.dumps(body).encode("utf-8")
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)

            def do_GET(self) -> None:
                path = urlsplit(self.path).path
                if path == "/.well-known/openid-configuration":
                    self._send_json(
                        200,
                        {
                            "issuer": idp.issuer,
                            "authorization_endpoint": f"{idp.issuer}/authorize",
                            "token_endpoint": f"{idp.issuer}/token",
                            "jwks_uri": f"{idp.issuer}/jwks.json",
                            "id_token_signing_alg_values_supported": ["RS256"],
                        },
                    )
                    return
                if path == "/jwks.json":
                    self._send_json(200, idp.jwks)
                    return
                self._send_json(404, {"error": "not_found"})

            def do_POST(self) -> None:
                path = urlsplit(self.path).path
                if path != "/token":
                    self._send_json(404, {"error": "not_found"})
                    return
                length = int(self.headers.get("Content-Length", "0"))
                raw_body = self.rfile.read(length).decode("utf-8")
                form = {k: v[0] for k, v in parse_qs(raw_body, keep_blank_values=True).items()}
                code = form.get("code", "")
                entry = idp.codes.pop(code, None)
                if entry is None:
                    self._send_json(400, {"error": "invalid_grant"})
                    return
                verifier = form.get("code_verifier", "")
                computed = _b64u(hashlib.sha256(verifier.encode("ascii")).digest())
                if computed != entry["code_challenge"] or form.get("redirect_uri") != entry["redirect_uri"]:
                    self._send_json(400, {"error": "invalid_grant"})
                    return
                now = int(time.time())
                id_token = idp.sign_jwt(
                    {
                        "iss": entry["issuer_override"] or idp.issuer,
                        "sub": entry["subject"],
                        "aud": entry["client_id"],
                        "nonce": entry["nonce"],
                        "iat": now,
                        "exp": now + 300,
                        "email": entry["email"],
                        "email_verified": True,
                        "name": entry["name"],
                    }
                )
                self._send_json(
                    200,
                    {
                        "access_token": "at-" + secrets.token_urlsafe(12),
                        "token_type": "Bearer",
                        "expires_in": 300,
                        "id_token": id_token,
                    },
                )

        srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        port = srv.server_address[1]
        self.issuer = f"http://127.0.0.1:{port}"
        thread = threading.Thread(target=srv.serve_forever, daemon=True)
        thread.start()
        try:
            yield self
        finally:
            srv.shutdown()
            srv.server_close()


def _begin_and_issue(
    client,
    idp: MockOidcIdp,
    path: str,
    *,
    headers: dict[str, str] | None = None,
    subject: str,
    email: str,
    name: str,
    issuer_override: str | None = None,
) -> tuple[str, str, dict[str, str]]:
    resp = client.get(path, headers=headers or {}, follow_redirects=False)
    require(resp.status_code == 302, f"expected 302 from {path}, got {resp.status_code}: {resp.text}")
    loc = resp.headers["location"]
    parts = urlsplit(loc)
    require(f"{parts.scheme}://{parts.netloc}{parts.path}" == f"{idp.issuer}/authorize", f"bad authorize URL: {loc}")
    qp = {k: v[0] for k, v in parse_qs(parts.query).items()}
    require(qp.get("code_challenge_method") == "S256", f"missing S256 in {loc}")
    require(bool(qp.get("state")) and bool(qp.get("nonce")), f"missing state/nonce in {loc}")
    code = idp.issue_code(
        client_id=qp["client_id"],
        redirect_uri=qp["redirect_uri"],
        nonce=qp["nonce"],
        code_challenge=qp["code_challenge"],
        subject=subject,
        email=email,
        name=name,
        issuer_override=issuer_override,
    )
    return qp["state"], code, qp


def run_suite(
    name: str,
    client,
    fresh_client_factory,
    idp: MockOidcIdp,
    owner_url: str,
    seeded_user_session: dict[str, str],
) -> None:
    # 1. Unknown provider -> 400
    unknown_resp = client.get(f"{API}/auth/login/does-not-exist", follow_redirects=False)
    require(unknown_resp.status_code == 400, f"[{name}] unknown provider expected 400, got {unknown_resp.status_code}")

    # 2. Invalid query literal -> 422
    bad_mode = client.get(f"{API}/auth/login/keycloak?mode=invalid", follow_redirects=False)
    require(bad_mode.status_code == 422, f"[{name}] bad mode expected 422, got {bad_mode.status_code}")

    # 3. Link without Sec-Fetch headers -> 403
    no_sec = client.get(f"{API}/auth/login/keycloak?mode=link", follow_redirects=False)
    require(no_sec.status_code == 403, f"[{name}] link missing sec-fetch expected 403, got {no_sec.status_code}")

    # 4. Link unauthenticated with Sec-Fetch -> 401
    link_headers = {
        "sec-fetch-site": "same-origin",
        "sec-fetch-mode": "navigate",
        "sec-fetch-user": "?1",
    }
    unauth_link = client.get(f"{API}/auth/login/keycloak?mode=link", headers=link_headers, follow_redirects=False)
    require(unauth_link.status_code == 401, f"[{name}] link unauth expected 401, got {unauth_link.status_code}")

    # 5. Password action non-keycloak -> 400
    bad_action = client.get(f"{API}/auth/login/other?action=password", follow_redirects=False)
    require(bad_action.status_code == 400, f"[{name}] non-keycloak password expected 400, got {bad_action.status_code}")

    # 6. Callback with invalid/missing transaction -> 400
    bad_cb = client.get(f"{API}/auth/callback/keycloak?state=missing&code=abc", follow_redirects=False)
    require(bad_cb.status_code == 400, f"[{name}] missing tx expected 400, got {bad_cb.status_code}")

    # 7. Access denied cancel redirect
    state_cancel, _code_cancel, qp_cancel = _begin_and_issue(
        client,
        idp,
        f"{API}/auth/login/keycloak?prompt=login&return_to=//evil.example",
        subject=f"{name}-cancel-sub",
        email=f"{name}-cancel@example.test",
        name=f"{name} Cancel",
    )
    require(qp_cancel.get("prompt") == "login", f"[{name}] missing prompt=login")
    cancel_cb = client.get(
        f"{API}/auth/callback/keycloak?" + urlencode({"state": state_cancel, "error": "access_denied"}),
        follow_redirects=False,
    )
    require(cancel_cb.status_code == 302, f"[{name}] cancel expected 302, got {cancel_cb.status_code}")
    require(
        cancel_cb.headers["location"] == "http://127.0.0.1:5173/?auth_cancelled=1",
        f"[{name}] unexpected cancel location: {cancel_cb.headers['location']}",
    )

    # 8. Issuer mismatch in ID token -> 400
    state_bad_iss, code_bad_iss, _ = _begin_and_issue(
        client,
        idp,
        f"{API}/auth/login/keycloak",
        subject=f"{name}-badiss-sub",
        email=f"{name}-badiss@example.test",
        name="Bad Iss",
        issuer_override="https://evil-issuer.example",
    )
    bad_iss_cb = client.get(
        f"{API}/auth/callback/keycloak?" + urlencode({"state": state_bad_iss, "code": code_bad_iss}),
        follow_redirects=False,
    )
    require(bad_iss_cb.status_code == 400, f"[{name}] bad issuer expected 400, got {bad_iss_cb.status_code}")

    # 9. Full OIDC login + unsafe return_to fallback? No, safe return_to=/app?from=oidc
    sub_primary = f"{name}-primary-{uuid4().hex[:8]}"
    state_login, code_login, _ = _begin_and_issue(
        client,
        idp,
        f"{API}/auth/login/keycloak?return_to=/app?from=oidc",
        subject=sub_primary,
        email=f"{sub_primary}@example.test",
        name=f"OIDC {name}",
    )
    login_cb = client.get(
        f"{API}/auth/callback/keycloak?" + urlencode({"state": state_login, "code": code_login}),
        follow_redirects=False,
    )
    require(login_cb.status_code == 302, f"[{name}] login callback expected 302, got {login_cb.status_code}: {login_cb.text}")
    require(
        login_cb.headers["location"] == "http://127.0.0.1:5173/app?from=oidc",
        f"[{name}] unexpected login redirect: {login_cb.headers['location']}",
    )
    session_cookie = client.cookies.get("ovg_session") or login_cb.cookies.get("ovg_session")
    require(bool(session_cookie), f"[{name}] missing ovg_session cookie after login")
    me = client.get(f"{API}/me")
    require(me.status_code == 200, f"[{name}] /me after OIDC login failed: {me.status_code} {me.text}")
    require(me.json()["display_name"] == f"OIDC {name}", f"[{name}] unexpected /me display_name: {me.json()}")

    # 10. Link new identity using 'other' provider
    sub_linked = f"{name}-linked-{uuid4().hex[:8]}"
    state_link, code_link, _ = _begin_and_issue(
        client,
        idp,
        f"{API}/auth/login/other?mode=link&return_to=/settings",
        headers=link_headers,
        subject=sub_linked,
        email=f"{sub_linked}@example.test",
        name=f"Linked {name}",
    )
    link_cb = client.get(
        f"{API}/auth/callback/other?" + urlencode({"state": state_link, "code": code_link}),
        follow_redirects=False,
    )
    require(link_cb.status_code == 302, f"[{name}] link callback expected 302, got {link_cb.status_code}: {link_cb.text}")
    require(link_cb.headers["location"] == "http://127.0.0.1:5173/settings", f"[{name}] bad link redirect")

    links_resp = client.get(f"{API}/me/identity-links")
    require(links_resp.status_code == 200, f"[{name}] /me/identity-links failed: {links_resp.status_code}")
    subjects = {item["subject"] for item in links_resp.json()}
    require(subjects == {sub_primary, sub_linked}, f"[{name}] unexpected linked subjects: {subjects}")

    # 11. Password action (keycloak, same user)
    state_pw, code_pw, qp_pw = _begin_and_issue(
        client,
        idp,
        f"{API}/auth/login/keycloak?action=password",
        subject=sub_primary,
        email=f"{sub_primary}@example.test",
        name=f"OIDC {name}",
    )
    require(qp_pw.get("kc_action") == "UPDATE_PASSWORD", f"[{name}] missing kc_action=UPDATE_PASSWORD")
    pw_cb = client.get(
        f"{API}/auth/callback/keycloak?"
        + urlencode({"state": state_pw, "code": code_pw, "kc_action_status": "success"}),
        follow_redirects=False,
    )
    require(pw_cb.status_code == 302, f"[{name}] password callback expected 302, got {pw_cb.status_code}")
    require(
        pw_cb.headers["location"] == "http://127.0.0.1:5173/app?auth_complete=1&password_result=success",
        f"[{name}] bad password redirect: {pw_cb.headers['location']}",
    )

    # 12. Link conflict: seeded different user tries to link sub_primary -> 400 + denied audit
    with fresh_client_factory() as other_client:
        other_client.cookies.set("ovg_session", seeded_user_session["token"])
        state_conf, code_conf, _ = _begin_and_issue(
            other_client,
            idp,
            f"{API}/auth/login/keycloak?mode=link",
            headers=link_headers,
            subject=sub_primary,
            email=f"{sub_primary}@example.test",
            name="Conflict Try",
        )
        conf_cb = other_client.get(
            f"{API}/auth/callback/keycloak?" + urlencode({"state": state_conf, "code": code_conf}),
            follow_redirects=False,
        )
    require(conf_cb.status_code == 400, f"[{name}] link conflict expected 400, got {conf_cb.status_code}: {conf_cb.text}")
    require(
        conf_cb.json().get("detail") == "external identity cannot be linked",
        f"[{name}] unexpected conflict detail: {conf_cb.json()}",
    )
    with psycopg.connect(owner_url) as conn:
        row = conn.execute(
            """
            SELECT outcome, metadata
            FROM audit.audit_events
            WHERE actor_id = %s AND action = 'identity.link' AND outcome = 'denied'
            ORDER BY created_at DESC LIMIT 1
            """,
            (seeded_user_session["user_id"],),
        ).fetchone()
    require(row is not None and row[1] == {"reason": "identity_conflict"}, f"[{name}] missing denied link audit: {row}")


def main() -> int:
    parser = argparse.ArgumentParser(description="E01 OIDC differential parity check.")
    parser.add_argument("--candidate", required=True)
    args = parser.parse_args()
    binary = Path(args.candidate).resolve()
    admin_url = os.environ.get("OPENVIETGIS_PARITY_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with MockOidcIdp().running() as idp:
        providers_json = json.dumps(
            {
                "keycloak": {
                    "issuer": idp.issuer,
                    "client_id": "ovg-web",
                    "client_secret": "ovg-secret-value",
                    "display_name": "Keycloak",
                },
                "other": {
                    "issuer": idp.issuer,
                    "client_id": "ovg-other",
                    "display_name": "Other IdP",
                },
            }
        )
        with tempfile.TemporaryDirectory(prefix="openvietgis-e01-oidc-") as tmpdir:
            work = Path(tmpdir)
            artifacts = work / "artifacts"
            artifacts.mkdir()
            with parity.database(admin_url) as (owner_url, runtime_url):
                parity.configure_python(owner_url, runtime_url, artifacts)
                os.environ["OPENVIETGIS_OIDC_PROVIDERS"] = providers_json
                os.environ["OPENVIETGIS_WEB_BASE_URL"] = "http://127.0.0.1:5173"
                os.environ["OPENVIETGIS_API_PUBLIC_BASE_URL"] = "http://127.0.0.1:8000"

                import httpx
                from fastapi.testclient import TestClient
                from openvietgis_api.adapters.persistence.db import get_engine
                from openvietgis_api.config import get_settings
                from openvietgis_api.main import app

                get_settings.cache_clear()
                seeded_o = parity.seed(owner_url)
                seeded_other = {
                    "user_id": seeded_o["user_b_id"],
                    "token": seeded_o["session_b_token"],
                }

                try:
                    with (
                        TestClient(app, raise_server_exceptions=False) as oracle,
                        parity.rust_client(binary, runtime_url, work) as rust,
                    ):
                        run_suite(
                            "oracle",
                            oracle,
                            lambda: TestClient(app, raise_server_exceptions=False),
                            idp,
                            owner_url,
                            seeded_other,
                        )
                        run_suite(
                            "rust",
                            rust,
                            lambda: httpx.Client(base_url=str(rust.base_url), trust_env=False, timeout=10),
                            idp,
                            owner_url,
                            seeded_other,
                        )
                finally:
                    get_engine().dispose()

    print("E01 OIDC parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
