"""R003 revision and audit evidence primitives (no transaction commit)."""

from __future__ import annotations

import re
from collections.abc import Sequence
from dataclasses import dataclass, replace
from datetime import UTC, datetime
from typing import NotRequired, TypedDict
from uuid import UUID, uuid4

from openvietgis_api.domains.audit.domain import AuditEvent, AuditOutcome
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.service import TenantResourceNotFoundError
from openvietgis_api.domains.spatial.crs import (
    WEB_MERCATOR_MAX_LATITUDE,
    CoordinateReferenceWire,
    TransformationProfileWire,
    get_coordinate_reference,
    get_transformation_profile,
)
from openvietgis_api.domains.spatial.domain import (
    MAX_REVISION,
    SUPPORTED_FAMILIES,
    AttributeValue,
    CatalogChange,
    CatalogOperation,
    ChangedValue,
    ChangeValue,
    CoordinateTree,
    DataSource,
    GeometryDimensions,
    GeometryState,
    Layer,
    Lifecycle,
    ReferenceFeature,
    ResourceType,
    SourceField,
    SourceKind,
    SpatialValidationError,
    ValidationState,
    canonical_attribute_value,
    content_revision_digest,
    schema_revision_digest,
    validate_ready_geometry,
    validate_revision_token,
    validate_source_contents,
)
from openvietgis_api.domains.spatial.ports import (
    SpatialActiveLayerDependenciesError,
    SpatialFeatureRepository,
    SpatialInvalidFeatureRequestError,
    SpatialLayerRepository,
    SpatialLayerRevisionConflictError,
    SpatialNoChangesError,
    SpatialRepository,
    SpatialRevisionConflictError,
    SpatialScopeError,
    SpatialSourceAlreadyRegisteredError,
    SpatialSourceNotReadyError,
    SpatialSourceRepository,
    SpatialStaleReadPinError,
    SpatialUnsupportedCRSError,
    SpatialUnsupportedTransformationError,
)
from openvietgis_api.domains.tenancy.domain import ActorType


@dataclass(frozen=True, slots=True)
class PinnedSource:
    source: DataSource
    fields: tuple[SourceField, ...]
    features: tuple[ReferenceFeature, ...]


def derive_source_pins(
    source: DataSource,
    fields: Sequence[SourceField],
    features: Sequence[ReferenceFeature],
) -> PinnedSource:
    """Compute both manifest hashes server-side and replace provisional pins.

    The digest inputs include geometry interpretation and explicit schema state;
    scope/attribute/availability invariants are checked on the resulting records.
    Never accept user-supplied revision tokens on registration or correction.
    """
    schema = schema_revision_digest(
        fields, source.geometry_family, source.dimensions, schema_state=source.schema_state
    )
    content = content_revision_digest(
        features,
        fields,
        source.stored_crs,
        source.source_crs,
        geometry_family=source.geometry_family,
        dimensions=source.dimensions,
    )
    result = PinnedSource(
        replace(source, schema_revision=schema, content_revision=content),
        tuple(replace(field, schema_revision=schema) for field in fields),
        tuple(
            replace(feature, schema_revision=schema, content_revision=content)
            for feature in features
        ),
    )
    validate_source_contents(result.source, result.fields, result.features)
    return result


def verify_source_pins(
    source: DataSource,
    fields: Sequence[SourceField],
    features: Sequence[ReferenceFeature],
) -> None:
    """Check both actual content and assigned tokens before materialization."""
    validate_source_contents(source, fields, features)
    expected = derive_source_pins(source, fields, features)
    if expected.source.revision != source.revision:
        raise SpatialValidationError("source revision is not its canonical manifest")


def next_revision(current: int, expected: int) -> int:
    if type(expected) is not int or expected != current:
        raise SpatialRevisionConflictError("stale catalog revision")
    if not 1 <= current < MAX_REVISION:
        raise SpatialValidationError("catalog revision exhausted")
    return current + 1


@dataclass(frozen=True, slots=True)
class MutationEvidence:
    change: CatalogChange
    audit: AuditEvent


def _source_snapshot(source: DataSource) -> dict[str, ChangeValue]:
    reason = source.validation_reason
    return {
        "name": source.name,
        "kind": source.kind.value,
        "reference_key": source.reference_key,
        "fixture_version": source.fixture_version,
        "manifest_digest": source.manifest_digest,
        "source_asset_id": None,
        "lifecycle": source.lifecycle.value,
        "validation_state": source.validation_state.value,
        "validation_reason.code": reason.code.value if reason else None,
        "validation_reason.message": reason.message if reason else None,
        "source_crs": source.source_crs,
        "stored_crs": source.stored_crs,
        "schema_state": source.schema_state.value,
        "geometry_family": source.geometry_family.value,
        "dimensions": source.dimensions.value,
        "feature_count_state": source.feature_count_state.value,
        "feature_count": source.feature_count,
        "extent_state": source.extent_state.value,
        "catalog_revision": source.catalog_revision,
        "content_revision": source.content_revision,
        "schema_revision": source.schema_revision,
    }


