"""US1 HTTP contract tests: real ASGI routes, mocked transaction/fixture verification.

The fake implements only the existing scoped persistence port and native fixture
verification seam. Authentication identity lookup is replaced, not CSRF or HTTP
authorization action selection. Native PostGIS verification, RLS and rollback are
separately exercised by restricted-role integration tests (T019/T026).
"""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from dataclasses import dataclass, field, replace
from datetime import datetime
from pathlib import Path
from uuid import UUID

import pytest
import yaml
from fastapi import HTTPException, Request
from fastapi.testclient import TestClient
from httpx import Response
from jsonschema import Draft202012Validator
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.gis_access_guard import project_access_guard
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
)
from openvietgis_api.adapters.persistence.spatial_fixtures import (
    get_reference_fixture,
    materialize_reference_fixture,
)
from openvietgis_api.adapters.persistence.spatial_repository import SqlSpatialRepository
from openvietgis_api.config import get_settings
from openvietgis_api.domains.audit.domain import AuditEvent
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.service import (
    AuthorizationDeniedError,
    TenantResourceNotFoundError,
)
from openvietgis_api.domains.spatial.domain import (
    CatalogChange,
    DataSource,
    Extent,
    ExtentState,
    Lifecycle,
    ReferenceFeature,
    SourceField,
    ValidationReason,
    ValidationState,
)
from openvietgis_api.domains.spatial.ports import (
    SpatialProjectReadOnlyError,
    SpatialRevisionConflictError,
)
from openvietgis_api.domains.spatial.service import derive_source_pins, verify_source_pins
from openvietgis_api.main import create_app
from openvietgis_api.security.csrf import make_csrf_token

ROOT = Path(__file__).resolve().parents[4]
CONTRACT = yaml.safe_load(
    (ROOT / "specs/003-spatial-catalog-crs/contracts/spatial-catalog-api.yaml").read_text(
        encoding="utf-8"
    )
)
ORG = UUID(int=1)
PROJECT = UUID(int=2)
HUMAN = UUID(int=3)
SERVICE = UUID(int=4)
CREDENTIAL = UUID(int=5)
SESSION = UUID(int=6)
UNKNOWN = UUID(int=7)
SESSION_TOKEN = "synthetic-contract-session"
BEARER = "synthetic-contract-service"
BASE = "/organizations/{organization_id}/projects/{project_id}/spatial/sources"
DETAIL = BASE + "/{source_id}"
RETIRE = DETAIL + "/retire"
READ = Permission.PROJECT_READ
EDIT = Permission.PROJECT_EDIT
MANAGE = Permission.PROJECT_MANAGE
VALIDATION_ERROR = {
    "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
}


def url(template: str = BASE, *, org=ORG, project=PROJECT, source_id=UNKNOWN) -> str:
    return "/api/v1" + template.format(organization_id=org, project_id=project, source_id=source_id)


def response_payload(response: Response, method: str, template: str, status: int) -> dict:
    """Validate the actual HTTP body against the frozen OpenAPI operation response."""
    assert response.status_code == status, response.text
    content = response.json()
    schema = CONTRACT["paths"][template][method.lower()]["responses"][str(status)]["content"][
        "application/json"
    ]["schema"]
    # Local OpenAPI schema references resolve at the OpenAPI document root.
    validator = Draft202012Validator(
        {
            **schema,
            "components": CONTRACT["components"],
        },
        format_checker=Draft202012Validator.FORMAT_CHECKER,
    )
    validator.validate(content)
    return content


def error(
    response: Response, method: str, template: str, status: int, code: str | None = None
) -> dict:
    body = response_payload(response, method, template, status)
    if code is not None:
        assert body["code"] == code
    return body


