"""US2 HTTP contract tests: Layer lifecycle, scoped access and truthful capabilities.

Exercise the ASGI adapter and application service against a transaction-bound
repository double. SQL locking, RLS and rollback belong to T028/T033 separately.
"""

from __future__ import annotations

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
    ExtentState,
    Layer,
    Lifecycle,
    ValidationReason,
    ValidationReasonCode,
    ValidationState,
)
from openvietgis_api.domains.spatial.ports import SpatialRevisionConflictError
from openvietgis_api.domains.spatial.service import derive_source_pins
from openvietgis_api.main import create_app
from openvietgis_api.security.csrf import make_csrf_token
from tests.contract.test_spatial_catalog import (
    BEARER,
    CREDENTIAL,
    EDIT,
    HUMAN,
    MANAGE,
    ORG,
    PROJECT,
    READ,
    SERVICE,
    SESSION,
    SESSION_TOKEN,
    UNKNOWN,
    VALIDATION_ERROR,
    CatalogHTTP,
    CatalogRepositoryDouble,
    error,
    response_payload,
)
from tests.contract.test_spatial_catalog import (
    DETAIL as SOURCE_DETAIL,
)
from tests.contract.test_spatial_catalog import (
    RETIRE as SOURCE_RETIRE,
)
from tests.contract.test_spatial_catalog import (
    url as source_url,
)

BASE = "/organizations/{organization_id}/projects/{project_id}/layers"
DETAIL = BASE + "/{layer_id}"
RETIRE = DETAIL + "/retire"

SUPPORTED_CAPABILITIES = {
    "metadata_read",
    "feature_read",
    "render",
    "style",
    "export",
    "query",
    "analysis",
    "offline",
}
UNSUPPORTED_CAPABILITIES = {"edit"}


def url(template: str = BASE, *, org=ORG, project=PROJECT, layer_id=UNKNOWN) -> str:
    return "/api/v1" + template.format(organization_id=org, project_id=project, layer_id=layer_id)


@dataclass
class LayerRepositoryDouble(CatalogRepositoryDouble):
    layers: dict[UUID, Layer] = field(default_factory=dict)
    layer_lookups: list[UUID] = field(default_factory=list)

    def get_layer(self, *, context, layer_id, lock=False) -> Layer | None:
        self._assert_scope(context)
        self.layer_lookups.append(layer_id)
        return self.layers.get(layer_id)

    def list_layers(self, *, context, after_id, limit) -> list[Layer]:
        self._assert_scope(context)
        return [
            layer
            for layer_id, layer in sorted(self.layers.items())
            if layer.lifecycle == Lifecycle.ACTIVE and (after_id is None or layer_id > after_id)
        ][:limit]

    def insert_layer(self, layer: Layer, *, context) -> None:
        self._assert_scope(context)
        assert layer.id not in self.layers
        assert (layer.organization_id, layer.project_id) == (ORG, PROJECT)
        layer.validate_source(self.sources[layer.source_id])
        self.layers[layer.id] = layer

    def update_layer(self, layer: Layer, *, context, expected_revision: int) -> None:
        self._assert_scope(context)
        prior = self.layers[layer.id]
        if prior.layer_revision != expected_revision or prior.lifecycle != Lifecycle.ACTIVE:
            raise SpatialRevisionConflictError("stale layer revision")
        assert layer.layer_revision == expected_revision + 1
        assert (prior.source_id, prior.created_at) == (layer.source_id, layer.created_at)
        self.layers[layer.id] = layer

    def has_active_layers(self, *, context, source_id) -> bool:
        self._assert_scope(context)
        return any(
            layer.source_id == source_id and layer.lifecycle == Lifecycle.ACTIVE
            for layer in self.layers.values()
        )


@dataclass
class LayerHTTP(CatalogHTTP):
    repository: LayerRepositoryDouble

    def create(self, source_id: str, name: str = "Layer") -> dict:
        return response_payload(
            self.client.post(url(), json={"source_id": source_id, "name": name}, headers=self.csrf),
            "POST",
            BASE,
            201,
        )

    def layer(self, layer_id: str) -> dict:
        return response_payload(self.client.get(url(DETAIL, layer_id=layer_id)), "GET", DETAIL, 200)

    def layer_list(self, **params) -> dict:
        return response_payload(self.client.get(url(), params=params), "GET", BASE, 200)