def _layer_snapshot(layer: Layer) -> dict[str, ChangeValue]:
    return {
        "name": layer.name,
        "source_id": str(layer.source_id),
        "lifecycle": layer.lifecycle.value,
        "layer_revision": layer.layer_revision,
    }


def _diff(
    prior: dict[str, ChangeValue], current: dict[str, ChangeValue]
) -> tuple[ChangedValue, ...]:
    return tuple(
        ChangedValue(field, prior.get(field), value)
        for field, value in sorted(current.items())
        if type(prior.get(field)) is not type(value) or prior.get(field) != value
    )


def build_mutation_evidence(
    *,
    context: AuthorizationContext,
    prior: DataSource | Layer | None,
    current: DataSource | Layer,
    operation: CatalogOperation,
    now: datetime,
) -> MutationEvidence:
    """Derive actual safe scalar changes from immutable domain objects."""
    actor = context.actor
    if (
        actor.actor_id is None
        or actor.actor_type not in ("human", "service_principal")
        or (context.organization_id, context.project_id)
        != (current.organization_id, current.project_id)
    ):
        raise SpatialScopeError("catalog mutation scope mismatch")
    if prior is not None and (
        type(prior) is not type(current)
        or (prior.id, prior.organization_id, prior.project_id)
        != (current.id, current.organization_id, current.project_id)
        or prior.lifecycle != Lifecycle.ACTIVE
        or prior.created_at != current.created_at
    ):
        raise SpatialValidationError(
            "catalog mutation changes immutable identity or retired resource"
        )
    if current.updated_at != now:
        raise SpatialValidationError("catalog mutation timestamp mismatch")
    source_id: UUID | None
    layer_id: UUID | None
    if isinstance(current, DataSource):
        if prior is not None and not isinstance(prior, DataSource):
            raise SpatialValidationError("source mutation type mismatch")
        current_values = _source_snapshot(current)
        prior_values = _source_snapshot(prior) if prior is not None else {}
        resource_type = ResourceType.DATA_SOURCE
        source_id, layer_id = current.id, None
        expected_operation = CatalogOperation.REGISTER_SOURCE
        mutable_fields = {
            CatalogOperation.UPDATE_SOURCE: {"name", "catalog_revision"},
            CatalogOperation.UPDATE_SCHEMA: {
                "schema_revision",
                "content_revision",
                "catalog_revision",
            },
            CatalogOperation.CORRECT_SOURCE_CRS: {
                "name",
                "source_crs",
                "validation_state",
                "validation_reason.code",
                "validation_reason.message",
                "extent_state",
                "content_revision",
                "catalog_revision",
            },
            CatalogOperation.RETIRE_SOURCE: {"lifecycle", "catalog_revision"},
            CatalogOperation.REFRESH_REFERENCE_FIXTURE: {
                "validation_state",
                "validation_reason.code",
                "validation_reason.message",
                "schema_state",
                "geometry_family",
                "dimensions",
                "feature_count_state",
                "feature_count",
                "extent_state",
                "content_revision",
                "schema_revision",
                "catalog_revision",
            },
        }
        revision_field = "catalog_revision"
        if (
            prior is not None
            and operation in (CatalogOperation.UPDATE_SOURCE, CatalogOperation.RETIRE_SOURCE)
            and prior.extent != current.extent
        ):
            raise SpatialValidationError("metadata-only mutation cannot change native extent")
        if prior is not None and (prior.stored_crs, prior.manifest_digest) != (
            current.stored_crs,
            current.manifest_digest,
        ):
            raise SpatialValidationError("native CRS or manifest identity cannot be rewritten")
    else:
        if prior is not None and not isinstance(prior, Layer):
            raise SpatialValidationError("layer mutation type mismatch")
        current_values = _layer_snapshot(current)
        prior_values = _layer_snapshot(prior) if prior is not None else {}
        resource_type = ResourceType.LAYER
        source_id, layer_id = None, current.id
        expected_operation = CatalogOperation.CREATE_LAYER
        mutable_fields = {
            CatalogOperation.RENAME_LAYER: {"name", "layer_revision"},
            CatalogOperation.RETIRE_LAYER: {"lifecycle", "layer_revision"},
        }
        revision_field = "layer_revision"

    changes = _diff(prior_values, current_values)
    changed = {change.field for change in changes}
    if prior is None:
        if operation != expected_operation or current.lifecycle != Lifecycle.ACTIVE:
            raise SpatialValidationError("invalid initial catalog operation")
    else:
        allowed = mutable_fields.get(operation)
        if allowed is None or not changed <= allowed or not (changed - {revision_field}):
            raise SpatialValidationError("invalid catalog transition or no material change")
        if revision_field not in changed:
            raise SpatialValidationError("catalog transition did not advance revision")
        retiring = operation in (CatalogOperation.RETIRE_SOURCE, CatalogOperation.RETIRE_LAYER)
        if retiring != (current.lifecycle == Lifecycle.RETIRED):
            raise SpatialValidationError("catalog retirement operation mismatch")
        if operation == CatalogOperation.CORRECT_SOURCE_CRS and (
            "source_crs" not in changed or "content_revision" not in changed
        ):
            raise SpatialValidationError("coordinate correction must change interpretation and pin")
        if operation == CatalogOperation.REFRESH_REFERENCE_FIXTURE and not (
            changed & {"content_revision", "schema_revision"}
        ):
            raise SpatialValidationError("controlled refresh must change a readable revision")
    change = CatalogChange(
        id=uuid4(),
        organization_id=current.organization_id,
        project_id=current.project_id,
        resource_type=resource_type,
        resource_id=current.id,
        source_id=source_id,
        layer_id=layer_id,
        actor_type=ActorType(actor.actor_type),
        actor_id=actor.actor_id,
        actor_display_snapshot=actor.display_name,
        operation=operation,
        created_at=now,
        audit_event_id=uuid4(),
        prior_revision=prior.revision if prior is not None else None,
        new_revision=current.revision,
        changes=changes,
    )
    audit = AuditEvent(
        id=change.audit_event_id,
        organization_id=change.organization_id,
        actor_type=change.actor_type,
        actor_id=change.actor_id,
        actor_display_snapshot=change.actor_display_snapshot,
        action=f"spatial.catalog.{change.operation.value}",
        resource_type=change.resource_type.value,
        resource_id=change.resource_id,
        outcome=AuditOutcome.SUCCESS,
        metadata={"project_id": str(change.project_id), "catalog_change_id": str(change.id)},
        created_at=change.created_at,
    )
    return MutationEvidence(change, audit)


