"""US3 HTTP contract: pinned, scoped feature pages and stable detail.

The real ASGI adapter/service is exercised with the same transaction-bound
SQL port double as US1/US2. Native SQL consistency and RLS belong to T035.
"""

from __future__ import annotations

import json
from collections.abc import Iterator
from dataclasses import dataclass, field, replace
from uuid import UUID

import pytest
from fastapi import HTTPException, Request
from fastapi.testclient import TestClient
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.gis_access_guard import project_access_guard
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
)
from openvietgis_api.adapters.persistence.spatial_repository import SqlSpatialRepository
from openvietgis_api.config import get_settings
from openvietgis_api.domains.authorization.context import Actor
from openvietgis_api.domains.spatial.domain import (
    DataSource,
    ExtentState,
    Lifecycle,
    ReferenceFeature,
    ValidationReason,
    ValidationReasonCode,
    ValidationState,
)
from openvietgis_api.domains.spatial.service import derive_source_pins
from openvietgis_api.main import create_app
from openvietgis_api.security.csrf import make_csrf_token
from tests.contract.test_spatial_catalog import (
    BEARER,
    CREDENTIAL,
    EDIT,
    HUMAN,
    ORG,
    PROJECT,
    READ,
    SERVICE,
    SESSION,
    SESSION_TOKEN,
    UNKNOWN,
    VALIDATION_ERROR,
    error,
    response_payload,
)
from tests.contract.test_spatial_layers import LayerHTTP, LayerRepositoryDouble

BASE = "/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}/features"
DETAIL = BASE + "/{feature_id}"


def url(template: str = BASE, *, org=ORG, project=PROJECT, layer_id=UNKNOWN, feature_id="missing"):
    return "/api/v1" + template.format(
        organization_id=org, project_id=project, layer_id=layer_id, feature_id=feature_id
    )


@dataclass
class FeatureRepositoryDouble(LayerRepositoryDouble):
    page_calls: list[tuple[UUID, str, str, str | None, int]] = field(default_factory=list)
    detail_calls: list[tuple[UUID, str, str, str]] = field(default_factory=list)

    def transform_features(self, **kwargs):
        from openvietgis_api.domains.spatial.ports import SpatialTransformationUnavailableError

        raise SpatialTransformationUnavailableError("synthetic unavailable runtime")

    def _pinned_records(self, *, context, source_id, content_revision, schema_revision):
        self._assert_scope(context)
        source = self.sources[source_id]
        if (source.content_revision, source.schema_revision) != (content_revision, schema_revision):
            from openvietgis_api.domains.spatial.ports import SpatialStaleReadPinError

            raise SpatialStaleReadPinError("stale read pin")
        return sorted(self.features[source_id], key=lambda item: item.feature_id)

    def read_features(
        self, *, context, source_id, content_revision, schema_revision, after_feature_id, limit
    ) -> list[ReferenceFeature]:
        self.page_calls.append(
            (source_id, content_revision, schema_revision, after_feature_id, limit)
        )
        records = self._pinned_records(
            context=context,
            source_id=source_id,
            content_revision=content_revision,
            schema_revision=schema_revision,
        )
        assert 1 <= limit <= 100
        if after_feature_id is not None and not any(
            item.feature_id == after_feature_id for item in records
        ):
            from openvietgis_api.domains.spatial.ports import SpatialInvalidCursorError

            raise SpatialInvalidCursorError("invalid feature cursor")
        return [
            item
            for item in records
            if after_feature_id is None or item.feature_id > after_feature_id
        ][: limit + 1]

    def get_feature(
        self, *, context, source_id, feature_id, content_revision, schema_revision
    ) -> ReferenceFeature | None:
        self.detail_calls.append((source_id, feature_id, content_revision, schema_revision))
        return next(
            (
                item
                for item in self._pinned_records(
                    context=context,
                    source_id=source_id,
                    content_revision=content_revision,
                    schema_revision=schema_revision,
                )
                if item.feature_id == feature_id
            ),
            None,
        )