@pytest.fixture
def layers(monkeypatch: pytest.MonkeyPatch) -> Iterator[LayerHTTP]:
    repository = LayerRepositoryDouble()
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
        "get_layer",
        "list_layers",
        "insert_layer",
        "update_layer",
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
                Actor("service_principal", SERVICE, "Layer contract service"),
                None,
                None,
                credential_id=CREDENTIAL,
                organization_id=ORG,
            )
        if request.cookies.get(get_settings().session_cookie_name) != SESSION_TOKEN:
            raise HTTPException(status_code=401, detail="authentication required")
        return AuthenticatedRequest(
            Actor("human", HUMAN, "Layer contract user"), SESSION, SESSION_TOKEN
        )

    def no_database() -> Iterator[Session]:
        # A stray SQL command must fail, not silently reach a real database.
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
        yield LayerHTTP(client, repository, csrf)


def assert_capabilities(layer: dict, *, feature_available: bool) -> None:
    by_name = {item["name"]: item for item in layer["capabilities"]}
    assert len(layer["capabilities"]) == len(by_name) == 9
    assert set(by_name) == SUPPORTED_CAPABILITIES | UNSUPPORTED_CAPABILITIES
    for name in SUPPORTED_CAPABILITIES:
        assert by_name[name] == {
            "name": name,
            "technical_supported": True,
            "available": feature_available
            if name in {"feature_read", "render", "style", "export", "query", "analysis", "offline"}
            else True,
            "required_permission": "project.edit" if name == "analysis" else "project.read",
            "actor_authorized": True,
        }
    for name in UNSUPPORTED_CAPABILITIES:
        assert by_name[name] == {
            "name": name,
            "technical_supported": False,
            "available": False,
            "required_permission": None,
            "actor_authorized": False,
        }


def test_create_list_detail_share_current_named_source_and_explicit_capabilities(
    layers: LayerHTTP,
) -> None:
    source_id = layers.register("reference_points_5899", "Source display")["resource_id"]
    first = layers.create(source_id, "  Same display  ")
    layers.permission(EDIT, True)
    second = layers.create(source_id, "Same display")
    assert first["resource_id"] != second["resource_id"]
    for created in (first, second):
        assert created["organization_id"] == str(ORG)
        assert created["project_id"] == str(PROJECT)
        assert created["resource_type"] == "layer"
        assert created["prior_revision"] is None
        assert created["new_revision"] == {"layer_revision": 1}
        assert created["lifecycle"] == "active"
        layer = layers.layer(created["resource_id"])
        layers.permission(READ, False)
        assert layer["id"] == created["resource_id"]
        assert layer["organization_id"] == str(ORG) and layer["project_id"] == str(PROJECT)
        assert layer["source_id"] == source_id
        assert layer["name"] == "Same display"
        assert layer["source"] == layers.detail(source_id)
        assert layer["source"]["name"] == "Source display"
        assert layer["layer_revision"] == 1 and layer["retired_at"] is None
        assert layer["lifecycle"] == "active"
        assert_capabilities(layer, feature_available=True)

    page1 = layers.layer_list(limit=1)
    layers.permission(READ, False)
    assert page1["organization_id"] == str(ORG) and page1["project_id"] == str(PROJECT)
    assert len(page1["items"]) == 1 and page1["next_after_id"] == page1["items"][0]["id"]
    page2 = layers.layer_list(limit=1, after_id=page1["next_after_id"])
    assert len(page2["items"]) == 1 and page2["next_after_id"] is None
    expected = sorted((first["resource_id"], second["resource_id"]))
    assert [page1["items"][0]["id"], page2["items"][0]["id"]] == expected
    assert page1["items"][0] == layers.layer(expected[0])
    assert page2["items"][0] == layers.layer(expected[1])
    assert layers.layer_list(after_id=expected[-1])["items"] == []
    assert layers.layer_list(after_id=expected[-1])["next_after_id"] is None

    # Nested source must be current independently of the Layer revision/name.
    response_payload(
        layers.client.patch(
            source_url(SOURCE_DETAIL, source_id=source_id),
            json={"name": "Renamed source", "expected_revision": 1},
            headers=layers.csrf,
        ),
        "PATCH",
        SOURCE_DETAIL,
        200,
    )
    updated_source = layers.detail(source_id)
    assert updated_source["name"] == "Renamed source"
    for item in layers.layer_list()["items"]:
        assert item["source"] == updated_source and item["layer_revision"] == 1
        assert item["name"] == "Same display"
    assert len(layers.repository.layers) == 2
    assert len(layers.repository.sources) == len(layers.repository.features) == 1