@dataclass(frozen=True, slots=True)
class SourceDetail:
    source: DataSource
    fields: tuple[SourceField, ...]


@dataclass(frozen=True, slots=True)
class SourceCatalogPage:
    items: tuple[SourceDetail, ...]
    next_after_id: UUID | None


@dataclass(frozen=True, slots=True)
class SourceMutationResult:
    source: DataSource
    change: CatalogChange


def _source_name(name: str) -> str:
    if not isinstance(name, str):
        raise SpatialValidationError("source name must be text")
    cleaned = name.strip()
    if not 1 <= len(cleaned) <= 200 or "\x00" in cleaned:
        raise SpatialValidationError("source name outside bounds")
    try:
        cleaned.encode("utf-8")
    except UnicodeError:
        raise SpatialValidationError("source name has invalid encoding") from None
    return cleaned


@dataclass(slots=True)
class SpatialCatalogService:
    """One authorized request transaction; no independent commit or storage locator."""

    repository: SpatialSourceRepository

    def _active(self, *, context: AuthorizationContext, source_id: UUID) -> DataSource:
        source = self.repository.get_source(context=context, source_id=source_id, lock=True)
        if source is None or source.lifecycle != Lifecycle.ACTIVE:
            raise TenantResourceNotFoundError("resource not found")
        return source

    def _detail(self, *, context: AuthorizationContext, source: DataSource) -> SourceDetail:
        fields = tuple(self.repository.get_fields(context=context, source_id=source.id))
        if any(
            (f.organization_id, f.project_id, f.source_id, f.schema_revision)
            != (source.organization_id, source.project_id, source.id, source.schema_revision)
            for f in fields
        ):
            raise SpatialValidationError("catalog schema scope or pin disagrees")
        if (
            schema_revision_digest(
                fields,
                source.geometry_family,
                source.dimensions,
                schema_state=source.schema_state,
            )
            != source.schema_revision
        ):
            raise SpatialValidationError("catalog schema revision disagrees")
        return SourceDetail(source, fields)

    def list_sources(
        self,
        *,
        context: AuthorizationContext,
        after_id: UUID | None,
        limit: int,
    ) -> SourceCatalogPage:
        self.repository.prepare_scope(
            context=context,
            permission=Permission.PROJECT_READ,
            mutation=False,
        )
        if type(limit) is not int or not 1 <= limit <= 100:
            raise SpatialValidationError("catalog limit out of bounds")
        sources = self.repository.list_sources(context=context, after_id=after_id, limit=limit + 1)
        visible = sources[:limit]
        items = tuple(self._detail(context=context, source=source) for source in visible)
        return SourceCatalogPage(items, visible[-1].id if len(sources) > limit else None)

    def get_source(self, *, context: AuthorizationContext, source_id: UUID) -> SourceDetail:
        self.repository.prepare_scope(
            context=context,
            permission=Permission.PROJECT_READ,
            mutation=False,
        )
        return self._detail(
            context=context, source=self._active(context=context, source_id=source_id)
        )

    def register_source(
        self,
        *,
        context: AuthorizationContext,
        reference_key: str,
        name: str,
    ) -> SourceMutationResult:
        self.repository.prepare_scope(
            context=context,
            permission=Permission.PROJECT_EDIT,
            mutation=True,
        )
        clean_name = _source_name(name)
        if (
            self.repository.find_source_by_reference_key(
                context=context, reference_key=reference_key
            )
            is not None
        ):
            raise SpatialSourceAlreadyRegisteredError("reference fixture already registered")
        now = datetime.now(UTC)
        source, fields, features = self.repository.prepare_reference_source(
            context=context,
            reference_key=reference_key,
            source_id=uuid4(),
            name=clean_name,
            now=now,
        )
        evidence = build_mutation_evidence(
            context=context,
            prior=None,
            current=source,
            operation=CatalogOperation.REGISTER_SOURCE,
            now=now,
        )
        self.repository.insert_source(source, fields, features, context=context)
        self.repository.append_change(evidence.change, evidence.audit, context=context)
        return SourceMutationResult(source, evidence.change)

    def update_source(
        self,
        *,
        context: AuthorizationContext,
        source_id: UUID,
        expected_revision: int,
        name: str | None = None,
        declared_crs: int | None = None,
    ) -> SourceMutationResult:
        self.repository.prepare_scope(
            context=context,
            permission=Permission.PROJECT_EDIT,
            mutation=True,
        )
        prior = self._active(context=context, source_id=source_id)
        revision = next_revision(prior.catalog_revision, expected_revision)
        clean_name = _source_name(name) if name is not None else prior.name
        now = datetime.now(UTC)
        if declared_crs is not None and (
            declared_crs != prior.source_crs or prior.validation_state != ValidationState.READY
        ):
            current = self.repository.correct_reference_source(
                context=context,
                source=prior,
                declared_crs=declared_crs,
                now=now,
            )
            current = replace(current, name=clean_name)
            operation = CatalogOperation.CORRECT_SOURCE_CRS
        else:
            if clean_name == prior.name:
                raise SpatialNoChangesError("source metadata did not change")
            current = replace(prior, name=clean_name, catalog_revision=revision, updated_at=now)
            operation = CatalogOperation.UPDATE_SOURCE
        if current.catalog_revision != revision:
            raise SpatialValidationError("source validation returned unexpected revision")
        evidence = build_mutation_evidence(
            context=context,
            prior=prior,
            current=current,
            operation=operation,
            now=now,
        )
        self.repository.update_source(current, context=context, expected_revision=expected_revision)
        self.repository.append_change(evidence.change, evidence.audit, context=context)
        return SourceMutationResult(current, evidence.change)

    def retire_source(
        self,
        *,
        context: AuthorizationContext,
        source_id: UUID,
        expected_revision: int,
    ) -> SourceMutationResult:
        self.repository.prepare_scope(
            context=context,
            permission=Permission.PROJECT_MANAGE,
            mutation=True,
        )
        prior = self._active(context=context, source_id=source_id)
        revision = next_revision(prior.catalog_revision, expected_revision)
        if self.repository.has_active_layers(context=context, source_id=source_id):
            raise SpatialActiveLayerDependenciesError(
                "Retire active Layers before retiring the source"
            )
        now = datetime.now(UTC)
        current = replace(
            prior,
            lifecycle=Lifecycle.RETIRED,
            retired_at=now,
            updated_at=now,
            catalog_revision=revision,
        )
        evidence = build_mutation_evidence(
            context=context,
            prior=prior,
            current=current,
            operation=CatalogOperation.RETIRE_SOURCE,
            now=now,
        )
        self.repository.update_source(current, context=context, expected_revision=expected_revision)
        self.repository.append_change(evidence.change, evidence.audit, context=context)
        return SourceMutationResult(current, evidence.change)


