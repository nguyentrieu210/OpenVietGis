"""HTTP runtime for the trusted official House Numbering module."""

from __future__ import annotations

import json
from datetime import date, datetime
from typing import Annotated, Any, Literal
from uuid import UUID

from fastapi import APIRouter, Depends, File, Form, Query, Response, UploadFile
from openvietgis_api.domains.platform_errors import PlatformValidationError
from openvietgis_api.module_sdk import (
    AuthenticatedRequest,
    authenticate_request,
    context_for,
    get_db_session,
    require_csrf,
)
from pydantic import BaseModel, ConfigDict, Field, PositiveInt, field_validator
from sqlalchemy.orm import Session

from modules.house_numbering.address_prefix import canonical_prefix
from modules.house_numbering.application import HouseNumberingService
from modules.house_numbering.documents import HouseNumberingDocumentService
from modules.house_numbering.exports import HouseNumberingExportService
from modules.house_numbering.imports import HouseNumberingImporter
from modules.house_numbering.privacy import HouseNumberingPrivateData
from modules.house_numbering.progress import build_progress_xlsx
from modules.house_numbering.reconciliation import HouseNumberingReconciliation
from modules.house_numbering.survey import HouseNumberingSurveyService

router = APIRouter(tags=["house-numbering"])
_PROJ = "/organizations/{organization_id}/projects/{project_id}/modules/house-numbering"


def _service(
    organization_id: UUID,
    project_id: UUID,
    auth: AuthenticatedRequest,
    session: Session,
) -> HouseNumberingService:
    return HouseNumberingService(
        session,
        context_for(auth, organization_id=organization_id, project_id=project_id),
    )


class RuleCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    version: str = Field(min_length=1, max_length=64)
    status: Literal["draft", "authoritative"] = "draft"
    authority_ref: str | None = Field(default=None, max_length=500)
    left_odd: bool = True
    suffix_style: Literal["letters", "dash"] = "letters"
    max_distance_m: float = Field(default=150, gt=0, le=5000)


class AreaCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: str = Field(min_length=1, max_length=200)
    kind: Literal["current", "historic", "custom"] = "current"
    parent_id: UUID | None = None
    source_id: UUID | None = None
    feature_id: str | None = Field(default=None, max_length=128)


class AreaUpdate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: PositiveInt
    name: str = Field(min_length=1, max_length=200)
    kind: Literal["current", "historic", "custom"]
    parent_id: UUID | None = None
    source_id: UUID | None = None
    feature_id: str | None = Field(default=None, max_length=128)


class RouteSegmentLink(BaseModel):
    model_config = ConfigDict(extra="forbid")
    source_id: UUID
    feature_id: str = Field(min_length=1, max_length=128)
    code: str = Field(default="", max_length=160)


class RouteCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: str = Field(min_length=1, max_length=200)
    route_type: Literal["street", "alley"] = "street"
    parent_route_id: UUID | None = None
    prefix: str | None = Field(default=None, max_length=100)
    source_id: UUID
    feature_id: str = Field(min_length=1, max_length=128)
    direction_state: Literal["forward", "reverse"] = "forward"
    source_label: str = Field(default="manual", min_length=1, max_length=200)
    segments: list[RouteSegmentLink] = Field(default_factory=list, max_length=200)
    status: Literal["draft", "confirmed"] = "draft"

    @field_validator("prefix")
    @classmethod
    def valid_prefix(cls, value: str | None) -> str | None:
        if value is None:
            return None
        return canonical_prefix(value)


class RouteImportLayer(BaseModel):
    model_config = ConfigDict(extra="forbid")
    source_id: UUID
    mapping: dict[Literal["name", "code", "parent_code", "prefix", "route_type", "logical_code"], str]
    default_type: Literal["street", "alley"] = "street"
    direction_state: Literal["forward", "reverse"] = "forward"


class RouteImportOverride(BaseModel):
    model_config = ConfigDict(extra="forbid")
    parent_code: str = Field(max_length=200)
    prefix: str = Field(max_length=100)