@dataclass
class CatalogRepositoryDouble:
    """In-memory transaction seam; scope permissions are controlled per request."""

    sources: dict[UUID, DataSource] = field(default_factory=dict)
    fields: dict[UUID, tuple[SourceField, ...]] = field(default_factory=dict)
    features: dict[UUID, tuple[ReferenceFeature, ...]] = field(default_factory=dict)
    changes: list[tuple[CatalogChange, AuditEvent]] = field(default_factory=list)
    actions: list[tuple[AuthorizationContext, Permission, bool]] = field(default_factory=list)
    lookups: list[UUID] = field(default_factory=list)
    active_layer_sources: set[UUID] = field(default_factory=set)
    human_permissions: set[Permission] = field(default_factory=lambda: {READ, EDIT, MANAGE})
    service_permissions: set[Permission] = field(default_factory=lambda: {READ, EDIT, MANAGE})
    archived: bool = False
    context: AuthorizationContext | None = None

    def prepare_scope(
        self, *, context: AuthorizationContext, permission: Permission, mutation: bool
    ) -> None:
        self.actions.append((context, permission, mutation))
        if (context.organization_id, context.project_id) != (ORG, PROJECT):
            raise TenantResourceNotFoundError("resource not found")
        permissions = (
            self.service_permissions
            if context.actor.actor_type == "service_principal"
            else self.human_permissions
        )
        if permission not in permissions:
            raise AuthorizationDeniedError("permission denied")
        if mutation and self.archived:
            raise SpatialProjectReadOnlyError("project is read-only")
        self.context = context

    def _assert_scope(self, context: AuthorizationContext) -> None:
        assert self.context == context, "repository accessed without current scoped authority"

    def get_source(self, *, context, source_id, lock=False) -> DataSource | None:
        self._assert_scope(context)
        self.lookups.append(source_id)
        return self.sources.get(source_id)

    def list_sources(self, *, context, after_id, limit) -> list[DataSource]:
        self._assert_scope(context)
        return [
            source
            for source_id, source in sorted(self.sources.items())
            if source.lifecycle == Lifecycle.ACTIVE and (after_id is None or source_id > after_id)
        ][:limit]

    def find_source_by_reference_key(self, *, context, reference_key) -> DataSource | None:
        self._assert_scope(context)
        return next(
            (source for source in self.sources.values() if source.reference_key == reference_key),
            None,
        )

    def get_fields(self, *, context, source_id) -> list[SourceField]:
        self._assert_scope(context)
        return sorted(self.fields[source_id], key=lambda item: item.ordinal)

    def prepare_reference_source(
        self, *, context, reference_key, source_id, name, now
    ) -> tuple[DataSource, tuple[SourceField, ...], tuple[ReferenceFeature, ...]]:
        self._assert_scope(context)
        fixture = get_reference_fixture(reference_key)
        if fixture is None:
            # These command exceptions are introduced by T022, after this red suite.
            from openvietgis_api.domains.spatial.ports import SpatialUnknownReferenceKeyError

            raise SpatialUnknownReferenceKeyError("unknown reference fixture")
        candidate = materialize_reference_fixture(
            reference_key,
            organization_id=context.organization_id,
            project_id=context.project_id,
            source_id=source_id,
            name=name,
            now=now,
        )
        # Native verification result is the only mocked spatial correctness boundary.
        # The frozen fixtures supply the expected facts; T019 exercises actual PostGIS.
        if fixture.expected_reason is None:
            extent = (
                Extent(*fixture.expected_native_bounds, fixture.stored_crs)
                if fixture.expected_native_bounds is not None
                else None
            )
            source = replace(
                candidate.source,
                validation_state=ValidationState.READY,
                validation_reason=None,
                extent_state=ExtentState.AVAILABLE if extent else ExtentState.EMPTY,
                extent=extent,
            )
        else:
            source = replace(
                candidate.source,
                validation_state=ValidationState.INVALID,
                validation_reason=ValidationReason(
                    fixture.expected_reason, "Fixture metadata could not be validated."
                ),
            )
        return source, candidate.fields, candidate.features

    def correct_reference_source(
        self, *, context, source: DataSource, declared_crs: int, now: datetime
    ) -> DataSource:
        self._assert_scope(context)
        fixture = get_reference_fixture(source.reference_key)
        if (
            fixture is None
            or fixture.correctable_to_crs != declared_crs
            or fixture.expected_native_bounds is None
        ):
            from openvietgis_api.domains.spatial.ports import SpatialCrsCorrectionUnverifiedError

            raise SpatialCrsCorrectionUnverifiedError("CRS correction could not be verified")
        return derive_source_pins(
            replace(
                source,
                source_crs=declared_crs,
                validation_state=ValidationState.READY,
                validation_reason=None,
                extent_state=ExtentState.AVAILABLE,
                extent=Extent(*fixture.expected_native_bounds, declared_crs),
                catalog_revision=source.catalog_revision + 1,
                updated_at=now,
            ),
            self.fields[source.id],
            self.features[source.id],
        ).source

    def insert_source(
        self,
        source: DataSource,
        fields: Sequence[SourceField],
        features: Sequence[ReferenceFeature],
        *,
        context,
    ) -> None:
        self._assert_scope(context)
        assert source.id not in self.sources
        assert (
            self.find_source_by_reference_key(context=context, reference_key=source.reference_key)
            is None
        )
        verify_source_pins(source, fields, features)
        self.sources[source.id] = source
        self.fields[source.id] = tuple(fields)
        self.features[source.id] = tuple(features)

    def update_source(self, source: DataSource, *, context, expected_revision: int) -> None:
        self._assert_scope(context)
        if self.sources[source.id].catalog_revision != expected_revision:
            raise SpatialRevisionConflictError("stale source catalog revision")
        pinned = derive_source_pins(source, self.fields[source.id], self.features[source.id])
        assert pinned.source.revision == source.revision
        self.sources[source.id] = source
        self.fields[source.id] = pinned.fields
        self.features[source.id] = pinned.features

    def has_active_layers(self, *, context, source_id) -> bool:
        self._assert_scope(context)
        return source_id in self.active_layer_sources

    def append_change(self, change: CatalogChange, audit: AuditEvent, *, context) -> None:
        self._assert_scope(context)
        assert change.audit_event_id == audit.id
        self.changes.append((change, audit))