@dataclass(frozen=True, slots=True)
class LayerCapability:
    name: str
    technical_supported: bool
    available: bool
    required_permission: str | None
    actor_authorized: bool


@dataclass(frozen=True, slots=True)
class LayerDetail:
    layer: Layer
    source: SourceDetail
    capabilities: tuple[LayerCapability, ...]


@dataclass(frozen=True, slots=True)
class LayerCatalogPage:
    items: tuple[LayerDetail, ...]
    next_after_id: UUID | None


@dataclass(frozen=True, slots=True)
class LayerMutationResult:
    layer: Layer
    change: CatalogChange


def source_render_supported(source: DataSource) -> bool:
    """Pure finite technical profile; actual reads recheck provenance and vertices."""
    # R004's finite presentation support is separate from current authority.
    # Readable native4326 at latitude86 remains outside the display profile.
    native = source.stored_crs
    provenance_compatible = (
        source.kind == SourceKind.IMPORTED_FILE
        or native not in (4756, 5899)
        or source.reference_key
        in {
            "reference_points_4756",
            "reference_points_4756_hvs",
            "reference_points_5899",
        }
    )
    geographic_extent_compatible = (
        native != 4326
        or source.feature_count == 0
        or (
            source.extent is not None
            and -WEB_MERCATOR_MAX_LATITUDE
            <= source.extent.min_y
            <= source.extent.max_y
            <= WEB_MERCATOR_MAX_LATITUDE
        )
    )
    return (
        source.kind
        in (
            SourceKind.REFERENCE_FIXTURE,
            SourceKind.IMPORTED_FILE,
            SourceKind.DERIVED_PROCESSING,
            SourceKind.MANUAL,
        )
        and source.geometry_family in SUPPORTED_FAMILIES
        and source.dimensions == GeometryDimensions.XY
        and source.feature_count is not None
        and source.feature_count <= 10_000
        and (
            native == 4326
            or (native is not None and get_transformation_profile(native, 4326) is not None)
        )
        and provenance_compatible
        and geographic_extent_compatible
    )


