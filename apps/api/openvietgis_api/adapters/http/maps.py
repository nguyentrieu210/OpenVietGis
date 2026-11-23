"""Strict Project-scoped map configuration HTTP interface."""

from __future__ import annotations

from typing import Annotated, Literal, Self
from uuid import UUID

from fastapi import APIRouter, Depends, Query, Response
from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator
from sqlalchemy import text
from sqlalchemy.orm import Session
from openvietgis_api.adapters.persistence.gis_access import scoped

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.map_repository import SqlMapRepository
from openvietgis_api.domains.maps.domain import (
    MAX_REVISION,
    LayerReference,
    SaveIntent,
    Style,
    Viewpoint,
    ViewpointIntent,
)
from openvietgis_api.domains.maps.service import MapService

router = APIRouter(tags=["maps"])
_PATH = "/organizations/{organization_id}/projects/{project_id}/map"
LogicalUUID = Annotated[UUID, Field(strict=False)]
FiniteNumber = Annotated[float, Field(strict=True, allow_inf_nan=False)]


class StrictRequest(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)


class BasicStyleRequest(StrictRequest):
    color: Annotated[str, Field(pattern=r"^#[0-9A-Fa-f]{6}$")]
    opacity: Annotated[FiniteNumber, Field(ge=0, le=1)]
    width: Annotated[FiniteNumber, Field(ge=1, le=12)]
    radius: Annotated[FiniteNumber, Field(ge=2, le=20)]

    def domain(self) -> Style:
        return Style(self.color, self.opacity, self.width, self.radius)


class MapLayerRequest(StrictRequest):
    layer_id: LogicalUUID
    visible: bool
    style: BasicStyleRequest

    @field_validator("layer_id", mode="before")
    @classmethod
    def string_uuid(cls, value: object) -> object:
        if not isinstance(value, str):
            raise ValueError("logical identifier must be text")
        return value


class ViewpointRequest(StrictRequest):
    longitude: Annotated[FiniteNumber, Field(ge=-180, le=180)]
    latitude: Annotated[FiniteNumber, Field(ge=-85.05112878, le=85.05112878)]
    zoom: Annotated[FiniteNumber, Field(ge=0, le=22)]
    bearing: Annotated[FiniteNumber, Field(ge=-180, le=180)]
    pitch: Annotated[FiniteNumber, Field(ge=0, le=60)]
    crs: Literal[4326]

    @field_validator("crs", mode="before")
    @classmethod
    def integer_crs(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("integer CRS code required")
        return value

    def domain(self) -> Viewpoint:
        return Viewpoint(
            self.longitude, self.latitude, self.zoom, self.bearing, self.pitch, self.crs
        )


class PreserveViewpointRequest(StrictRequest):
    mode: Literal["preserve"]


class ClearViewpointRequest(StrictRequest):
    mode: Literal["clear"]


class SetViewpointRequest(StrictRequest):
    mode: Literal["set"]
    value: ViewpointRequest


class SaveMapRequest(StrictRequest):
    operation_id: LogicalUUID
    expected_revision: Annotated[int, Field(strict=True, ge=0, le=MAX_REVISION)]
    layers: Annotated[list[MapLayerRequest], Field(max_length=32)]
    basemap: Literal["grid"]
    default_viewpoint: Annotated[
        PreserveViewpointRequest | ClearViewpointRequest | SetViewpointRequest,
        Field(discriminator="mode"),
    ]

    @field_validator("operation_id", mode="before")
    @classmethod
    def string_uuid(cls, value: object) -> object:
        if not isinstance(value, str):
            raise ValueError("logical identifier must be text")
        return value

    @model_validator(mode="after")
    def unique_layers(self) -> Self:
        if len({item.layer_id for item in self.layers}) != len(self.layers):
            raise ValueError("duplicate map Layer")
        return self

    def domain(self) -> SaveIntent:
        choice = self.default_viewpoint
        return SaveIntent(
            self.operation_id,
            self.expected_revision,
            tuple(
                LayerReference(item.layer_id, item.visible, item.style.domain())
                for item in self.layers
            ),
            self.basemap,
            ViewpointIntent(
                choice.mode,
                choice.value.domain() if isinstance(choice, SetViewpointRequest) else None,
            ),
        )


@router.get(_PATH, operation_id="getMap")
def get_map(
    organization_id: UUID,
    project_id: UUID,
    response: Response,
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    response.headers["Cache-Control"] = "no-store"
    context = context_for(authentication, organization_id=organization_id, project_id=project_id)
    wire=MapService(SqlMapRepository(session)).get_map(context=context).to_wire()
    if scoped(session,context):
        allowed={str(x) for x in session.execute(text("SELECT id FROM spatial.layers WHERE organization_id=:org AND project_id=:proj"),{"org":organization_id,"proj":project_id}).scalars()}
        config=wire.get("configuration")
        if config:
            config["layers"]=[l for l in config.get("layers",[]) if str(l.get("layer_id")) in allowed]
            config["default_viewpoint"]=None
    return wire


@router.put(_PATH, operation_id="saveMap")
def save_map(
    organization_id: UUID,
    project_id: UUID,
    payload: SaveMapRequest,
    response: Response,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    response.headers["Cache-Control"] = "no-store"
    context = context_for(authentication, organization_id=organization_id, project_id=project_id)
    return (
        MapService(SqlMapRepository(session))
        .save(context=context, intent=payload.domain())
        .to_wire()
    )


@router.get(_PATH + "/operations/{operation_id}", operation_id="getMapOperation")
def get_operation(
    organization_id: UUID,
    project_id: UUID,
    operation_id: UUID,
    response: Response,
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    response.headers["Cache-Control"] = "no-store"
    context = context_for(authentication, organization_id=organization_id, project_id=project_id)
    return (
        MapService(SqlMapRepository(session))
        .operation(context=context, operation_id=operation_id)
        .to_wire()
    )


@router.get(_PATH + "/history", operation_id="listMapHistory")
def history(
    organization_id: UUID,
    project_id: UUID,
    response: Response,
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    limit: Annotated[int, Query(ge=1, le=100)] = 50,
    after_revision: Annotated[int | None, Query(ge=1, le=MAX_REVISION)] = None,
) -> dict[str, object]:
    response.headers["Cache-Control"] = "no-store"
    context = context_for(authentication, organization_id=organization_id, project_id=project_id)
    items, after = MapService(SqlMapRepository(session)).history(
        context=context,
        limit=limit,
        after_revision=after_revision,
    )
    return {"items": [item.to_wire() for item in items], "next_after_revision": after}
