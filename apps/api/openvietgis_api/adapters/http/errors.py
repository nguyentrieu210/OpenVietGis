from fastapi import FastAPI, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from sqlalchemy.exc import DBAPIError
from starlette.middleware.base import RequestResponseEndpoint
from starlette.responses import Response

from openvietgis_api.domains.authorization.service import (
    AuthorizationDeniedError,
    GrantNotFoundError,
    InvalidGrantError,
    InvalidServiceCredentialError,
    InvalidServicePrincipalError,
    ServiceCredentialNotFoundError,
    ServicePrincipalNotFoundError,
    TenantResourceNotFoundError,
)
from openvietgis_api.domains.maps.domain import MapValidationError
from openvietgis_api.domains.maps.ports import (
    MapIntentConflictError,
    MapNoChangesError,
    MapReferenceUnavailableError,
    MapRevisionConflictError,
)
from openvietgis_api.domains.maps.viewport import MapReadUnavailableError
from openvietgis_api.domains.platform_errors import PlatformDomainError
from openvietgis_api.domains.spatial.ports import (
    SpatialActiveLayerDependenciesError,
    SpatialCrsCorrectionUnverifiedError,
    SpatialInvalidCursorError,
    SpatialInvalidFeatureRequestError,
    SpatialLayerRevisionConflictError,
    SpatialNoChangesError,
    SpatialProjectReadOnlyError,
    SpatialRevisionConflictError,
    SpatialScopeError,
    SpatialSourceAlreadyRegisteredError,
    SpatialSourceNotReadyError,
    SpatialStaleReadPinError,
    SpatialTransformationOutOfAreaError,
    SpatialTransformationUnavailableError,
    SpatialUnknownReferenceKeyError,
    SpatialUnsupportedCRSError,
    SpatialUnsupportedTransformationError,
)
from openvietgis_api.domains.tenancy.service import (
    CrossOrganizationWorkspaceError,
    InvalidInvitationError,
    InvalidLifecycleTransitionError,
    InvalidOrganizationInputError,
    InvitationNotFoundError,
    LastOwnerError,
    MembershipNotFoundError,
    OrganizationNotFoundError,
    OwnerRequiredError,
    ProjectNotFoundError,
    WorkspaceHasActiveProjectsError,
    WorkspaceNotFoundError,
)
from openvietgis_api.domains.transfers.ports import TransferError

# Fixed customer-safe descriptions; never forward exception text or SQL details.
_SPATIAL_ERRORS: dict[type[Exception], tuple[int, str, str]] = {
    MapRevisionConflictError: (
        409,
        "STALE_MAP_REVISION",
        "Map revision changed; reload the committed configuration.",
    ),
    MapIntentConflictError: (
        409,
        "OPERATION_INTENT_CONFLICT",
        "This save operation cannot be reused.",
    ),
    MapNoChangesError: (409, "NO_CHANGES", "The map configuration has no material changes."),
    MapReferenceUnavailableError: (
        422,
        "MAP_REFERENCE_UNAVAILABLE",
        "A requested map Layer is unavailable.",
    ),
    MapValidationError: (422, "INVALID_MAP_REQUEST", "Invalid map request."),
    MapReadUnavailableError: (
        503,
        "MAP_READ_UNAVAILABLE",
        "Map read unavailable; retry.",
    ),
    SpatialRevisionConflictError: (
        409,
        "STALE_CATALOG_REVISION",
        "Source revision changed; read current metadata and retry.",
    ),
    SpatialLayerRevisionConflictError: (
        409,
        "STALE_LAYER_REVISION",
        "Layer revision changed; read current metadata and retry.",
    ),
    SpatialStaleReadPinError: (
        409,
        "STALE_READ_PIN",
        "Read pins changed; reload the source and start a new traversal.",
    ),
    SpatialInvalidCursorError: (422, "INVALID_CURSOR", "Cursor is not part of the current source."),
    SpatialUnsupportedCRSError: (422, "UNSUPPORTED_CRS", "Select a readable supported CRS."),
    SpatialUnsupportedTransformationError: (
        422,
        "UNSUPPORTED_TRANSFORMATION",
        "No approved direct operation exists.",
    ),
    SpatialTransformationOutOfAreaError: (
        422,
        "TRANSFORMATION_OUT_OF_AREA",
        "The source is outside the approved operation area or evidence.",
    ),
    SpatialTransformationUnavailableError: (
        422,
        "TRANSFORMATION_UNAVAILABLE",
        "The requested operation is unavailable.",
    ),
    SpatialSourceNotReadyError: (409, "SOURCE_NOT_READY", "The source is not ready."),
    SpatialProjectReadOnlyError: (409, "PROJECT_NOT_WRITABLE", "The project is read-only."),
    SpatialSourceAlreadyRegisteredError: (
        409,
        "SOURCE_ALREADY_REGISTERED",
        "This reference source is already registered.",
    ),
    SpatialActiveLayerDependenciesError: (
        409,
        "ACTIVE_LAYER_DEPENDENCIES",
        "Retire active Layers before retiring the source.",
    ),
    SpatialNoChangesError: (422, "NO_CHANGES", "The request does not change metadata."),
    SpatialUnknownReferenceKeyError: (
        422,
        "UNKNOWN_REFERENCE_KEY",
        "Select a supported platform reference fixture.",
    ),
    SpatialCrsCorrectionUnverifiedError: (
        422,
        "CRS_CORRECTION_UNVERIFIED",
        "The requested CRS correction cannot be verified.",
    ),
}


