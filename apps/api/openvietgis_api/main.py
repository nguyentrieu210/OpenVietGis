from fastapi import APIRouter, Depends, FastAPI
from fastapi.middleware.cors import CORSMiddleware
from starlette.datastructures import MutableHeaders
from starlette.middleware.sessions import SessionMiddleware
from starlette.types import ASGIApp, Message, Receive, Scope, Send

from openvietgis_api.adapters.http import (
    audit,
    auth,
    bootstrap,
    cad,
    cad_documents,
    cartography,
    collaboration,
    editing,
    extensions,
    external_sources,
    field,
    gis_access,
    grants,
    health,
    integration_bindings,
    integration_hub,
    invitations,
    layout_designer,
    managed_sync,
    map_viewport,
    maps,
    marketplace,
    marketplace_developers,
    me,
    model_vault,
    ops,
    org_chat,
    organizations,
    partner_platform,
    projects,
    queries,
    raster,
    saved_scenes,
    service_principals,
    spatial,
    team_chat,
    team_locks,
    team_presence,
    team_realtime,
    terrain,
    tilesets,
    transfers,
    v8i_bridge,
    workspaces,
)
from openvietgis_api.adapters.http.errors import install_error_handlers
from openvietgis_api.adapters.http.gis_access_guard import project_access_guard
from openvietgis_api.adapters.http.native_processing import install_native_processing
from openvietgis_api.adapters.http.official_modules import discover_official_module_runtimes
from openvietgis_api.config import get_settings


class PrivateDenialCacheMiddleware:
    """Apply the same fail-closed HTTP cache policy as the Rust API.

    Pure ASGI middleware preserves streaming and websocket behavior.
    """

    def __init__(self, app: ASGIApp) -> None:
        self.app = app

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        async def send_response(message: Message) -> None:
            if message["type"] == "http.response.start" and message["status"] in (401, 403, 404):
                headers = MutableHeaders(scope=message)
                if "cache-control" not in headers:
                    headers["Cache-Control"] = "private, no-store"
            await send(message)

        await self.app(scope, receive, send_response if scope["type"] == "http" else send)


def create_app() -> FastAPI:
    settings = get_settings()
    app = FastAPI(title="OpenVietGIS API", version="0.2.0")
    app.state.settings = settings

    app.add_middleware(
        SessionMiddleware,
        secret_key=settings.application_session_secret.get_secret_value(),
        session_cookie="ovg_oidc_state",
        max_age=600,
        same_site="lax",
        https_only=settings.session_cookie_secure,
    )
    app.add_middleware(
        CORSMiddleware,
        allow_origins=(
            [settings.web_base_url.rstrip("/")]
            if settings.environment == "production"
            else sorted(
                {
                    "http://127.0.0.1:5173",
                    "http://localhost:5173",
                    "http://127.0.0.1:5178",
                    settings.web_base_url.rstrip("/"),
                }
            )
        ),
        allow_credentials=True,
        allow_methods=["GET", "POST", "PATCH", "PUT", "DELETE", "OPTIONS"],
        allow_headers=["content-type", "authorization", "x-openvietgis-csrf"],
    )

    app.add_middleware(PrivateDenialCacheMiddleware)

    api_v1 = APIRouter(prefix="/api/v1")
    api_v1.include_router(health.router)
    api_v1.include_router(auth.router)
    api_v1.include_router(bootstrap.router)
    api_v1.include_router(audit.router)
    api_v1.include_router(me.router)
    api_v1.include_router(organizations.router)
    api_v1.include_router(workspaces.router)
    api_v1.include_router(projects.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(grants.router)
    api_v1.include_router(gis_access.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(invitations.router)
    api_v1.include_router(service_principals.router)
    api_v1.include_router(spatial.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(maps.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(map_viewport.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(transfers.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(editing.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(queries.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(raster.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(cartography.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(layout_designer.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(external_sources.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(integration_hub.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(integration_bindings.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(managed_sync.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(collaboration.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(cad.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(cad_documents.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(v8i_bridge.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(model_vault.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(saved_scenes.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(terrain.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(tilesets.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(team_chat.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(org_chat.router)
    api_v1.include_router(team_locks.router, dependencies=[Depends(project_access_guard)])
    # WebSocket presence authenticates its cookie, checks project read and
    # rejects scoped users in team_presence.authorize, including revalidation.
    # HTTP-only Request dependencies cannot be injected into a WebSocket.
    api_v1.include_router(team_presence.router)
    api_v1.include_router(team_realtime.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(field.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(extensions.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(partner_platform.router, dependencies=[Depends(project_access_guard)])
    api_v1.include_router(marketplace.router)
    api_v1.include_router(marketplace_developers.router)
    api_v1.include_router(ops.router)
    for module_runtime in discover_official_module_runtimes():
        api_v1.include_router(module_runtime.router, dependencies=[Depends(project_access_guard)])
    app.include_router(api_v1)
    install_native_processing(app)
    install_error_handlers(app)
    return app


app = create_app()