@dataclass
class CatalogHTTP:
    client: TestClient
    repository: CatalogRepositoryDouble
    csrf: dict[str, str]

    def register(self, key: str, name: str = "Reference") -> dict:
        response = self.client.post(
            url(), json={"reference_key": key, "name": name}, headers=self.csrf
        )
        return response_payload(response, "POST", BASE, 201)

    def detail(self, source_id: str) -> dict:
        return response_payload(
            self.client.get(url(DETAIL, source_id=source_id)), "GET", DETAIL, 200
        )

    def permission(self, expected: Permission, mutation: bool) -> None:
        context, action, write = self.repository.actions[-1]
        assert (context.organization_id, context.project_id) == (ORG, PROJECT)
        assert (action, write) == (expected, mutation)


@pytest.fixture
def catalog(monkeypatch: pytest.MonkeyPatch) -> Iterator[CatalogHTTP]:
    repository = CatalogRepositoryDouble()

    # Patch the existing SQL adapter's public methods rather than the HTTP handler
    # or service. New port methods are not implemented yet during test-first run.
    for name in (
        "prepare_scope",
        "get_source",
        "list_sources",
        "find_source_by_reference_key",
        "get_fields",
        "prepare_reference_source",
        "correct_reference_source",
        "insert_source",
        "update_source",
        "has_active_layers",
        "append_change",
    ):
        method = getattr(repository, name)
        monkeypatch.setattr(
            SqlSpatialRepository,
            name,
            lambda _instance, *args, _method=method, **kwargs: _method(*args, **kwargs),
            raising=False,
        )

    def authenticate(request: Request) -> AuthenticatedRequest:
        # New request means a fresh transaction; stale bound context is invalid.
        repository.context = None
        if request.headers.get("authorization") == "Bearer " + BEARER:
            return AuthenticatedRequest(
                Actor("service_principal", SERVICE, "Contract service"),
                None,
                None,
                credential_id=CREDENTIAL,
                organization_id=ORG,
            )
        if request.cookies.get(get_settings().session_cookie_name) != SESSION_TOKEN:
            raise HTTPException(status_code=401, detail="authentication required")
        return AuthenticatedRequest(Actor("human", HUMAN, "Contract user"), SESSION, SESSION_TOKEN)

    def no_database() -> Iterator[Session]:
        # No bind: accidental SQL execution fails rather than contacting a database.
        with Session() as session:
            yield session

    app = create_app()
    app.dependency_overrides[authenticate_request] = authenticate
    app.dependency_overrides[get_db_session] = no_database
    # These tests exercise the real HTTP and domain permissions using fake
    # repositories. Restricted ACL/RLS is covered by PostGIS integration tests.
    app.dependency_overrides[project_access_guard] = lambda: None
    settings = get_settings()
    csrf = {
        "X-OpenVietGIS-CSRF": make_csrf_token(
            SESSION_TOKEN, settings.csrf_secret.get_secret_value()
        )
    }
    with TestClient(app) as client:
        client.cookies.set(settings.session_cookie_name, SESSION_TOKEN)
        yield CatalogHTTP(client, repository, csrf)