def layer_capabilities(
    layer: Layer, source: DataSource, *, actor_authorized: bool
) -> tuple[LayerCapability, ...]:
    """Support, availability and caller authority are independent facts."""
    layer.validate_source(source)
    active = layer.lifecycle == Lifecycle.ACTIVE
    ready = (
        active
        and source.lifecycle == Lifecycle.ACTIVE
        and (source.validation_state == ValidationState.READY)
    )
    render_supported = source_render_supported(source)
    return (
        LayerCapability(
            "metadata_read", True, active, Permission.PROJECT_READ.value, actor_authorized
        ),
        LayerCapability(
            "feature_read", True, ready, Permission.PROJECT_READ.value, actor_authorized
        ),
        *(
            LayerCapability(
                name,
                render_supported,
                ready and render_supported,
                Permission.PROJECT_READ.value if render_supported else None,
                actor_authorized if render_supported else False,
            )
            for name in ("render", "style", "export", "query")
        ),
        LayerCapability(
            "edit",
            render_supported and source.kind != SourceKind.REFERENCE_FIXTURE,
            ready and render_supported and source.kind != SourceKind.REFERENCE_FIXTURE,
            Permission.PROJECT_EDIT.value
            if (render_supported and source.kind != SourceKind.REFERENCE_FIXTURE)
            else None,
            actor_authorized
            if (render_supported and source.kind != SourceKind.REFERENCE_FIXTURE)
            else False,
        ),
        LayerCapability(
            "analysis",
            render_supported,
            ready and render_supported,
            Permission.PROJECT_EDIT.value if render_supported else None,
            actor_authorized if render_supported else False,
        ),
        LayerCapability(
            "offline",
            render_supported,
            ready and render_supported,
            Permission.PROJECT_READ.value if render_supported else None,
            actor_authorized if render_supported else False,
        ),
    )


def _next_layer_revision(current: int, expected: int) -> int:
    try:
        return next_revision(current, expected)
    except SpatialRevisionConflictError as exc:
        raise SpatialLayerRevisionConflictError("stale layer revision") from exc