class RouteImportBody(BaseModel):
    model_config = ConfigDict(extra="forbid")
    layers: list[RouteImportLayer] = Field(min_length=1, max_length=20)
    overrides: dict[str, RouteImportOverride] = Field(default_factory=dict, max_length=5000)


class RouteImportApply(RouteImportBody):
    preview_token: str = Field(pattern=r"^[a-f0-9]{64}$")


class RouteUpdate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: PositiveInt
    name: str = Field(min_length=1, max_length=200)
    direction_state: Literal["forward", "reverse"]
    status: Literal["draft", "confirmed"]


class BuildingAssignmentPreview(BaseModel):
    model_config = ConfigDict(extra="forbid")
    source_id: UUID | None = None
    after_id: UUID | None = None
    limit: int = Field(default=200, ge=1, le=500)
    mapping: dict[Literal["road", "alley", "code"], str] = Field(
        default_factory=lambda: {"road": "ten_duong", "alley": "hem", "code": "tuyen_id"}
    )
    max_distance_m: float = Field(default=80, ge=5, le=500)


class BuildingAssignmentApply(BuildingAssignmentPreview):
    preview_token: str = Field(pattern=r"^[a-f0-9]{64}$")
    assignments: dict[UUID, UUID] = Field(min_length=1, max_length=500)


class BuildingCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    route_id: UUID | None = None
    area_id: UUID | None = None
    parcel_ref: str | None = Field(default=None, max_length=200)
    source_id: UUID
    feature_id: str = Field(min_length=1, max_length=128)
    eligibility: Literal["unknown", "eligible", "ineligible"] = "unknown"
    ineligibility_reason: str | None = Field(default=None, max_length=500)


class BuildingUpdate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: PositiveInt
    route_id: UUID | None = None
    area_id: UUID | None = None
    parcel_ref: str | None = Field(default=None, max_length=200)
    source_id: UUID
    feature_id: str = Field(min_length=1, max_length=128)


class EligibilityUpdate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: PositiveInt
    eligibility: Literal["unknown", "eligible", "ineligible"]
    reason: str | None = Field(default=None, max_length=500)


class PlaqueUpdate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: PositiveInt
    status: Literal["not_planned", "planned", "installed", "replaced", "removed"]


class BuildingLineageCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    relation_kind: Literal["split", "merge"]
    operation_id: UUID
    predecessor_building_ids: list[UUID] = Field(min_length=1, max_length=100)
    successor_building_ids: list[UUID] = Field(min_length=1, max_length=100)
    expected_revisions: dict[str, PositiveInt]


class InspectRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    x: float
    y: float
    crs_code: int = Field(gt=0)
    route_id: UUID
    rule_id: UUID
    existing_building_id: UUID | None = None


class PreviewRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    building_ids: list[UUID] = Field(min_length=1, max_length=5000)
    rule_id: UUID
    operation_id: UUID


class ProposalCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    scope_kind: Literal["route", "area"]
    scope_id: UUID
    rule_id: UUID
    building_ids: list[UUID] = Field(min_length=1, max_length=5000)