def test_feature_unavailable_preserves_read_authority_and_metadata_support(
    layers: LayerHTTP,
) -> None:
    source_id = layers.register("reference_points_5899")["resource_id"]
    layer_id = layers.create(source_id)["resource_id"]
    key = UUID(source_id)
    source = layers.repository.sources[key]
    # Simulate a subsequent independent native availability assessment at the
    # port seam, not a user mutation endpoint or arbitrary source update.
    nonready = replace(
        source,
        validation_state=ValidationState.NOT_READY,
        validation_reason=ValidationReason(
            ValidationReasonCode.FIXTURE_UNAVAILABLE, "Fixture native assessment is unavailable."
        ),
        extent_state=ExtentState.UNAVAILABLE,
        extent=None,
    )
    pinned = derive_source_pins(
        nonready, layers.repository.fields[key], layers.repository.features[key]
    )
    layers.repository.sources[key] = pinned.source
    layers.repository.fields[key] = pinned.fields
    layers.repository.features[key] = pinned.features

    item = layers.layer(layer_id)
    assert item["source"]["validation_state"] == "not_ready"
    assert_capabilities(item, feature_available=False)
    assert layers.layer_list()["items"] == [item]


def test_rename_retire_stale_noop_and_dependency_contract(layers: LayerHTTP) -> None:
    source_id = layers.register("reference_empty_4326", "Original source")["resource_id"]
    first_id = layers.create(source_id, "Original")["resource_id"]
    sibling_id = layers.create(source_id, "Sibling")["resource_id"]
    initial_source = layers.detail(source_id)
    initial_sibling = layers.layer(sibling_id)

    renamed = response_payload(
        layers.client.patch(
            url(DETAIL, layer_id=first_id),
            json={"name": "  Renamed  ", "expected_revision": 1},
            headers=layers.csrf,
        ),
        "PATCH",
        DETAIL,
        200,
    )
    layers.permission(EDIT, True)
    assert renamed["prior_revision"] == {"layer_revision": 1}
    assert renamed["new_revision"] == {"layer_revision": 2}
    assert layers.layer(first_id)["name"] == "Renamed"
    assert layers.detail(source_id) == initial_source

    for revision, status, code in ((1, 409, "STALE_LAYER_REVISION"), (2, 422, "NO_CHANGES")):
        error(
            layers.client.patch(
                url(DETAIL, layer_id=first_id),
                json={"name": "Renamed", "expected_revision": revision},
                headers=layers.csrf,
            ),
            "PATCH",
            DETAIL,
            status,
            code,
        )
    error(
        layers.client.post(
            url(RETIRE, layer_id=first_id),
            json={"expected_revision": 1},
            headers=layers.csrf,
        ),
        "POST",
        RETIRE,
        409,
        "STALE_LAYER_REVISION",
    )
    assert len(layers.repository.changes) == 4

    # Crucially Layer retirement needs PROJECT_EDIT, not PROJECT_MANAGE.
    layers.repository.human_permissions = {READ, EDIT}
    retired = response_payload(
        layers.client.post(
            url(RETIRE, layer_id=first_id),
            json={"expected_revision": 2},
            headers=layers.csrf,
        ),
        "POST",
        RETIRE,
        200,
    )
    layers.permission(EDIT, True)
    assert retired["prior_revision"] == {"layer_revision": 2}
    assert retired["new_revision"] == {"layer_revision": 3} and retired["lifecycle"] == "retired"
    assert layers.repository.layers[UUID(first_id)].retired_at is not None
    assert layers.layer(sibling_id) == initial_sibling
    assert layers.detail(source_id) == initial_source
    assert layers.layer_list()["items"] == [initial_sibling]
    error(layers.client.get(url(DETAIL, layer_id=first_id)), "GET", DETAIL, 404)

    error(
        layers.client.patch(
            url(DETAIL, layer_id=first_id),
            json={"name": "No restore", "expected_revision": 3},
            headers=layers.csrf,
        ),
        "PATCH",
        DETAIL,
        404,
    )
    error(
        layers.client.post(
            url(RETIRE, layer_id=first_id),
            json={"expected_revision": 3},
            headers=layers.csrf,
        ),
        "POST",
        RETIRE,
        404,
    )
    layers.repository.human_permissions = {READ, EDIT, MANAGE}
    error(
        layers.client.post(
            source_url(SOURCE_RETIRE, source_id=source_id),
            json={"expected_revision": 1},
            headers=layers.csrf,
        ),
        "POST",
        SOURCE_RETIRE,
        409,
        "ACTIVE_LAYER_DEPENDENCIES",
    )
    assert len(layers.repository.changes) == 5
    change, audit = layers.repository.changes[-1]
    assert str(change.id) == retired["change_id"]
    assert str(change.resource_id) == str(change.layer_id) == first_id
    assert change.source_id is None
    assert audit.actor_id == HUMAN