def test_register_and_inspect_projected_reference_source_contract(catalog: CatalogHTTP) -> None:
    created = catalog.register("reference_points_5899", "Quang Ninh controls")
    catalog.permission(EDIT, True)
    assert created["resource_type"] == "data_source"
    assert created["prior_revision"] is None
    assert created["new_revision"]["catalog_revision"] == 1
    assert created["lifecycle"] == "active"
    assert created["validation_state"] == "ready"
    assert created["validation_reason"] is None
    assert created["organization_id"] == str(ORG)
    assert created["project_id"] == str(PROJECT)
    change, audit = catalog.repository.changes[-1]
    assert str(change.id) == created["change_id"]
    assert audit.actor_id == HUMAN

    source = catalog.detail(created["resource_id"])
    catalog.permission(READ, False)
    assert source["id"] == created["resource_id"]
    assert source["name"] == "Quang Ninh controls"
    assert source["provenance"]["reference_key"] == "reference_points_5899"
    assert source["provenance"]["source_asset_id"] is None
    assert source["source_crs"]["code"] == source["stored_crs"]["code"] == 5899
    assert source["source_crs"]["native_axes"] == ["easting", "northing"]
    assert source["source_crs"]["coordinate_order"] == "xy"
    assert source["source_crs"]["unit"] == "metre"
    assert source["schema"]["geometry_family"] == "Point"
    assert source["schema"]["dimensions"] == "XY"
    assert source["schema"]["state"] == "known"
    assert source["schema"]["schema_revision"] == source["schema_revision"]
    assert [item["field_id"] for item in source["schema"]["fields"]] == [
        "fld_feature_code",
        "fld_name",
        "fld_rank",
        "fld_measure",
        "fld_active",
        "fld_observed_at",
        "fld_note",
    ]
    assert source["feature_count"] == 4
    assert source["feature_count_state"] == "known"
    assert source["extent_state"] == "available"
    assert source["extent"] == {
        "min_x": 437957.55,
        "min_y": 2291039.83,
        "max_x": 439365.92,
        "max_y": 2292185.83,
        "coordinate_reference": {
            "authority": "EPSG",
            "code": 5899,
            "coordinate_order": "xy",
            "x_axis": "easting",
            "y_axis": "northing",
            "unit": "metre",
        },
    }
    assert source["content_revision"] == created["new_revision"]["content_revision"]


def test_empty_and_invalid_registered_states_and_catalog_pagination(catalog: CatalogHTTP) -> None:
    empty = catalog.register("reference_empty_4326", "Empty")
    invalid = catalog.register("reference_unknown_crs", "Unknown CRS")
    assert invalid["validation_state"] == "invalid"
    assert invalid["validation_reason"]["code"] == "MISSING_CRS"
    empty_detail = catalog.detail(empty["resource_id"])
    assert empty_detail["feature_count_state"] == "known"
    assert empty_detail["feature_count"] == 0
    assert empty_detail["extent_state"] == "empty" and empty_detail["extent"] is None
    bad_detail = catalog.detail(invalid["resource_id"])
    assert bad_detail["source_crs"] is None and bad_detail["stored_crs"] is None
    assert bad_detail["extent_state"] == "unavailable" and bad_detail["extent"] is None

    first = response_payload(catalog.client.get(url(), params={"limit": 1}), "GET", BASE, 200)
    assert len(first["items"]) == 1
    assert first["next_after_id"] == first["items"][0]["id"]
    assert first["organization_id"] == str(ORG) and first["project_id"] == str(PROJECT)
    second = response_payload(
        catalog.client.get(url(), params={"limit": 1, "after_id": first["next_after_id"]}),
        "GET",
        BASE,
        200,
    )
    assert len(second["items"]) == 1 and second["next_after_id"] is None
    assert [first["items"][0]["id"], second["items"][0]["id"]] == sorted(
        [empty["resource_id"], invalid["resource_id"]]
    )