@dataclass
class FeatureHTTP(LayerHTTP):
    repository: FeatureRepositoryDouble

    def seed(self, key: str) -> tuple[str, DataSource, dict]:
        source_id = self.register(key)["resource_id"]
        layer_id = self.create(source_id)["resource_id"]
        source = self.repository.sources[UUID(source_id)]
        return (
            layer_id,
            source,
            {
                "content_revision": source.content_revision,
                "schema_revision": source.schema_revision,
            },
        )

    def page(self, layer_id: str, **params) -> dict:
        response = self.client.get(url(layer_id=layer_id), params=params)
        result = response_payload(response, "GET", BASE, 200)
        self.permission(READ, False)
        return result

    def feature(self, layer_id: str, feature_id: str, **params) -> dict:
        response = self.client.get(
            url(DETAIL, layer_id=layer_id, feature_id=feature_id), params=params
        )
        result = response_payload(response, "GET", DETAIL, 200)
        self.permission(READ, False)
        return result


@pytest.fixture
def feature_http(monkeypatch: pytest.MonkeyPatch) -> Iterator[FeatureHTTP]:
    repository = FeatureRepositoryDouble()
    for name in (
        "prepare_scope",
        "prepare_reference_source",
        "find_source_by_reference_key",
        "insert_source",
        "get_source",
        "get_fields",
        "append_change",
        "list_sources",
        "list_layers",
        "get_layer",
        "insert_layer",
        "read_features",
        "get_feature",
        "transform_features",
    ):
        method = getattr(repository, name)
        monkeypatch.setattr(
            SqlSpatialRepository,
            name,
            lambda _instance, *args, _method=method, **kwargs: _method(*args, **kwargs),
            raising=False,
        )

    def authenticate(request: Request) -> AuthenticatedRequest:
        repository.context = None
        if request.headers.get("authorization") == "Bearer " + BEARER:
            return AuthenticatedRequest(
                Actor("service_principal", SERVICE, "Feature contract service"),
                None,
                None,
                credential_id=CREDENTIAL,
                organization_id=ORG,
            )
        if request.cookies.get(get_settings().session_cookie_name) != SESSION_TOKEN:
            raise HTTPException(status_code=401, detail="authentication required")
        return AuthenticatedRequest(
            Actor("human", HUMAN, "Feature contract user"), SESSION, SESSION_TOKEN
        )

    def no_database() -> Iterator[Session]:
        with Session() as session:
            yield session

    monkeypatch.setattr(
        "openvietgis_api.adapters.http.spatial.filtered_layer_wire",
        lambda _session, _context, wire: wire,
    )
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
        yield FeatureHTTP(client, repository, csrf)


def coordinate_reference(code: int) -> dict:
    geographic = code in (4326, 4756)
    return {
        "authority": "EPSG",
        "code": code,
        "coordinate_order": "xy",
        "x_axis": "longitude" if geographic else "easting",
        "y_axis": "latitude" if geographic else "northing",
        "unit": "degree" if geographic else "metre",
    }


def expected_context(layer_id: str, source: DataSource, *, include_geometry=True) -> dict:
    assert source.stored_crs is not None
    return {
        "organization_id": str(ORG),
        "project_id": str(PROJECT),
        "layer_id": layer_id,
        "source_id": str(source.id),
        "content_revision": source.content_revision,
        "schema_revision": source.schema_revision,
        "geometry_included": include_geometry,
        "response_crs": coordinate_reference(source.stored_crs) if include_geometry else None,
        "transformation": None,
    }


def expected_feature(record: ReferenceFeature, *, include_geometry=True) -> dict:
    item = {
        "feature_id": record.feature_id,
        "attributes": dict(record.attributes),
        "geometry_state": record.geometry_state.value,
    }
    if include_geometry:
        assert record.geometry is not None and record.geometry.srid is not None
        item["geometry"] = {
            "type": record.geometry.type,
            "coordinates": json.loads(json.dumps(record.geometry.coordinates)),
            "coordinate_reference": coordinate_reference(record.geometry.srid),
        }
    return item