def test_create_must_resolve_active_ready_source_and_hide_absent_or_retired_ids(
    layers: LayerHTTP,
) -> None:
    invalid_id = layers.register("reference_unknown_crs")["resource_id"]
    retired_id = layers.register("reference_empty_4326")["resource_id"]
    response_payload(
        layers.client.post(
            source_url(SOURCE_RETIRE, source_id=retired_id),
            json={"expected_revision": 1},
            headers=layers.csrf,
        ),
        "POST",
        SOURCE_RETIRE,
        200,
    )
    missing = error(
        layers.client.post(
            url(), json={"source_id": str(UNKNOWN), "name": "Missing"}, headers=layers.csrf
        ),
        "POST",
        BASE,
        404,
    )
    retired = error(
        layers.client.post(
            url(), json={"source_id": retired_id, "name": "Retired"}, headers=layers.csrf
        ),
        "POST",
        BASE,
        404,
    )
    assert missing == retired
    error(
        layers.client.post(
            url(), json={"source_id": invalid_id, "name": "Unknown CRS"}, headers=layers.csrf
        ),
        "POST",
        BASE,
        409,
        "SOURCE_NOT_READY",
    )
    assert not layers.repository.layers and len(layers.repository.changes) == 3


@pytest.mark.parametrize(
    ("template", "method", "payload"),
    [
        (BASE, "POST", {"name": "Missing source"}),
        (BASE, "POST", {"source_id": "secret-marker", "name": "Invalid source"}),
        (BASE, "POST", {"source_id": str(UNKNOWN), "name": ""}),
        (BASE, "POST", {"source_id": str(UNKNOWN), "name": " "}),
        (BASE, "POST", {"source_id": str(UNKNOWN), "name": None}),
        (BASE, "POST", {"source_id": str(UNKNOWN), "name": "a" * 201}),
        (BASE, "POST", {"source_id": str(UNKNOWN), "name": "NUL\x00"}),
        (BASE, "POST", {"source_id": str(UNKNOWN), "name": "A", "dsn": "secret-marker"}),
        (DETAIL, "PATCH", {"name": "Missing revision"}),
        (DETAIL, "PATCH", {"expected_revision": 1}),
        (DETAIL, "PATCH", {"expected_revision": True, "name": "A"}),
        (DETAIL, "PATCH", {"expected_revision": "1", "name": "A"}),
        (DETAIL, "PATCH", {"expected_revision": 1, "name": None}),
        (DETAIL, "PATCH", {"expected_revision": 1, "name": " "}),
        (DETAIL, "PATCH", {"expected_revision": 1, "name": "A", "source_id": str(UNKNOWN)}),
        (RETIRE, "POST", {"expected_revision": 0}),
        (RETIRE, "POST", {"expected_revision": 2**53}),
        (RETIRE, "POST", {"expected_revision": 1, "purge": True}),
    ],
)
def test_invalid_layer_contract_payloads_are_sanitized_and_side_effect_free(
    layers: LayerHTTP, template: str, method: str, payload: dict
) -> None:
    response = layers.client.request(method, url(template), json=payload, headers=layers.csrf)
    assert error(response, method, template, 422) == VALIDATION_ERROR
    assert "secret-marker" not in response.text
    assert not layers.repository.layers and not layers.repository.changes


@pytest.mark.parametrize(
    "params", [{"limit": 0}, {"limit": 101}, {"limit": "x"}, {"after_id": "x"}]
)
def test_invalid_layer_pagination_contract(layers: LayerHTTP, params: dict) -> None:
    assert error(layers.client.get(url(), params=params), "GET", BASE, 422) == VALIDATION_ERROR


