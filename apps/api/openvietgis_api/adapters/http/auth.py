from __future__ import annotations

from secrets import compare_digest, token_urlsafe
from typing import Annotated, Any, Literal
from urllib.parse import urlsplit

from authlib.integrations.base_client import OAuthError  # type: ignore[import-untyped]
from fastapi import APIRouter, Depends, HTTPException, Query, Request, Response, status
from fastapi.responses import JSONResponse, RedirectResponse
from sqlalchemy.orm import Session
from starlette.concurrency import run_in_threadpool

from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.oidc.provider import (
    OidcInvalidResponseError,
    OidcProviderNotFoundError,
    OidcProviderRegistry,
)
from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.identity_repository import SqlIdentityRepository
from openvietgis_api.config import Settings, get_settings
from openvietgis_api.domains.identity.service import (
    ExternalIdentityClaims,
    IdentityConflictError,
    IdentityLinkConflictError,
    IdentityService,
)
from openvietgis_api.security.csrf import make_csrf_token

router = APIRouter(tags=["identity"])


def _safe_return_to(value: str | None) -> str:
    if not value or not value.startswith("/") or value.startswith("//") or "://" in value:
        return "/"
    return value


def _registry(settings: Settings) -> OidcProviderRegistry:
    return OidcProviderRegistry(settings)


def _identity_service(session: Session) -> IdentityService:
    return IdentityService(
        SqlIdentityRepository(session),
        SqlAuditRepository(session),
    )


@router.get("/auth/providers")
def list_identity_providers(
    settings: Annotated[Settings, Depends(get_settings)],
) -> list[dict[str, str]]:
    return [
        {"id": item.id, "display_name": item.display_name}
        for item in _registry(settings).summaries()
    ]


@router.get("/auth/login/{provider}")
async def begin_login(
    provider: str,
    request: Request,
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    settings: Annotated[Settings, Depends(get_settings)],
    mode: Literal["login", "link"] = Query(default="login"),
    return_to: str | None = Query(default=None),
    prompt: Literal["login"] | None = Query(default=None),
    action: Literal["password"] | None = Query(default=None),
) -> Response:
    state = token_urlsafe(32)
    transaction = {
        "state": state,
        "provider": provider,
        "mode": mode,
        "return_to": _safe_return_to(return_to),
    }
    if mode == "link":
        # A top-level cross-site navigation can carry SameSite=Lax cookies.
        # Only a same-origin, user-activated browser navigation is valid link intent.
        if (
            request.headers.get("sec-fetch-site") != "same-origin"
            or request.headers.get("sec-fetch-mode") != "navigate"
            or request.headers.get("sec-fetch-user") != "?1"
        ):
            raise HTTPException(status_code=403, detail="explicit identity link intent required")
        authentication = await run_in_threadpool(authenticate_request, request, session, settings)
        if authentication.actor.actor_id is None or authentication.session_id is None:
            raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED)
        transaction["user_id"] = str(authentication.actor.actor_id)
        transaction["session_id"] = str(authentication.session_id)
    if action == "password":
        if provider != "keycloak" or mode != "login":
            raise HTTPException(status_code=400, detail="invalid account action")
        authentication = await run_in_threadpool(authenticate_request, request, session, settings)
        if authentication.actor.actor_id is None or authentication.session_id is None:
            raise HTTPException(status_code=401)
        transaction.update(action="password", user_id=str(authentication.actor.actor_id), session_id=str(authentication.session_id), return_to="/app")
    redirect_uri = (
        settings.api_public_base_url.rstrip("/")
        + f"/api/v1/auth/callback/{provider}"
    )
    try:
        redirect = await _registry(settings).begin_login(
            request,
            provider_name=provider,
            redirect_uri=redirect_uri,
            state=state,
            **({"prompt": "login"} if prompt == "login" and mode == "login" else {}),
            **({"action": "UPDATE_PASSWORD"} if action == "password" else {}),
        )
    except OidcProviderNotFoundError as exc:
        raise HTTPException(status_code=400, detail="unknown identity provider") from exc
    # Only the most recently initiated flow can complete. Bind intent to provider and
    # the exact OAuth state, never infer login/link intent from unrelated cookie data.
    request.session["ovg_oidc_transaction"] = transaction
    return redirect


@router.get("/auth/callback/{provider}", name="complete_login")
async def complete_login(
    provider: str,
    request: Request,
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    settings: Annotated[Settings, Depends(get_settings)],
) -> Response:
    transaction = request.session.get("ovg_oidc_transaction")
    states = request.query_params.getlist("state")
    if (
        not isinstance(transaction, dict)
        or len(states) != 1
        or not isinstance(transaction.get("state"), str)
        or not compare_digest(transaction["state"].encode(), states[0].encode())
        or transaction.get("provider") != provider
        or transaction.get("mode") not in {"login", "link"}
    ):
        raise HTTPException(status_code=400, detail="invalid OIDC transaction")
    request.session.pop("ovg_oidc_transaction")
    if request.query_params.get("error") == "access_denied" and transaction.get("mode") == "login":
        path = "/app?auth_complete=1&password_result=cancelled" if transaction.get("action") == "password" else "/?auth_cancelled=1"
        return RedirectResponse(settings.web_base_url.rstrip("/") + path, status_code=302)
    try:
        claims = await _registry(settings).complete_login(
            request,
            provider_name=provider,
        )
    except OidcProviderNotFoundError as exc:
        raise HTTPException(status_code=400, detail="unknown identity provider") from exc
    except (OAuthError, OidcInvalidResponseError) as exc:
        raise HTTPException(status_code=400, detail="invalid OIDC response") from exc

    # The synchronous SQLAlchemy transaction can wait on another callback's
    # locks. Keep it off the event loop so that the other request can commit.
    return await run_in_threadpool(
        _complete_verified_login, request, session, settings, transaction, claims
    )