def test_rename_stale_revision_and_non_destructive_retirement(catalog: CatalogHTTP) -> None:
    created = catalog.register("reference_empty_4326", "Original")
    source_id = created["resource_id"]
    before = created["new_revision"]
    rename = response_payload(
        catalog.client.patch(
            url(DETAIL, source_id=source_id),
            json={"expected_revision": 1, "name": "  Renamed  "},
            headers=catalog.csrf,
        ),
        "PATCH",
        DETAIL,
        200,
    )
    catalog.permission(EDIT, True)
    assert rename["prior_revision"] == before
    assert rename["new_revision"] == {**before, "catalog_revision": 2}
    assert catalog.detail(source_id)["name"] == "Renamed"
    assert len(catalog.repository.changes) == 2

    error(
        catalog.client.patch(
            url(DETAIL, source_id=source_id),
            json={"expected_revision": 1, "name": "Stale"},
            headers=catalog.csrf,
        ),
        "PATCH",
        DETAIL,
        409,
        "STALE_CATALOG_REVISION",
    )
    error(
        catalog.client.patch(
            url(DETAIL, source_id=source_id),
            json={"expected_revision": 2, "name": "Renamed"},
            headers=catalog.csrf,
        ),
        "PATCH",
        DETAIL,
        422,
        "NO_CHANGES",
    )
    assert len(catalog.repository.changes) == 2
    catalog.repository.active_layer_sources.add(UUID(source_id))
    error(
        catalog.client.post(
            url(RETIRE, source_id=source_id),
            json={"expected_revision": 2},
            headers=catalog.csrf,
        ),
        "POST",
        RETIRE,
        409,
        "ACTIVE_LAYER_DEPENDENCIES",
    )
    catalog.permission(MANAGE, True)
    assert catalog.detail(source_id)["lifecycle"] == "active"
    catalog.repository.active_layer_sources.clear()
    retired = response_payload(
        catalog.client.post(
            url(RETIRE, source_id=source_id),
            json={"expected_revision": 2},
            headers=catalog.csrf,
        ),
        "POST",
        RETIRE,
        200,
    )
    assert retired["lifecycle"] == "retired"
    assert retired["prior_revision"] == rename["new_revision"]
    assert retired["new_revision"] == {**before, "catalog_revision": 3}
    assert catalog.repository.sources[UUID(source_id)].lifecycle == Lifecycle.RETIRED
    assert UUID(source_id) in catalog.repository.features
    error(catalog.client.get(url(DETAIL, source_id=source_id)), "GET", DETAIL, 404)
    assert response_payload(catalog.client.get(url()), "GET", BASE, 200)["items"] == []


def test_explicit_crs_correction_requires_verified_fixture_evidence(catalog: CatalogHTTP) -> None:
    created = catalog.register("reference_correctable_crs")
    source_id = created["resource_id"]
    assert created["validation_reason"]["code"] == "CRS_MISMATCH"
    before = catalog.detail(source_id)
    assert before["source_crs"]["code"] == 3857 and before["stored_crs"]["code"] == 4326
    old_geometry = catalog.repository.features[UUID(source_id)][0].geometry

    def correction(code: int) -> dict:
        return {
            "expected_revision": 1,
            "declared_crs": {"authority": "EPSG", "code": code, "coordinate_order": "xy"},
        }

    error(
        catalog.client.patch(
            url(DETAIL, source_id=source_id),
            json=correction(4756),
            headers=catalog.csrf,
        ),
        "PATCH",
        DETAIL,
        422,
        "CRS_CORRECTION_UNVERIFIED",
    )
    assert catalog.detail(source_id) == before
    assert len(catalog.repository.changes) == 1
    corrected = response_payload(
        catalog.client.patch(
            url(DETAIL, source_id=source_id),
            json=correction(4326),
            headers=catalog.csrf,
        ),
        "PATCH",
        DETAIL,
        200,
    )
    catalog.permission(EDIT, True)
    assert corrected["prior_revision"] == created["new_revision"]
    assert corrected["new_revision"]["catalog_revision"] == 2
    assert (
        corrected["new_revision"]["schema_revision"] == created["new_revision"]["schema_revision"]
    )
    assert (
        corrected["new_revision"]["content_revision"] != created["new_revision"]["content_revision"]
    )
    assert corrected["validation_state"] == "ready"
    after = catalog.detail(source_id)
    assert after["source_crs"]["code"] == after["stored_crs"]["code"] == 4326
    assert after["source_crs"]["native_axes"] == ["latitude", "longitude"]
    assert after["extent_state"] == "available"
    assert catalog.repository.features[UUID(source_id)][0].geometry == old_geometry