@dataclass(slots=True)
class SpatialLayerService:
    """Layer identity remains separate from shared source storage and revisions."""

    repository: SpatialLayerRepository

    def _active(self, *, context: AuthorizationContext, layer_id: UUID) -> Layer:
        layer = self.repository.get_layer(context=context, layer_id=layer_id, lock=True)
        if layer is None or layer.lifecycle != Lifecycle.ACTIVE:
            raise TenantResourceNotFoundError("resource not found")
        return layer

    def _detail(
        self, *, context: AuthorizationContext, layer: Layer, cache: dict[UUID, SourceDetail]
    ) -> LayerDetail:
        detail = cache.get(layer.source_id)
        if detail is None:
            catalog = SpatialCatalogService(self.repository)
            source = catalog._active(context=context, source_id=layer.source_id)
            detail = catalog._detail(context=context, source=source)
            cache[layer.source_id] = detail
        # prepare_scope for a read succeeded; capabilities are not an authorization gate.
        return LayerDetail(
            layer, detail, layer_capabilities(layer, detail.source, actor_authorized=True)
        )

    def list_layers(
        self, *, context: AuthorizationContext, after_id: UUID | None, limit: int
    ) -> LayerCatalogPage:
        self.repository.prepare_scope(
            context=context, permission=Permission.PROJECT_READ, mutation=False
        )
        if type(limit) is not int or not 1 <= limit <= 100:
            raise SpatialValidationError("catalog limit out of bounds")
        layers = self.repository.list_layers(context=context, after_id=after_id, limit=limit + 1)
        visible = layers[:limit]
        cache: dict[UUID, SourceDetail] = {}
        items = tuple(self._detail(context=context, layer=layer, cache=cache) for layer in visible)
        return LayerCatalogPage(items, visible[-1].id if len(layers) > limit else None)

    def get_layer(self, *, context: AuthorizationContext, layer_id: UUID) -> LayerDetail:
        self.repository.prepare_scope(
            context=context, permission=Permission.PROJECT_READ, mutation=False
        )
        return self._detail(
            context=context, layer=self._active(context=context, layer_id=layer_id), cache={}
        )

    def create_layer(
        self, *, context: AuthorizationContext, source_id: UUID, name: str
    ) -> LayerMutationResult:
        self.repository.prepare_scope(
            context=context, permission=Permission.PROJECT_EDIT, mutation=True
        )
        source = SpatialCatalogService(self.repository)._active(
            context=context, source_id=source_id
        )
        if source.validation_state != ValidationState.READY:
            raise SpatialSourceNotReadyError("source not ready")
        now = datetime.now(UTC)
        layer = Layer(
            id=uuid4(),
            organization_id=source.organization_id,
            project_id=source.project_id,
            source_id=source.id,
            name=_source_name(name),
            lifecycle=Lifecycle.ACTIVE,
            layer_revision=1,
            created_at=now,
            updated_at=now,
            retired_at=None,
        )
        evidence = build_mutation_evidence(
            context=context,
            prior=None,
            current=layer,
            operation=CatalogOperation.CREATE_LAYER,
            now=now,
        )
        self.repository.insert_layer(layer, context=context)
        self.repository.append_change(evidence.change, evidence.audit, context=context)
        return LayerMutationResult(layer, evidence.change)

    def rename_layer(
        self, *, context: AuthorizationContext, layer_id: UUID, expected_revision: int, name: str
    ) -> LayerMutationResult:
        self.repository.prepare_scope(
            context=context, permission=Permission.PROJECT_EDIT, mutation=True
        )
        prior = self._active(context=context, layer_id=layer_id)
        revision = _next_layer_revision(prior.layer_revision, expected_revision)
        clean_name = _source_name(name)
        if prior.name == clean_name:
            raise SpatialNoChangesError("layer metadata did not change")
        now = datetime.now(UTC)
        current = replace(prior, name=clean_name, layer_revision=revision, updated_at=now)
        return self._persist(
            context=context,
            prior=prior,
            current=current,
            operation=CatalogOperation.RENAME_LAYER,
            expected_revision=expected_revision,
            now=now,
        )

    def retire_layer(
        self, *, context: AuthorizationContext, layer_id: UUID, expected_revision: int
    ) -> LayerMutationResult:
        self.repository.prepare_scope(
            context=context, permission=Permission.PROJECT_EDIT, mutation=True
        )
        prior = self._active(context=context, layer_id=layer_id)
        revision = _next_layer_revision(prior.layer_revision, expected_revision)
        now = datetime.now(UTC)
        current = replace(
            prior,
            lifecycle=Lifecycle.RETIRED,
            layer_revision=revision,
            updated_at=now,
            retired_at=now,
        )
        return self._persist(
            context=context,
            prior=prior,
            current=current,
            operation=CatalogOperation.RETIRE_LAYER,
            expected_revision=expected_revision,
            now=now,
        )

    def _persist(
        self,
        *,
        context: AuthorizationContext,
        prior: Layer,
        current: Layer,
        operation: CatalogOperation,
        expected_revision: int,
        now: datetime,
    ) -> LayerMutationResult:
        evidence = build_mutation_evidence(
            context=context, prior=prior, current=current, operation=operation, now=now
        )
        self.repository.update_layer(current, context=context, expected_revision=expected_revision)
        self.repository.append_change(evidence.change, evidence.audit, context=context)
        return LayerMutationResult(current, evidence.change)


class FeatureGeometryWire(TypedDict):
    type: str
    coordinates: CoordinateTree
    coordinate_reference: CoordinateReferenceWire


class FeatureWire(TypedDict):
    feature_id: str
    attributes: dict[str, AttributeValue]
    geometry_state: str
    geometry: NotRequired[FeatureGeometryWire]


class FeatureReadContextWire(TypedDict):
    organization_id: UUID
    project_id: UUID
    layer_id: UUID
    source_id: UUID
    content_revision: str
    schema_revision: str
    geometry_included: bool
    response_crs: CoordinateReferenceWire | None
    transformation: TransformationProfileWire | None


class FeaturePageWire(TypedDict):
    context: FeatureReadContextWire
    items: list[FeatureWire]
    next_after_feature_id: str | None


class FeatureDetailWire(TypedDict):
    context: FeatureReadContextWire
    feature: FeatureWire