@pytest.mark.parametrize(
    "key",
    [
        "reference_points_5899",
        "reference_points_3857",
        "reference_points_4756",
        "reference_lines_4326",
        "reference_polygons_4326",
    ],
)
def test_native_and_identity_page_detail_exact_contract(
    feature_http: FeatureHTTP, key: str
) -> None:
    layer, source, pins = feature_http.seed(key)
    record = min(feature_http.repository.features[source.id], key=lambda item: item.feature_id)
    page = feature_http.page(layer, **pins, limit=1)
    assert page == {
        "context": expected_context(layer, source),
        "items": [expected_feature(record)],
        "next_after_feature_id": record.feature_id,
    }
    detail = feature_http.feature(layer, record.feature_id, **pins)
    assert detail == {"context": page["context"], "feature": page["items"][0]}
    assert (
        feature_http.feature(layer, record.feature_id, **pins, response_crs=source.stored_crs)
        == detail
    )
    assert feature_http.page(layer, **pins, limit=1, response_crs=source.stored_crs) == page
    assert feature_http.repository.page_calls[-1] == (source.id, *pins.values(), None, 1)
    assert feature_http.repository.detail_calls[-1] == (
        source.id,
        record.feature_id,
        *pins.values(),
    )


def test_limit_and_ascii_keyset_continuations_with_exact_typed_attributes(
    feature_http: FeatureHTTP,
) -> None:
    layer, source, pins = feature_http.seed("reference_points_4326")
    initial = feature_http.page(layer, **pins)
    assert len(initial["items"]) == 100
    assert initial["items"][0]["feature_id"] == "point-000001"
    assert initial["next_after_feature_id"] == "point-000100"
    assert feature_http.repository.page_calls[-1] == (source.id, *pins.values(), None, 100)
    records = sorted(feature_http.repository.features[source.id], key=lambda item: item.feature_id)
    assert initial["items"] == [expected_feature(record) for record in records[:100]]
    # Prevent Python bool/int equality or decimal coercion from hiding wire type regressions.
    for item, record in zip(initial["items"], records, strict=False):
        assert set(item["attributes"]) == set(record.attributes)
        for name, value in record.attributes.items():
            assert type(item["attributes"][name]) is type(value)
    assert initial["items"][0]["attributes"]["fld_measure"] == "0"
    assert initial["items"][0]["attributes"]["fld_note"] == ""
    assert initial["items"][4]["attributes"]["fld_note"] is None
    assert initial["items"][6]["attributes"]["fld_measure"] is None
    following = feature_http.page(layer, **pins, limit=100, after_feature_id="point-000100")
    assert following["context"] == initial["context"]
    assert following["items"] == [expected_feature(record) for record in records[100:200]]
    assert following["next_after_feature_id"] == "point-000200"
    terminal = feature_http.page(layer, **pins, limit=100, after_feature_id=records[-2].feature_id)
    assert terminal["items"] == [expected_feature(records[-1])]
    assert terminal["next_after_feature_id"] is None
    empty = feature_http.page(layer, **pins, after_feature_id=records[-1].feature_id)
    assert empty == {"context": initial["context"], "items": [], "next_after_feature_id": None}


def test_small_fixture_full_traversal_and_geometry_omission(feature_http: FeatureHTTP) -> None:
    layer, source, pins = feature_http.seed("reference_points_5899")
    records = sorted(feature_http.repository.features[source.id], key=lambda item: item.feature_id)
    expected = [expected_feature(record, include_geometry=False) for record in records]
    context = expected_context(layer, source, include_geometry=False)
    first = feature_http.page(layer, **pins, limit=2, include_geometry=False)
    assert first == {
        "context": context,
        "items": expected[:2],
        "next_after_feature_id": records[1].feature_id,
    }
    second = feature_http.page(
        layer,
        **pins,
        limit=2,
        include_geometry=False,
        after_feature_id=first["next_after_feature_id"],
    )
    assert second == {"context": context, "items": expected[2:], "next_after_feature_id": None}
    for item in first["items"] + second["items"]:
        assert "geometry" not in item
        assert feature_http.feature(layer, item["feature_id"], **pins, include_geometry=False) == {
            "context": context,
            "feature": item,
        }