DATABASE_GUARD_SQLSTATE = "OV409"


async def _spatial_error_handler(_: Request, exc: Exception) -> JSONResponse:
    status, code, detail = _SPATIAL_ERRORS[type(exc)]
    return JSONResponse(status_code=status, content={"code": code, "detail": detail})


def install_error_handlers(app: FastAPI) -> None:
    # Exceptions propagate out of function-scoped database dependencies first
    # (rollback), then are rendered by FastAPI. A route must not swallow them
    # and return a normal response that would commit an interrupted mutation.
    for exception_type in _SPATIAL_ERRORS:
        app.add_exception_handler(exception_type, _spatial_error_handler)

    @app.exception_handler(SpatialScopeError)
    async def spatial_scope_denied_handler(_: Request, exc: Exception) -> JSONResponse:
        return JSONResponse(status_code=403, content={"detail": "permission denied"})

    @app.exception_handler(SpatialInvalidFeatureRequestError)
    @app.exception_handler(RequestValidationError)
    async def request_validation_handler(
        _: Request,
        exc: RequestValidationError | SpatialInvalidFeatureRequestError,
    ) -> JSONResponse:
        # FastAPI's default validation body echoes raw input. Authentication/invitation
        # requests can include one-time secrets, so expose no input, loc or context.
        return JSONResponse(
            status_code=422,
            content={
                "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
            },
        )

    @app.exception_handler(PlatformDomainError)
    async def platform_domain_error_handler(_: Request, exc: Exception) -> JSONResponse:
        assert isinstance(exc, PlatformDomainError)
        payload: dict[str, object] = {"code": exc.code, "message": exc.message}
        if exc.details:
            payload["details"] = exc.details
        return JSONResponse(status_code=exc.status_code, content=payload)

    @app.exception_handler(DBAPIError)
    async def database_guard_handler(_: Request, exc: Exception) -> JSONResponse:
        # SQLSTATE OV409: a database guard (e.g. a module lock trigger) refused the change.
        orig = getattr(exc, "orig", None)
        if getattr(orig, "sqlstate", None) != DATABASE_GUARD_SQLSTATE:
            raise exc
        diag = getattr(orig, "diag", None)
        message = getattr(diag, "message_primary", None) or "Dữ liệu đang bị khóa"
        return JSONResponse(status_code=409, content={"code": "DATA_LOCKED", "message": message})

    @app.exception_handler(TransferError)
    async def transfer_error_handler(_: Request, exc: Exception) -> JSONResponse:
        assert isinstance(exc, TransferError)
        return JSONResponse(
            status_code=exc.status_code,
            content={"code": exc.code, "message": exc.message},
        )

    @app.exception_handler(AuthorizationDeniedError)
    @app.exception_handler(OwnerRequiredError)
    async def authorization_denied_handler(
        _: Request,
        exc: Exception,
    ) -> JSONResponse:
        return JSONResponse(status_code=403, content={"detail": str(exc)})

    @app.exception_handler(TenantResourceNotFoundError)
    @app.exception_handler(WorkspaceNotFoundError)
    @app.exception_handler(ProjectNotFoundError)
    @app.exception_handler(OrganizationNotFoundError)
    @app.exception_handler(MembershipNotFoundError)
    @app.exception_handler(GrantNotFoundError)
    @app.exception_handler(InvitationNotFoundError)
    @app.exception_handler(ServicePrincipalNotFoundError)
    @app.exception_handler(ServiceCredentialNotFoundError)
    async def tenant_not_found_handler(
        _: Request,
        exc: Exception,
    ) -> JSONResponse:
        return JSONResponse(status_code=404, content={"detail": str(exc)})

    @app.exception_handler(CrossOrganizationWorkspaceError)
    @app.exception_handler(InvalidLifecycleTransitionError)
    @app.exception_handler(WorkspaceHasActiveProjectsError)
    @app.exception_handler(InvalidOrganizationInputError)
    @app.exception_handler(InvalidGrantError)
    @app.exception_handler(InvalidServiceCredentialError)
    @app.exception_handler(InvalidServicePrincipalError)
    @app.exception_handler(InvalidInvitationError)
    @app.exception_handler(LastOwnerError)
    async def tenancy_validation_handler(
        _: Request,
        exc: Exception,
    ) -> JSONResponse:
        return JSONResponse(status_code=400, content={"detail": str(exc)})

    @app.middleware("http")
    async def private_no_store_error_responses(
        request: Request, call_next: RequestResponseEndpoint
    ) -> Response:
        response = await call_next(request)
        if response.status_code in {401, 403, 404} and "cache-control" not in response.headers:
            response.headers["Cache-Control"] = "private, no-store"
        return response