def test_unknown_and_duplicate_reference_key_are_explicit_safe_errors(catalog: CatalogHTTP) -> None:
    error(
        catalog.client.post(
            url(),
            json={"reference_key": "not_allowlisted", "name": "Unknown"},
            headers=catalog.csrf,
        ),
        "POST",
        BASE,
        422,
        "UNKNOWN_REFERENCE_KEY",
    )
    assert not catalog.repository.sources and not catalog.repository.changes
    created = catalog.register("reference_empty_4326")
    error(
        catalog.client.post(
            url(),
            json={"reference_key": "reference_empty_4326", "name": "Another"},
            headers=catalog.csrf,
        ),
        "POST",
        BASE,
        409,
        "SOURCE_ALREADY_REGISTERED",
    )
    response_payload(
        catalog.client.post(
            url(RETIRE, source_id=created["resource_id"]),
            json={"expected_revision": 1},
            headers=catalog.csrf,
        ),
        "POST",
        RETIRE,
        200,
    )
    error(
        catalog.client.post(
            url(),
            json={"reference_key": "reference_empty_4326", "name": "Retry"},
            headers=catalog.csrf,
        ),
        "POST",
        BASE,
        409,
        "SOURCE_ALREADY_REGISTERED",
    )
    assert len(catalog.repository.sources) == 1 and len(catalog.repository.changes) == 2


@pytest.mark.parametrize(
    ("template", "method", "payload"),
    [
        (BASE, "POST", {"name": "Missing key"}),
        (BASE, "POST", {"reference_key": "reference_empty_4326", "name": ""}),
        (BASE, "POST", {"reference_key": "reference_empty_4326", "name": " "}),
        (BASE, "POST", {"reference_key": "https://secret.invalid/path", "name": "URL"}),
        (
            BASE,
            "POST",
            {"reference_key": "reference_empty_4326", "name": "A", "dsn": "secret-marker"},
        ),
        (DETAIL, "PATCH", {"name": "Name"}),
        (DETAIL, "PATCH", {"expected_revision": 1}),
        (DETAIL, "PATCH", {"expected_revision": True, "name": "Name"}),
        (DETAIL, "PATCH", {"expected_revision": "1", "name": "Name"}),
        (DETAIL, "PATCH", {"expected_revision": 1, "name": None}),
        (DETAIL, "PATCH", {"expected_revision": 1, "stored_crs": 4326, "name": "Name"}),
        (
            DETAIL,
            "PATCH",
            {"expected_revision": 1, "name": "Name", "content_revision": "secret-marker"},
        ),
        (
            DETAIL,
            "PATCH",
            {
                "expected_revision": 1,
                "declared_crs": {
                    "authority": "EPSG",
                    "code": 4326,
                    "coordinate_order": "yx",
                },
            },
        ),
        (
            DETAIL,
            "PATCH",
            {
                "expected_revision": 1,
                "declared_crs": {
                    "authority": "EPSG",
                    "code": 4326,
                    "coordinate_order": "xy",
                    "proj": "secret-marker",
                },
            },
        ),
        (RETIRE, "POST", {"expected_revision": 0}),
        (RETIRE, "POST", {"expected_revision": 2**53}),
        (RETIRE, "POST", {"expected_revision": 1, "purge": True}),
    ],
)
def test_malformed_mutations_have_sanitized_422_and_no_repository_side_effect(
    catalog: CatalogHTTP, template: str, method: str, payload: dict
) -> None:
    response = catalog.client.request(method, url(template), json=payload, headers=catalog.csrf)
    assert error(response, method, template, 422) == VALIDATION_ERROR
    assert "secret-marker" not in response.text and "secret.invalid" not in response.text
    assert not catalog.repository.sources and not catalog.repository.changes


