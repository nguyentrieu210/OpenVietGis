"""US2 service lifecycle and capability invariants under the restricted role."""

from dataclasses import replace

import pytest

from openvietgis_api.adapters.persistence.db import get_session_factory
from openvietgis_api.adapters.persistence.rls import bind_authenticated_actor
from openvietgis_api.adapters.persistence.spatial_repository import SqlSpatialRepository
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from openvietgis_api.domains.authorization.service import TenantResourceNotFoundError
from openvietgis_api.domains.spatial.domain import (
    ExtentState,
    Lifecycle,
    ValidationReason,
    ValidationReasonCode,
    ValidationState,
)
from openvietgis_api.domains.spatial.ports import (
    SpatialLayerRevisionConflictError,
    SpatialNoChangesError,
    SpatialSourceNotReadyError,
)
from openvietgis_api.domains.spatial.service import (
    SpatialCatalogService,
    SpatialLayerService,
    layer_capabilities,
)
from tests.fixtures.identity_tenancy import seed_access_world


def test_layer_service_lifecycle_and_capabilities(migrated_database: None) -> None:
    world = seed_access_world()
    ctx = AuthorizationContext(
        Actor("human", world.owner_user_id, "Owner"),
        world.organization_id,
        project_id=world.project_id,
    )
    with get_session_factory()() as session:
        bind_authenticated_actor(session, ctx.actor)
        repo = SqlSpatialRepository(session)
        catalog = SpatialCatalogService(repo)
        layers = SpatialLayerService(repo)
        source = catalog.register_source(
            context=ctx, reference_key="reference_empty_4326", name="Empty"
        ).source
        unavailable = catalog.register_source(
            context=ctx, reference_key="reference_unknown_crs", name="Unknown"
        ).source
        with pytest.raises(SpatialSourceNotReadyError):
            layers.create_layer(context=ctx, source_id=unavailable.id, name="Rejected")
        first = layers.create_layer(context=ctx, source_id=source.id, name=" Same ").layer
        second = layers.create_layer(context=ctx, source_id=source.id, name="Same").layer
        assert first.id != second.id and first.name == second.name
        detail = layers.get_layer(context=ctx, layer_id=first.id)
        assert detail.source.source == source
        capabilities = {item.name: item for item in detail.capabilities}
        assert len(capabilities) == 9
        assert capabilities["feature_read"].technical_supported
        assert capabilities["feature_read"].available
        assert capabilities["feature_read"].actor_authorized
        assert all(
            not cap.technical_supported
            and not cap.available
            and not cap.actor_authorized
            and cap.required_permission is None
            for name, cap in capabilities.items()
            if name
            not in (
                "metadata_read",
                "feature_read",
                "render",
                "style",
                "export",
                "query",
                "analysis",
                "offline",
            )
        )
        no_authority = {
            item.name: item for item in layer_capabilities(first, source, actor_authorized=False)
        }
        assert (
            no_authority["feature_read"].available
            and not no_authority["feature_read"].actor_authorized
        )
        unready = replace(
            source,
            validation_state=ValidationState.NOT_READY,
            extent_state=ExtentState.UNAVAILABLE,
            validation_reason=ValidationReason(
                ValidationReasonCode.FIXTURE_UNAVAILABLE, "Unavailable"
            ),
        )
        degraded = {
            item.name: item for item in layer_capabilities(first, unready, actor_authorized=True)
        }
        assert degraded["metadata_read"].available
        assert degraded["feature_read"].technical_supported
        assert degraded["feature_read"].actor_authorized and not degraded["feature_read"].available
        with pytest.raises(SpatialLayerRevisionConflictError):
            layers.rename_layer(context=ctx, layer_id=first.id, expected_revision=2, name="Same")
        with pytest.raises(SpatialNoChangesError):
            layers.rename_layer(context=ctx, layer_id=first.id, expected_revision=1, name="Same")
        renamed = layers.rename_layer(
            context=ctx, layer_id=first.id, expected_revision=1, name="Renamed"
        )
        assert renamed.layer.layer_revision == 2
        retired = layers.retire_layer(context=ctx, layer_id=first.id, expected_revision=2)
        assert retired.layer.lifecycle == Lifecycle.RETIRED and retired.layer.layer_revision == 3
        with pytest.raises(TenantResourceNotFoundError):
            layers.get_layer(context=ctx, layer_id=first.id)
        assert layers.get_layer(context=ctx, layer_id=second.id).source.source == source
        assert [
            detail.layer
            for detail in layers.list_layers(context=ctx, after_id=None, limit=100).items
        ] == [second]
        session.commit()