def _complete_verified_login(
    request: Request,
    session: Session,
    settings: Settings,
    transaction: dict[str, Any],
    claims: ExternalIdentityClaims,
) -> Response:
    service = _identity_service(session)
    return_to = _safe_return_to(transaction.get("return_to"))
    redirect = RedirectResponse(
        url=f"{settings.web_base_url.rstrip('/')}{return_to}",
        status_code=status.HTTP_302_FOUND,
    )

    try:
        if transaction.get("action") == "password":
            authentication = authenticate_request(request, session, settings)
            identity = SqlIdentityRepository(session).find_by_external_identity(issuer=claims.issuer, subject=claims.subject)
            same = (identity is not None and str(identity[0].id) == transaction.get("user_id") and str(authentication.actor.actor_id) == transaction.get("user_id") and str(authentication.session_id) == transaction.get("session_id"))
            action_status = request.query_params.get("kc_action_status")
            outcome = "success" if same and action_status == "success" else "cancelled" if same and action_status == "cancelled" else "unconfirmed"
            return RedirectResponse(settings.web_base_url.rstrip("/") + "/app?auth_complete=1&password_result=" + outcome, status_code=302)
        if transaction["mode"] == "link":
            # Reauthenticate after provider verification. Logout, expiry, unlinking,
            # a different session for the same user, or an account switch invalidate
            # this transaction and also prevent unauthenticated audit insertion.
            try:
                authentication = authenticate_request(request, session, settings)
            except HTTPException as exc:
                raise HTTPException(status_code=400, detail="link session no longer valid") from exc
            actor_id = authentication.actor.actor_id
            if (
                actor_id is None
                or authentication.session_id is None
                or str(actor_id) != transaction.get("user_id")
                or str(authentication.session_id) != transaction.get("session_id")
            ):
                raise HTTPException(status_code=400, detail="link session changed")
            if not SqlIdentityRepository(session).lock_active_web_session(
                user_id=actor_id, session_id=authentication.session_id,
            ):
                raise HTTPException(status_code=400, detail="link session no longer valid")
            try:
                service.link_identity(user_id=actor_id, claims=claims)
            except IdentityLinkConflictError:
                # The confirmed-conflict branch made no identity mutation and
                # appended its sanitized denial audit. Return normally so the
                # function-scoped transaction commits only that audit; other
                # exceptions still trigger the standard rollback.
                return JSONResponse(
                    status_code=400,
                    content={"detail": "external identity cannot be linked"},
                )
            return redirect
        result = service.login(
            claims,
            session_ttl_seconds=settings.session_ttl_seconds,
        )
    except IdentityConflictError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    redirect.set_cookie(
        settings.session_cookie_name,
        result.session_token,
        max_age=settings.session_ttl_seconds,
        httponly=True,
        secure=settings.session_cookie_secure,
        samesite="lax",
        path="/",
    )
    return redirect


@router.get("/auth/csrf")
def get_csrf(
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    settings: Annotated[Settings, Depends(get_settings)],
) -> dict[str, str]:
    if authentication.raw_session_token is None:
        raise HTTPException(status_code=500, detail="CSRF session unavailable")
    return {
        "token": make_csrf_token(
            authentication.raw_session_token,
            settings.csrf_secret.get_secret_value(),
        )
    }


@router.post("/auth/logout", status_code=204)
def logout(
    request: Request,
    response: Response,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    settings: Annotated[Settings, Depends(get_settings)],
) -> None:
    if authentication.session_id is not None:
        SqlIdentityRepository(session).revoke_web_session(authentication.session_id)
    request.session.clear()
    response.delete_cookie(settings.session_cookie_name, path="/", secure=settings.session_cookie_secure, httponly=True, samesite="lax")
    response.headers["Cache-Control"] = "no-store"

    # The production Keycloak realm is reverse-proxied on the web origin.
    # Discard that browser's realm cookies too; otherwise a new authorization
    # request may resume the previous SSO account after app session revocation.
    # Do not touch any other origin, realm, or Google account cookies.
    provider = settings.oidc_providers.get("keycloak")
    if provider:
        issuer = urlsplit(provider.issuer)
        web = urlsplit(settings.web_base_url)
        api = urlsplit(settings.api_public_base_url)
        if (
            (issuer.scheme, issuer.netloc) == (web.scheme, web.netloc) == (api.scheme, api.netloc)
            and issuer.path.rstrip("/").startswith("/realms/")
        ):
            realm_path = issuer.path.rstrip("/")
            for cookie in (
                "KEYCLOAK_IDENTITY", "KEYCLOAK_SESSION", "KEYCLOAK_IDENTITY_LEGACY",
                "KEYCLOAK_SESSION_LEGACY", "KEYCLOAK_REMEMBER_ME", "AUTH_SESSION_ID",
                "AUTH_SESSION_ID_LEGACY", "KC_AUTH_SESSION_HASH", "KC_RESTART",
            ):
                for path in (realm_path + "/", realm_path):
                    response.delete_cookie(cookie, path=path, secure=issuer.scheme == "https", httponly=True, samesite="lax")