@pytest.mark.parametrize(
    "params", [{"limit": 0}, {"limit": 101}, {"limit": "x"}, {"after_id": "x"}]
)
def test_catalog_pagination_rejects_invalid_bounds(catalog: CatalogHTTP, params: dict) -> None:
    assert error(catalog.client.get(url(), params=params), "GET", BASE, 422) == VALIDATION_ERROR


def test_scope_and_permission_denial_precede_resource_lookup(catalog: CatalogHTTP) -> None:
    created = catalog.register("reference_empty_4326")
    source_id = created["resource_id"]
    foreign = error(
        catalog.client.get(url(DETAIL, project=UNKNOWN, source_id=source_id)), "GET", DETAIL, 404
    )
    missing = error(catalog.client.get(url(DETAIL)), "GET", DETAIL, 404)
    assert source_id not in str(foreign) and source_id not in str(missing)
    error(catalog.client.get(url(DETAIL, org=UNKNOWN, source_id=source_id)), "GET", DETAIL, 404)

    catalog.repository.human_permissions = {READ}
    catalog.repository.lookups.clear()
    error(
        catalog.client.patch(
            url(DETAIL, source_id=source_id),
            json={"expected_revision": 1, "name": "Denied"},
            headers=catalog.csrf,
        ),
        "PATCH",
        DETAIL,
        403,
    )
    catalog.permission(EDIT, True)
    assert catalog.repository.lookups == []
    error(
        catalog.client.post(
            url(RETIRE, source_id=source_id),
            json={"expected_revision": 1},
            headers=catalog.csrf,
        ),
        "POST",
        RETIRE,
        403,
    )
    catalog.permission(MANAGE, True)
    assert catalog.repository.lookups == []
    assert len(catalog.repository.changes) == 1

    catalog.repository.human_permissions = set()
    denied = error(catalog.client.get(url(DETAIL, source_id=source_id)), "GET", DETAIL, 403)
    absent = error(catalog.client.get(url(DETAIL)), "GET", DETAIL, 403)
    assert denied == absent and not catalog.repository.lookups


def test_archived_project_and_human_csrf_service_permission_ceiling(catalog: CatalogHTTP) -> None:
    created = catalog.register("reference_empty_4326")
    source_id = created["resource_id"]
    catalog.repository.archived = True
    assert catalog.detail(source_id)["id"] == source_id
    error(
        catalog.client.patch(
            url(DETAIL, source_id=source_id),
            json={"expected_revision": 1, "name": "Archived"},
            headers=catalog.csrf,
        ),
        "PATCH",
        DETAIL,
        409,
        "PROJECT_NOT_WRITABLE",
    )
    assert len(catalog.repository.changes) == 1
    catalog.repository.archived = False

    error(
        catalog.client.post(
            url(),
            json={"reference_key": "reference_unknown_crs", "name": "No CSRF"},
        ),
        "POST",
        BASE,
        403,
    )
    assert len(catalog.repository.changes) == 1

    headers = {"Authorization": "Bearer " + BEARER}
    catalog.repository.service_permissions = {EDIT}
    machine = response_payload(
        catalog.client.post(
            url(),
            json={"reference_key": "reference_unknown_crs", "name": "Machine"},
            headers=headers,
        ),
        "POST",
        BASE,
        201,
    )
    context, permission, write = catalog.repository.actions[-1]
    assert context.actor.actor_type == "service_principal"
    assert context.credential_id == CREDENTIAL and permission == EDIT and write
    assert machine["validation_state"] == "invalid"
    # Mutation response carries no independently unauthorized metadata detail.
    assert "schema" not in machine and "extent" not in machine and "provenance" not in machine
    error(
        catalog.client.get(url(DETAIL, source_id=machine["resource_id"]), headers=headers),
        "GET",
        DETAIL,
        403,
    )
    catalog.client.cookies.clear()
    error(catalog.client.get(url()), "GET", BASE, 401)