class ProposalTransition(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: PositiveInt
    action: Literal["submit", "withdraw", "approve", "reject"]
    comment: str | None = Field(default=None, max_length=1000)


@router.get(f"{_PROJ}/rules")
def list_rules(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return {"items": _service(organization_id, project_id, auth, session).list_rules()}


@router.post(f"{_PROJ}/rules", status_code=201)
def create_rule(
    organization_id: UUID,
    project_id: UUID,
    body: RuleCreate,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).create_rule(
        **body.model_dump()
    )


@router.get(f"{_PROJ}/areas")
def list_areas(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return {"items": _service(organization_id, project_id, auth, session).list_areas()}


@router.post(f"{_PROJ}/areas", status_code=201)
def create_area(
    organization_id: UUID,
    project_id: UUID,
    body: AreaCreate,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).create_area(
        **body.model_dump()
    )


@router.put(f"{_PROJ}/areas/{{area_id}}")
def update_area(
    organization_id: UUID,
    project_id: UUID,
    area_id: UUID,
    body: AreaUpdate,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).update_area(
        area_id, **body.model_dump()
    )


@router.delete(f"{_PROJ}/areas/{{area_id}}", status_code=204)
def delete_area(
    organization_id: UUID,
    project_id: UUID,
    area_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    expected_revision: Annotated[PositiveInt, Query()],
) -> Response:
    _service(organization_id, project_id, auth, session).delete_area(
        area_id, expected_revision=expected_revision
    )
    return Response(status_code=204)


@router.get(f"{_PROJ}/routes")
def list_routes(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return {"items": _service(organization_id, project_id, auth, session).list_routes()}


@router.post(f"{_PROJ}/routes", status_code=201)
def create_route(
    organization_id: UUID,
    project_id: UUID,
    body: RouteCreate,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).create_route(
        **body.model_dump()
    )


@router.post(f"{_PROJ}/route-import/preview")
def preview_route_import(
    organization_id: UUID,
    project_id: UUID,
    body: RouteImportBody,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    from modules.house_numbering.route_import import RouteTreeImporter
    service = _service(organization_id, project_id, auth, session)
    return RouteTreeImporter(service.repo).preview(
        [layer.model_dump() for layer in body.layers],
        {key: value.model_dump() for key, value in body.overrides.items()},
    )


@router.post(f"{_PROJ}/route-import/apply", status_code=201)
def apply_route_import(
    organization_id: UUID,
    project_id: UUID,
    body: RouteImportApply,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    from modules.house_numbering.route_import import RouteTreeImporter
    service = _service(organization_id, project_id, auth, session)
    return RouteTreeImporter(service.repo).apply(
        [layer.model_dump() for layer in body.layers],
        {key: value.model_dump() for key, value in body.overrides.items()},
        body.preview_token,
    )


@router.put(f"{_PROJ}/routes/{{route_id}}")
def update_route(
    organization_id: UUID,
    project_id: UUID,
    route_id: UUID,
    body: RouteUpdate,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).update_route(
        route_id, **body.model_dump()
    )


@router.delete(f"{_PROJ}/routes/{{route_id}}", status_code=204)
def delete_route(
    organization_id: UUID,
    project_id: UUID,
    route_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> Response:
    _service(organization_id, project_id, auth, session).delete_route(route_id)
    return Response(status_code=204)


@router.get(f"{_PROJ}/buildings")
def list_buildings(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    route_id: Annotated[UUID | None, Query()] = None,
) -> dict[str, object]:
    return {
        "items": _service(organization_id, project_id, auth, session).list_buildings(
            route_id=route_id
        )
    }


@router.post(f"{_PROJ}/building-assignment/preview")
def preview_building_assignment(
    organization_id: UUID,
    project_id: UUID,
    body: BuildingAssignmentPreview,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    from modules.house_numbering.building_assignment import BuildingRouteAssigner
    service = _service(organization_id, project_id, auth, session)
    return BuildingRouteAssigner(service.repo).preview(**body.model_dump())


@router.post(f"{_PROJ}/building-assignment/apply")
def apply_building_assignment(
    organization_id: UUID,
    project_id: UUID,
    body: BuildingAssignmentApply,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    from modules.house_numbering.building_assignment import BuildingRouteAssigner
    service = _service(organization_id, project_id, auth, session)
    return BuildingRouteAssigner(service.repo).apply(**body.model_dump())


@router.post(f"{_PROJ}/buildings", status_code=201)
def create_building(
    organization_id: UUID,
    project_id: UUID,
    body: BuildingCreate,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).create_building(
        **body.model_dump()
    )


@router.put(f"{_PROJ}/buildings/{{building_id}}")
def update_building(
    organization_id: UUID,
    project_id: UUID,
    building_id: UUID,
    body: BuildingUpdate,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).update_building(
        building_id, **body.model_dump()
    )


@router.delete(f"{_PROJ}/buildings/{{building_id}}", status_code=204)
def retire_building(
    organization_id: UUID,
    project_id: UUID,
    building_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    expected_revision: Annotated[PositiveInt, Query()],
) -> Response:
    _service(organization_id, project_id, auth, session).retire_building(
        building_id, expected_revision=expected_revision
    )
    return Response(status_code=204)


@router.patch(f"{_PROJ}/buildings/{{building_id}}/eligibility")
def set_eligibility(
    organization_id: UUID,
    project_id: UUID,
    building_id: UUID,
    body: EligibilityUpdate,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(
        organization_id, project_id, auth, session
    ).set_building_eligibility(
        building_id,
        expected_revision=body.expected_revision,
        eligibility=body.eligibility,
        reason=body.reason,
    )


@router.patch(f"{_PROJ}/buildings/{{building_id}}/plaque")
def set_building_plaque(
    organization_id: UUID,
    project_id: UUID,
    building_id: UUID,
    body: PlaqueUpdate,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).set_building_plaque(
        building_id,
        expected_revision=body.expected_revision,
        status=body.status,
    )


@router.post(f"{_PROJ}/buildings/lineage", status_code=201)
def record_building_lineage(
    organization_id: UUID,
    project_id: UUID,
    body: BuildingLineageCreate,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).record_building_lineage(
        **body.model_dump()
    )


@router.get(f"{_PROJ}/buildings/{{building_id}}/lineage")
def list_building_lineage(
    organization_id: UUID,
    project_id: UUID,
    building_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return {
        "items": _service(
            organization_id, project_id, auth, session
        ).list_building_lineage(building_id)
    }


@router.post(f"{_PROJ}/inspect")
def inspect_location(
    organization_id: UUID,
    project_id: UUID,
    body: InspectRequest,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).inspect(
        **body.model_dump()
    )


@router.post(f"{_PROJ}/previews", status_code=201)
def preview_numbering(
    organization_id: UUID,
    project_id: UUID,
    body: PreviewRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).preview_assignment(
        **body.model_dump()
    )


@router.post(f"{_PROJ}/previews/{{preview_id}}/apply")
def apply_numbering(
    organization_id: UUID,
    project_id: UUID,
    preview_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return {
        "items": _service(organization_id, project_id, auth, session).apply_preview(
            preview_id
        )
    }


@router.post(f"{_PROJ}/proposals", status_code=201)
def create_proposal(
    organization_id: UUID,
    project_id: UUID,
    body: ProposalCreate,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).create_proposal(
        **body.model_dump()
    )


@router.get(f"{_PROJ}/proposals/{{proposal_id}}")
def get_proposal(
    organization_id: UUID,
    project_id: UUID,
    proposal_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return _service(organization_id, project_id, auth, session).get_proposal(
        proposal_id
    )


@router.post(f"{_PROJ}/proposals/{{proposal_id}}/transition")
def transition_proposal(
    organization_id: UUID,
    project_id: UUID,
    proposal_id: UUID,
    body: ProposalTransition,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    reviewer = body.action in {"approve", "reject"}
    return _service(organization_id, project_id, auth, session).transition_proposal(
        proposal_id,
        expected_revision=body.expected_revision,
        action=body.action,
        comment=body.comment,
        reviewer=reviewer,
    )


class RebuildReconciliationRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    removed_building_ids: list[UUID] = Field(default_factory=list, max_length=5000)
    estimated_building_ids: list[UUID] = Field(default_factory=list, max_length=5000)


class ResolveCaseRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: PositiveInt
    status: Literal["resolved", "ignored"]
    resolution: str = Field(min_length=1, max_length=1000)
    building_id: UUID | None = None


class SurveySyncRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    operations: list[dict[str, Any]] = Field(min_length=1, max_length=500)


class SurveyReviewRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: PositiveInt
    decision: Literal["accepted", "rejected"]


class AssignmentCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    scope_kind: Literal["area", "route"]
    scope_id: UUID
    assignee_actor_id: UUID
    starts_on: date
    ends_on: date | None = None
    target_count: int | None = Field(default=None, ge=0)


class AssignmentReassign(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: PositiveInt
    assignee_actor_id: UUID


class DocumentIssueRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    building_id: UUID
    doc_type: Literal["inspection", "certificate", "house_form", "plate"]
    output_format: Literal["docx", "pdf"] = "pdf"
    template_revision: str = Field(min_length=1, max_length=100)
    template_verified: bool = False
    replaces_document_id: UUID | None = None


class DocumentBatchRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    building_ids: list[UUID] = Field(min_length=1, max_length=100)
    doc_type: Literal["inspection", "certificate", "house_form", "plate"]
    output_format: Literal["docx", "pdf"] = "pdf"
    template_revision: str = Field(min_length=1, max_length=100)
    template_verified: bool = False


class ExportRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    fields: list[str] = Field(min_length=1, max_length=100)
    output_format: Literal["csv", "geojson", "dxf", "shapefile_zip"]
    include_private: bool = False
    purpose: str | None = Field(default=None, max_length=500)


class PrivateDataRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    building_id: UUID
    purpose: str = Field(min_length=1, max_length=500)
    consent: bool = False
    lawful_basis_ref: str | None = Field(default=None, max_length=500)
    protected_data: dict[str, Any]
    expires_at: datetime | None = None
    record_id: UUID | None = None
    expected_revision: PositiveInt | None = None


def _ctx(organization_id: UUID, project_id: UUID, auth: AuthenticatedRequest):
    return context_for(auth, organization_id=organization_id, project_id=project_id)


@router.post(f"{_PROJ}/imports", status_code=201)
async def import_evidence(
    organization_id: UUID,
    project_id: UUID,
    source_format: Annotated[str, Form()],
    origin: Annotated[str, Form()],
    mapping_json: Annotated[str, Form()],
    default_source_reference: Annotated[str, Form()],
    file: Annotated[UploadFile, File()],
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    source_crs: Annotated[int | None, Form()] = None,
    default_confidence: Annotated[str, Form()] = "observed",
) -> dict[str, object]:
    try:
        mapping = json.loads(mapping_json)
    except json.JSONDecodeError as exc:
        raise PlatformValidationError("mapping_json must be valid JSON") from exc
    if not isinstance(mapping, dict):
        raise PlatformValidationError("mapping_json must be an object")
    data = await file.read()
    result = HouseNumberingImporter(
        session, _ctx(organization_id, project_id, auth)
    ).stage(
        filename=file.filename or "import.bin",
        source_format=source_format,
        origin=origin,
        data=data,
        mapping={str(k): str(v) for k, v in mapping.items()},
        source_crs=source_crs,
        default_source_reference=default_source_reference,
        default_confidence=default_confidence,
    )
    return result


@router.get(f"{_PROJ}/imports")
def list_imports(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    items = HouseNumberingImporter(
        session, _ctx(organization_id, project_id, auth)
    ).list_batches()
    return {"items": items}


@router.post(f"{_PROJ}/reconciliation/rebuild")
def rebuild_reconciliation(
    organization_id: UUID,
    project_id: UUID,
    body: RebuildReconciliationRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    items = HouseNumberingReconciliation(
        session, _ctx(organization_id, project_id, auth)
    ).rebuild_cases(
        removed_building_ids=set(body.removed_building_ids),
        estimated_building_ids=set(body.estimated_building_ids),
    )
    return {"items": items}


@router.get(f"{_PROJ}/reconciliation/cases")
def list_reconciliation_cases(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    status: Annotated[str | None, Query()] = None,
    case_class: Annotated[str | None, Query()] = None,
) -> dict[str, object]:
    items = HouseNumberingReconciliation(
        session, _ctx(organization_id, project_id, auth)
    ).list_cases(status=status, case_class=case_class)
    return {"items": items}


@router.get(f"{_PROJ}/reconciliation/evidence/{{evidence_id}}/suggest")
def suggest_evidence_match(
    organization_id: UUID,
    project_id: UUID,
    evidence_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return HouseNumberingReconciliation(
        session, _ctx(organization_id, project_id, auth)
    ).suggest_unmatched_certificate(evidence_id)


@router.post(f"{_PROJ}/reconciliation/cases/{{case_id}}/resolve")
def resolve_reconciliation_case(
    organization_id: UUID,
    project_id: UUID,
    case_id: UUID,
    body: ResolveCaseRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return HouseNumberingReconciliation(
        session, _ctx(organization_id, project_id, auth)
    ).resolve_case(case_id, **body.model_dump())


@router.post(f"{_PROJ}/survey/photos", status_code=201)
async def upload_survey_photo(
    organization_id: UUID,
    project_id: UUID,
    file: Annotated[UploadFile, File()],
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    data = await file.read()
    return HouseNumberingSurveyService(
        session, _ctx(organization_id, project_id, auth)
    ).upload_photo(
        content_type=file.content_type or "application/octet-stream", data=data
    )


@router.post(f"{_PROJ}/survey/sync")
def sync_surveys(
    organization_id: UUID,
    project_id: UUID,
    body: SurveySyncRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return HouseNumberingSurveyService(
        session, _ctx(organization_id, project_id, auth)
    ).sync(body.operations)


@router.get(f"{_PROJ}/surveys")
def list_surveys(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    state: Annotated[str | None, Query()] = None,
) -> dict[str, object]:
    items = HouseNumberingSurveyService(
        session, _ctx(organization_id, project_id, auth)
    ).list_surveys(state=state)
    return {"items": items}


@router.post(f"{_PROJ}/surveys/{{survey_id}}/review")
def review_survey(
    organization_id: UUID,
    project_id: UUID,
    survey_id: UUID,
    body: SurveyReviewRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return HouseNumberingSurveyService(
        session, _ctx(organization_id, project_id, auth)
    ).review(survey_id, **body.model_dump())


@router.get(f"{_PROJ}/assignments")
def list_assignments(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return {
        "items": HouseNumberingSurveyService(
            session, _ctx(organization_id, project_id, auth)
        ).list_assignments()
    }


@router.post(f"{_PROJ}/assignments", status_code=201)
def create_assignment(
    organization_id: UUID,
    project_id: UUID,
    body: AssignmentCreate,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return HouseNumberingSurveyService(
        session, _ctx(organization_id, project_id, auth)
    ).create_assignment(**body.model_dump())


@router.patch(f"{_PROJ}/assignments/{{assignment_id}}")
def reassign_assignment(
    organization_id: UUID,
    project_id: UUID,
    assignment_id: UUID,
    body: AssignmentReassign,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return HouseNumberingSurveyService(
        session, _ctx(organization_id, project_id, auth)
    ).reassign(assignment_id, **body.model_dump())


@router.get(f"{_PROJ}/progress")
def progress_summary(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return HouseNumberingSurveyService(
        session, _ctx(organization_id, project_id, auth)
    ).progress()


@router.get(f"{_PROJ}/progress/report.xlsx")
def progress_report(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> Response:
    service = HouseNumberingSurveyService(
        session, _ctx(organization_id, project_id, auth)
    )
    data = build_progress_xlsx(summary=service.progress(), rows=service.report_rows())
    return Response(
        content=data,
        media_type="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        headers={
            "Content-Disposition": 'attachment; filename="house-numbering-progress.xlsx"'
        },
    )


@router.post(f"{_PROJ}/documents", status_code=201)
def issue_document(
    organization_id: UUID,
    project_id: UUID,
    body: DocumentIssueRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return HouseNumberingDocumentService(
        session, _ctx(organization_id, project_id, auth)
    ).issue(**body.model_dump())


@router.post(f"{_PROJ}/documents/batch", status_code=201)
def issue_document_batch(
    organization_id: UUID,
    project_id: UUID,
    body: DocumentBatchRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    items = HouseNumberingDocumentService(
        session, _ctx(organization_id, project_id, auth)
    ).issue_batch(**body.model_dump())
    return {"items": items}


@router.get(f"{_PROJ}/buildings/{{building_id}}/documents")
def document_history(
    organization_id: UUID,
    project_id: UUID,
    building_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return {
        "items": HouseNumberingDocumentService(
            session, _ctx(organization_id, project_id, auth)
        ).history(building_id)
    }


@router.get(f"{_PROJ}/documents/{{document_id}}/download")
def download_document(
    organization_id: UUID,
    project_id: UUID,
    document_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> Response:
    meta, data = HouseNumberingDocumentService(
        session, _ctx(organization_id, project_id, auth)
    ).download(document_id)
    return Response(
        content=data,
        media_type=str(meta["content_type"]),
        headers={"Content-Disposition": f'attachment; filename="{meta["filename"]}"'},
    )


class IntegrationAddress(BaseModel):
    address_id: UUID
    house_number: str
    street: str | None
    area: str | None
    longitude: float
    latitude: float


class IntegrationAddressResult(BaseModel):
    items: list[IntegrationAddress]
    crs: Literal["EPSG:4326"] = "EPSG:4326"


@router.get(f"{_PROJ}/integrations/v1/addresses", response_model=IntegrationAddressResult)
def integration_address_lookup(
    organization_id: UUID,
    project_id: UUID,
    response: Response,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    query: Annotated[str, Query(min_length=2, max_length=120)],
    limit: Annotated[int, Query(ge=1, le=100)] = 50,
) -> dict[str, object]:
    """Look up approved addresses using an authenticated project credential."""
    response.headers["Cache-Control"] = "no-store"
    items = HouseNumberingExportService(
        session, _ctx(organization_id, project_id, auth)
    ).lookup_approved_addresses(query=query, limit=limit)
    return {"items": items, "crs": "EPSG:4326"}


@router.get(f"{_PROJ}/search")
def search_house_numbers(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    query: Annotated[str | None, Query()] = None,
    x: Annotated[float | None, Query()] = None,
    y: Annotated[float | None, Query()] = None,
    crs_code: Annotated[int | None, Query()] = None,
    max_distance: Annotated[float, Query(gt=0, le=10000)] = 100.0,
    limit: Annotated[int, Query(ge=1, le=500)] = 100,
) -> dict[str, object]:
    items = HouseNumberingExportService(
        session, _ctx(organization_id, project_id, auth)
    ).search(
        query=query,
        x=x,
        y=y,
        crs_code=crs_code,
        max_distance=max_distance,
        limit=limit,
    )
    return {"items": items}


@router.post(f"{_PROJ}/exports", status_code=201)
def create_export(
    organization_id: UUID,
    project_id: UUID,
    body: ExportRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return HouseNumberingExportService(
        session, _ctx(organization_id, project_id, auth)
    ).export(**body.model_dump())


@router.get(f"{_PROJ}/exports/{{artifact_id}}/download")
def download_export(
    organization_id: UUID,
    project_id: UUID,
    artifact_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> Response:
    meta, data = HouseNumberingExportService(
        session, _ctx(organization_id, project_id, auth)
    ).download(artifact_id)
    return Response(
        content=data,
        media_type=str(meta["content_type"]),
        headers={"Content-Disposition": f'attachment; filename="{meta["filename"]}"'},
    )


@router.post(f"{_PROJ}/private-households", status_code=201)
def upsert_private_household(
    organization_id: UUID,
    project_id: UUID,
    body: PrivateDataRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    return HouseNumberingPrivateData(
        session, _ctx(organization_id, project_id, auth)
    ).upsert(**body.model_dump())


@router.get(f"{_PROJ}/buildings/{{building_id}}/private-households")
def read_private_households(
    organization_id: UUID,
    project_id: UUID,
    building_id: UUID,
    purpose: Annotated[str, Query(min_length=1, max_length=500)],
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    items = HouseNumberingPrivateData(
        session, _ctx(organization_id, project_id, auth)
    ).read_for_building(building_id, purpose=purpose)
    return {"items": items}