def test_layer_permission_missing_and_wrong_scope_hide_existence_before_lookup(
    layers: LayerHTTP,
) -> None:
    source_id = layers.register("reference_empty_4326")["resource_id"]
    layer_id = layers.create(source_id)["resource_id"]
    for path in (
        url(DETAIL, layer_id=layer_id, org=UNKNOWN),
        url(DETAIL, layer_id=layer_id, project=UNKNOWN),
    ):
        body = error(layers.client.get(path), "GET", DETAIL, 404)
        assert layer_id not in str(body)
    error(layers.client.get(url(DETAIL)), "GET", DETAIL, 404)

    layers.repository.human_permissions = {READ}
    layers.repository.lookups.clear()
    layers.repository.layer_lookups.clear()
    error(
        layers.client.post(
            url(), json={"source_id": source_id, "name": "Denied"}, headers=layers.csrf
        ),
        "POST",
        BASE,
        403,
    )
    layers.permission(EDIT, True)
    for template, method, data in (
        (DETAIL, "PATCH", {"name": "Denied", "expected_revision": 1}),
        (RETIRE, "POST", {"expected_revision": 1}),
    ):
        error(
            layers.client.request(
                method, url(template, layer_id=layer_id), json=data, headers=layers.csrf
            ),
            method,
            template,
            403,
        )
        layers.permission(EDIT, True)
    assert not layers.repository.lookups and not layers.repository.layer_lookups

    layers.repository.human_permissions.clear()
    existing = error(layers.client.get(url(DETAIL, layer_id=layer_id)), "GET", DETAIL, 403)
    missing = error(layers.client.get(url(DETAIL)), "GET", DETAIL, 403)
    assert existing == missing
    error(layers.client.get(url()), "GET", BASE, 403)
    assert not layers.repository.lookups and not layers.repository.layer_lookups
    assert len(layers.repository.changes) == 2


def test_archived_layers_and_current_actor_csrf_service_ceiling(layers: LayerHTTP) -> None:
    source_id = layers.register("reference_empty_4326")["resource_id"]
    layer_id = layers.create(source_id)["resource_id"]
    layers.repository.archived = True
    assert layers.layer_list()["items"] == [layers.layer(layer_id)]
    error(
        layers.client.post(
            url(), json={"source_id": source_id, "name": "Archived"}, headers=layers.csrf
        ),
        "POST",
        BASE,
        409,
        "PROJECT_NOT_WRITABLE",
    )
    error(
        layers.client.patch(
            url(DETAIL, layer_id=layer_id),
            json={"name": "Archived", "expected_revision": 1},
            headers=layers.csrf,
        ),
        "PATCH",
        DETAIL,
        409,
        "PROJECT_NOT_WRITABLE",
    )
    error(
        layers.client.post(
            url(RETIRE, layer_id=layer_id), json={"expected_revision": 1}, headers=layers.csrf
        ),
        "POST",
        RETIRE,
        409,
        "PROJECT_NOT_WRITABLE",
    )
    layers.repository.human_permissions = {READ}
    error(
        layers.client.post(
            url(), json={"source_id": source_id, "name": "No permission"}, headers=layers.csrf
        ),
        "POST",
        BASE,
        403,
    )
    layers.repository.archived = False
    layers.repository.human_permissions = {READ, EDIT, MANAGE}
    error(
        layers.client.post(url(), json={"source_id": source_id, "name": "No CSRF"}),
        "POST",
        BASE,
        403,
    )
    error(
        layers.client.patch(
            url(DETAIL, layer_id=layer_id), json={"name": "No CSRF", "expected_revision": 1}
        ),
        "PATCH",
        DETAIL,
        403,
    )
    error(
        layers.client.post(url(RETIRE, layer_id=layer_id), json={"expected_revision": 1}),
        "POST",
        RETIRE,
        403,
    )
    assert len(layers.repository.changes) == 2

    # Bearer with EDIT but not READ may mutate; strict response schema contains
    # only identity/revision/lifecycle/evidence, never the nested source/detail.
    bearer = {"Authorization": "Bearer " + BEARER}
    layers.repository.service_permissions = {EDIT}
    created = response_payload(
        layers.client.post(url(), json={"source_id": source_id, "name": "Machine"}, headers=bearer),
        "POST",
        BASE,
        201,
    )
    context, permission, write = layers.repository.actions[-1]
    assert context.actor.actor_type == "service_principal" and context.credential_id == CREDENTIAL
    assert permission == EDIT and write
    error(
        layers.client.get(url(DETAIL, layer_id=created["resource_id"]), headers=bearer),
        "GET",
        DETAIL,
        403,
    )
    error(layers.client.get(url(), headers=bearer), "GET", BASE, 403)

    layers.repository.service_permissions = {READ}
    item = response_payload(
        layers.client.get(url(DETAIL, layer_id=created["resource_id"]), headers=bearer),
        "GET",
        DETAIL,
        200,
    )
    assert_capabilities(item, feature_available=True)
    layers.client.cookies.clear()
    error(layers.client.get(url()), "GET", BASE, 401)