def test_ready_empty_and_nonexistent_scoped_cursor_and_feature(feature_http: FeatureHTTP) -> None:
    layer, source, pins = feature_http.seed("reference_empty_4326")
    assert feature_http.page(layer, **pins) == {
        "context": expected_context(layer, source),
        "items": [],
        "next_after_feature_id": None,
    }
    missing = error(
        feature_http.client.get(url(DETAIL, layer_id=layer), params=pins), "GET", DETAIL, 404
    )
    # Even an ID known in a different source cannot serve as cursor or detail.
    other, other_source, _ = feature_http.seed("reference_points_5899")
    assert other != layer
    foreign_id = feature_http.repository.features[other_source.id][0].feature_id
    error(
        feature_http.client.get(
            url(layer_id=layer), params={**pins, "after_feature_id": foreign_id}
        ),
        "GET",
        BASE,
        422,
        "INVALID_CURSOR",
    )
    assert (
        error(
            feature_http.client.get(
                url(DETAIL, layer_id=layer, feature_id=foreign_id), params=pins
            ),
            "GET",
            DETAIL,
            404,
        )
        == missing
    )


@pytest.mark.parametrize("template", [BASE, DETAIL])
@pytest.mark.parametrize("pin_name", ["content_revision", "schema_revision"])
def test_one_stale_pin_is_conflict_even_for_missing_feature(
    feature_http: FeatureHTTP, template: str, pin_name: str
) -> None:
    layer, _, pins = feature_http.seed("reference_points_5899")
    stale = "0" * 64 if pins[pin_name] != "0" * 64 else "1" * 64
    error(
        feature_http.client.get(url(template, layer_id=layer), params={**pins, pin_name: stale}),
        "GET",
        template,
        409,
        "STALE_READ_PIN",
    )


@pytest.mark.parametrize("template", [BASE, DETAIL])
def test_unsupported_requests_fail_without_geometry_output(
    feature_http: FeatureHTTP, template: str
) -> None:
    layer, source, pins = feature_http.seed("reference_points_5899")
    feature_id = feature_http.repository.features[source.id][0].feature_id
    path = url(template, layer_id=layer, feature_id=feature_id)
    for code in (3405, 999999):
        error(
            feature_http.client.get(path, params={**pins, "response_crs": code}),
            "GET",
            template,
            422,
            "UNSUPPORTED_CRS",
        )
    error(
        feature_http.client.get(path, params={**pins, "response_crs": 3857}),
        "GET",
        template,
        422,
        "UNSUPPORTED_TRANSFORMATION",
    )
    assert (
        error(
            feature_http.client.get(
                path, params={**pins, "include_geometry": False, "response_crs": 5899}
            ),
            "GET",
            template,
            422,
        )
        == VALIDATION_ERROR
    )


@pytest.mark.parametrize("template", [BASE, DETAIL])
def test_permission_before_lookup_and_reauthorize_every_request(
    feature_http: FeatureHTTP, template: str
) -> None:
    layer, source, pins = feature_http.seed("reference_points_5899")
    repo = feature_http.repository
    feature_id = repo.features[source.id][0].feature_id
    path = url(template, layer_id=layer, feature_id=feature_id)
    response_payload(feature_http.client.get(path, params=pins), "GET", template, 200)
    feature_http.permission(READ, False)
    repo.human_permissions = {EDIT}
    repo.service_permissions = {EDIT}
    repo.layer_lookups.clear()
    repo.lookups.clear()
    repo.page_calls.clear()
    repo.detail_calls.clear()
    for headers in ({}, {"Authorization": "Bearer " + BEARER}):
        for layer_id in (layer, str(UNKNOWN)):
            for params in (pins, {**pins, "content_revision": "0" * 64, "response_crs": 3405}):
                error(
                    feature_http.client.get(
                        url(template, layer_id=layer_id, feature_id=feature_id),
                        params=params,
                        headers=headers,
                    ),
                    "GET",
                    template,
                    403,
                )
                feature_http.permission(READ, False)
    assert (
        not repo.layer_lookups
        and not repo.lookups
        and not repo.page_calls
        and not repo.detail_calls
    )
    repo.human_permissions = {READ}
    for scoped in (
        url(template, layer_id=layer, feature_id=feature_id, org=UNKNOWN),
        url(template, layer_id=layer, feature_id=feature_id, project=UNKNOWN),
    ):
        error(feature_http.client.get(scoped, params=pins), "GET", template, 404)
    assert (
        not repo.layer_lookups
        and not repo.lookups
        and not repo.page_calls
        and not repo.detail_calls
    )