def serialize_feature(
    source: DataSource,
    fields: Sequence[SourceField],
    feature: ReferenceFeature,
    *,
    include_geometry: bool,
) -> FeatureWire:
    """Emit exact logical attributes and an explicit native XY geometry envelope."""
    if (
        not source.readable
        or (feature.organization_id, feature.project_id, feature.source_id)
        != (source.organization_id, source.project_id, source.id)
        or (feature.content_revision, feature.schema_revision)
        != (source.content_revision, source.schema_revision)
    ):
        raise SpatialValidationError("feature scope or pin disagrees")
    if set(feature.attributes) != {field.field_id for field in fields}:
        raise SpatialValidationError("logical attributes disagree with schema")
    attributes: dict[str, AttributeValue] = {}
    for field in fields:
        if (field.organization_id, field.project_id, field.source_id, field.schema_revision) != (
            source.organization_id,
            source.project_id,
            source.id,
            source.schema_revision,
        ):
            raise SpatialValidationError("logical schema scope or pin disagrees")
        raw = feature.attributes[field.field_id]
        value = canonical_attribute_value(field, raw)
        if type(value) is not type(raw) or value != raw:
            raise SpatialValidationError("noncanonical logical attribute")
        attributes[field.field_id] = value
    geometry = feature.geometry
    if feature.geometry_state != GeometryState.PRESENT or geometry is None:
        raise SpatialValidationError("ready feature requires present geometry")
    validate_ready_geometry(geometry, source.geometry_family, source.stored_crs)
    wire: FeatureWire = {
        "feature_id": feature.feature_id,
        "attributes": attributes,
        "geometry_state": feature.geometry_state.value,
    }
    if include_geometry:
        if source.stored_crs is None:
            raise SpatialValidationError("ready source requires native CRS")
        reference = get_coordinate_reference(source.stored_crs)
        if reference is None:
            raise SpatialValidationError("ready source requires readable native CRS")
        wire["geometry"] = {
            "type": geometry.type,
            "coordinates": geometry.coordinates,
            "coordinate_reference": reference.to_wire(),
        }
    return wire


def validate_feature_id(value: str) -> None:
    if not isinstance(value, str) or re.fullmatch(r"[A-Za-z0-9_-]{1,128}", value) is None:
        raise SpatialValidationError("invalid stable feature ID")


@dataclass(slots=True)
class SpatialFeatureService:
    """Native pinned reads; the transaction remains owned by the request adapter."""

    repository: SpatialFeatureRepository

    def _snapshot(
        self,
        *,
        context: AuthorizationContext,
        layer_id: UUID,
        content_revision: str,
        schema_revision: str,
        include_geometry: bool,
        response_crs: int | None,
    ) -> tuple[SourceDetail, FeatureReadContextWire]:
        self.repository.prepare_scope(
            context=context, permission=Permission.PROJECT_READ, mutation=False
        )
        layer = SpatialLayerService(self.repository)._active(context=context, layer_id=layer_id)
        catalog = SpatialCatalogService(self.repository)
        source = catalog._active(context=context, source_id=layer.source_id)
        if not source.readable:
            raise SpatialSourceNotReadyError("source not ready")
        validate_revision_token(content_revision)
        validate_revision_token(schema_revision)
        if (content_revision, schema_revision) != (source.content_revision, source.schema_revision):
            raise SpatialStaleReadPinError("read pins are stale; reload the current catalog")
        if type(include_geometry) is not bool:
            raise SpatialValidationError("invalid geometry inclusion")
        reference = None
        transformation = None
        if not include_geometry:
            if response_crs is not None:
                raise SpatialInvalidFeatureRequestError("response CRS requires geometry")
        else:
            native = source.stored_crs
            if native is None:
                raise SpatialValidationError("ready source requires native CRS")
            target = native if response_crs is None else response_crs
            reference = get_coordinate_reference(target)
            if reference is None:
                raise SpatialUnsupportedCRSError("requested CRS is not readable")
            if target != native:
                operation = get_transformation_profile(native, target)
                if operation is None:
                    raise SpatialUnsupportedTransformationError("no approved direct operation")
                transformation = operation.to_wire()
        detail = catalog._detail(context=context, source=source)
        wire: FeatureReadContextWire = {
            "organization_id": source.organization_id,
            "project_id": source.project_id,
            "layer_id": layer.id,
            "source_id": source.id,
            "content_revision": source.content_revision,
            "schema_revision": source.schema_revision,
            "geometry_included": include_geometry,
            "response_crs": reference.to_wire() if reference is not None else None,
            "transformation": transformation,
        }
        return detail, wire

    def _serialize_response(
        self,
        *,
        context: AuthorizationContext,
        detail: SourceDetail,
        wire: FeatureReadContextWire,
        features: Sequence[ReferenceFeature],
    ) -> list[FeatureWire]:
        result = [
            serialize_feature(
                detail.source,
                detail.fields,
                feature,
                include_geometry=wire["geometry_included"],
            )
            for feature in features
        ]
        if wire["transformation"] is None:
            return result
        reference = wire["response_crs"]
        if reference is None or not wire["geometry_included"]:
            raise SpatialValidationError("transform response requires coordinate reference")
        geometries = self.repository.transform_features(
            context=context,
            source=detail.source,
            features=features,
            target_code=reference["code"],
        )
        if len(geometries) != len(result):
            raise SpatialValidationError("transform response count disagrees")
        for item, geometry in zip(result, geometries, strict=True):
            validate_ready_geometry(
                geometry,
                detail.source.geometry_family,
                reference["code"],
                output_roundoff=True,
            )
            item["geometry"] = {
                "type": geometry.type,
                "coordinates": geometry.coordinates,
                "coordinate_reference": reference,
            }
        return result

    def read_features(
        self,
        *,
        context: AuthorizationContext,
        layer_id: UUID,
        content_revision: str,
        schema_revision: str,
        include_geometry: bool = True,
        response_crs: int | None = None,
        after_feature_id: str | None = None,
        limit: int = 100,
    ) -> FeaturePageWire:
        detail, wire = self._snapshot(
            context=context,
            layer_id=layer_id,
            content_revision=content_revision,
            schema_revision=schema_revision,
            include_geometry=include_geometry,
            response_crs=response_crs,
        )
        if type(limit) is not int or not 1 <= limit <= 100:
            raise SpatialValidationError("feature page limit out of bounds")
        if after_feature_id is not None:
            validate_feature_id(after_feature_id)
        rows = self.repository.read_features(
            context=context,
            source_id=detail.source.id,
            content_revision=content_revision,
            schema_revision=schema_revision,
            after_feature_id=after_feature_id,
            limit=limit,
        )
        if len(rows) > limit + 1:
            raise SpatialValidationError("unbounded feature page")
        previous = after_feature_id
        items: list[FeatureWire] = []
        for row in rows:
            if previous is not None and row.feature_id <= previous:
                raise SpatialValidationError("feature page is not strictly ordered")
            previous = row.feature_id
        items = self._serialize_response(
            context=context,
            detail=detail,
            wire=wire,
            features=rows,
        )[:limit]
        return {
            "context": wire,
            "items": items,
            "next_after_feature_id": items[-1]["feature_id"] if len(rows) > limit else None,
        }

    def get_feature(
        self,
        *,
        context: AuthorizationContext,
        layer_id: UUID,
        feature_id: str,
        content_revision: str,
        schema_revision: str,
        include_geometry: bool = True,
        response_crs: int | None = None,
    ) -> FeatureDetailWire:
        detail, wire = self._snapshot(
            context=context,
            layer_id=layer_id,
            content_revision=content_revision,
            schema_revision=schema_revision,
            include_geometry=include_geometry,
            response_crs=response_crs,
        )
        validate_feature_id(feature_id)
        feature = self.repository.get_feature(
            context=context,
            source_id=detail.source.id,
            feature_id=feature_id,
            content_revision=content_revision,
            schema_revision=schema_revision,
        )
        if feature is None:
            raise TenantResourceNotFoundError("resource not found")
        if feature.feature_id != feature_id:
            raise SpatialValidationError("feature identity disagrees")
        return {
            "context": wire,
            "feature": self._serialize_response(
                context=context,
                detail=detail,
                wire=wire,
                features=[feature],
            )[0],
        }

    def refresh_reference_fixture_for_test(
        self, *, context: AuthorizationContext, source_id: UUID, variant: str
    ) -> SourceMutationResult:
        """Internal controlled acceptance-test seam; deliberately no HTTP route."""
        self.repository.prepare_scope(
            context=context, permission=Permission.PROJECT_EDIT, mutation=True
        )
        prior = SpatialCatalogService(self.repository)._active(context=context, source_id=source_id)
        if not prior.readable:
            raise SpatialSourceNotReadyError("source not ready")
        if variant not in ("content", "schema"):
            raise SpatialValidationError("invalid controlled fixture variant")
        next_revision(prior.catalog_revision, prior.catalog_revision)
        now = datetime.now(UTC)
        current = self.repository.refresh_reference_fixture_for_test(
            context=context, source=prior, variant=variant, now=now
        )
        evidence = build_mutation_evidence(
            context=context,
            prior=prior,
            current=current,
            operation=CatalogOperation.REFRESH_REFERENCE_FIXTURE,
            now=now,
        )
        self.repository.append_change(evidence.change, evidence.audit, context=context)
        return SourceMutationResult(current, evidence.change)


@dataclass(frozen=True, slots=True)
class CatalogChangePage:
    items: tuple[CatalogChange, ...]
    next_after_id: UUID | None


@dataclass(slots=True)
class SpatialHistoryService:
    """Current authorized, bounded, append-only catalog history."""

    repository: SpatialRepository

    def list_changes(
        self, *, context: AuthorizationContext, after_id: UUID | None, limit: int
    ) -> CatalogChangePage:
        self.repository.prepare_scope(
            context=context, permission=Permission.PROJECT_READ, mutation=False
        )
        if type(limit) is not int or not 1 <= limit <= 100:
            raise SpatialValidationError("catalog limit out of bounds")
        changes = self.repository.list_changes(context=context, after_id=after_id, limit=limit + 1)
        if len(changes) > limit + 1:
            raise SpatialValidationError("unbounded catalog evidence")
        previous = after_id
        for change in changes:
            if (change.organization_id, change.project_id) != (
                context.organization_id,
                context.project_id,
            ):
                raise SpatialValidationError("catalog evidence scope disagrees")
            if previous is not None and change.id <= previous:
                raise SpatialValidationError("catalog evidence is not strictly ordered")
            previous = change.id
        visible = tuple(changes[:limit])
        return CatalogChangePage(visible, visible[-1].id if len(changes) > limit else None)