@pytest.mark.parametrize("template", [BASE, DETAIL])
def test_retired_or_nonready_source_and_layer_fail_closed(
    feature_http: FeatureHTTP, template: str
) -> None:
    layer, source, pins = feature_http.seed("reference_points_5899")
    repo = feature_http.repository
    feature_id = repo.features[source.id][0].feature_id
    path = url(template, layer_id=layer, feature_id=feature_id)
    missing = error(
        feature_http.client.get(url(template, feature_id=feature_id), params=pins),
        "GET",
        template,
        404,
    )
    active_layer = repo.layers[UUID(layer)]
    repo.layers[UUID(layer)] = replace(
        active_layer, lifecycle=Lifecycle.RETIRED, retired_at=active_layer.updated_at
    )
    assert error(feature_http.client.get(path, params=pins), "GET", template, 404) == missing
    repo.layers[UUID(layer)] = active_layer
    repo.sources[source.id] = replace(
        source, lifecycle=Lifecycle.RETIRED, retired_at=source.updated_at
    )
    assert error(feature_http.client.get(path, params=pins), "GET", template, 404) == missing
    nonready = replace(
        source,
        validation_state=ValidationState.NOT_READY,
        validation_reason=ValidationReason(
            ValidationReasonCode.FIXTURE_UNAVAILABLE, "Fixture unavailable."
        ),
        extent_state=ExtentState.UNAVAILABLE,
        extent=None,
    )
    pinned = derive_source_pins(nonready, repo.fields[source.id], repo.features[source.id])
    repo.sources[source.id] = pinned.source
    repo.fields[source.id] = pinned.fields
    repo.features[source.id] = pinned.features
    error(
        feature_http.client.get(
            path,
            params={
                "content_revision": pinned.source.content_revision,
                "schema_revision": pinned.source.schema_revision,
            },
        ),
        "GET",
        template,
        409,
        "SOURCE_NOT_READY",
    )
    assert not repo.page_calls and not repo.detail_calls


@pytest.mark.parametrize("template", [BASE, DETAIL])
def test_invalid_request_errors_are_sanitized_and_do_not_read(
    feature_http: FeatureHTTP, template: str
) -> None:
    layer, source, pins = feature_http.seed("reference_points_5899")
    repo = feature_http.repository
    path = url(template, layer_id=layer, feature_id=repo.features[source.id][0].feature_id)
    invalid_params = [
        {},
        {"content_revision": pins["content_revision"]},
        {"schema_revision": pins["schema_revision"]},
        {**pins, "content_revision": "secret-marker"},
        {**pins, "schema_revision": "A" * 64},
        {**pins, "include_geometry": "secret-marker"},
        {**pins, "response_crs": "secret-marker"},
        {**pins, "response_crs": 0},
        {**pins, "response_crs": 2147483648},
    ]
    if template == BASE:
        invalid_params += [
            {**pins, "limit": 0},
            {**pins, "limit": 101},
            {**pins, "limit": "secret-marker"},
            {**pins, "after_feature_id": "secret-marker!"},
            {**pins, "after_feature_id": "x" * 129},
        ]
    for params in invalid_params:
        response = feature_http.client.get(path, params=params)
        assert error(response, "GET", template, 422) == VALIDATION_ERROR
        assert "secret-marker" not in response.text
    if template == DETAIL:
        response = feature_http.client.get(
            url(template, layer_id=layer, feature_id="secret-marker!"), params=pins
        )
        assert error(response, "GET", template, 422) == VALIDATION_ERROR
        assert "secret-marker" not in response.text
    assert not repo.page_calls and not repo.detail_calls
