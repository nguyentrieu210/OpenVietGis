"""Real restricted-role native verification, independently seeded scoped rows."""

from __future__ import annotations

import json
from dataclasses import asdict, replace
from datetime import UTC, datetime, timedelta
from uuid import uuid4

import pytest
from sqlalchemy import text
from sqlalchemy.exc import SQLAlchemyError
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.db import get_session_factory
from openvietgis_api.adapters.persistence.rls import bind_authenticated_actor
from openvietgis_api.adapters.persistence.spatial_fixtures import (
    REFERENCE_FIXTURES,
    ReferenceCorrectionUnverifiedError,
    ReferenceFixtureIntegrityError,
    ReferenceFixtureQueryError,
    materialize_reference_fixture,
    native_geometry_parameters,
    validate_persisted_reference_source,
    validate_reference_candidate,
    validate_reference_correction,
)
from openvietgis_api.adapters.persistence.spatial_repository import SqlSpatialRepository
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.spatial.domain import ExtentState, ValidationState
from openvietgis_api.domains.spatial.service import PinnedSource, verify_source_pins
from tests.fixtures.identity_tenancy import AccessWorld, seed_access_world

NOW = datetime(2026, 11, 18, tzinfo=UTC)
NEGATIVE = {
    "reference_unknown_crs": "MISSING_CRS",
    "reference_conflicting_metadata": "GEOMETRY_FAMILY_MISMATCH",
    "reference_unsupported_dimensions": "UNSUPPORTED_DIMENSIONS",
    "reference_unsupported_dimensions_m": "UNSUPPORTED_DIMENSIONS",
    "reference_mixed_geometry": "UNSUPPORTED_GEOMETRY",
    "reference_unsupported_family": "UNSUPPORTED_GEOMETRY",
    "reference_null_geometry": "NULL_GEOMETRY",
    "reference_empty_geometry": "EMPTY_GEOMETRY",
    "reference_invalid_geometry": "INVALID_GEOMETRY",
    "reference_axis_conflict": "AXIS_CONFLICT",
    "reference_out_of_domain": "COORDINATE_OUT_OF_DOMAIN",
    "reference_definition_only_crs": "UNSUPPORTED_CRS",
    "reference_wrapping_extent": "EXTENT_WRAP_UNSUPPORTED",
    "reference_correctable_crs": "CRS_MISMATCH",
}
READY = {
    "reference_points_4326": (4000, (106.6, 10.7, 106.699, 10.739)),
    "reference_lines_4326": (3000, (106.6, 10.7, 106.6996, 10.7292)),
    "reference_polygons_4326": (3000, (106.6, 10.7, 106.6996, 10.7294)),
    "reference_points_3857": (
        3, (11866657.718562961, 1198103.040560261, 11928662.674934816, 2357927.646019978)
    ),
    "reference_points_4756": (5, None),
    "reference_points_4326_quang_ninh": (5, None),
    "reference_points_4756_hvs": (4, None),
    "reference_points_4326_hvs": (4, None),
    "reference_points_5899": (4, (437957.55, 2291039.83, 439365.92, 2292185.83)),
    "reference_points_4326_webmerc_out_of_area": (1, (0.0, 86.0, 0.0, 86.0)),
    "reference_empty_4326": (0, None),
}


def _prepare(session: Session, world: AccessWorld) -> AuthorizationContext:
    context = AuthorizationContext(
        Actor("human", world.owner_user_id, "Owner"),
        world.organization_id,
        project_id=world.project_id,
    )
    bind_authenticated_actor(session, context.actor)
    SqlSpatialRepository(session).prepare_scope(
        context=context, permission=Permission.PROJECT_EDIT, mutation=True
    )
    return context


def _candidate(world: AccessWorld, key: str) -> PinnedSource:
    return materialize_reference_fixture(
        key, organization_id=world.organization_id, project_id=world.project_id,
        source_id=uuid4(), name="Reference", now=NOW,
    )


def _insert(session: Session, pinned: PinnedSource) -> None:
    """Independent SQL seed, deliberately not the future registration repository."""
    metadata = asdict(pinned.source)
    metadata["validation_reason"] = (
        json.dumps(pinned.source.validation_reason.to_wire())
        if pinned.source.validation_reason is not None else None
    )
    metadata["extent"] = (
        json.dumps(asdict(pinned.source.extent)) if pinned.source.extent else None
    )
    session.execute(text("""
        INSERT INTO spatial.data_sources (
            id,organization_id,project_id,name,kind,reference_key,fixture_version,
            manifest_digest,source_asset_id,lifecycle,validation_state,validation_reason,
            source_crs,stored_crs,schema_state,geometry_family,dimensions,feature_count_state,
            feature_count,extent_state,extent,catalog_revision,content_revision,schema_revision,
            created_at,updated_at,retired_at
        ) VALUES (
            :id,:organization_id,:project_id,:name,:kind,:reference_key,:fixture_version,
            :manifest_digest,:source_asset_id,:lifecycle,:validation_state,
            CAST(:validation_reason AS jsonb),:source_crs,:stored_crs,:schema_state,
            :geometry_family,:dimensions,:feature_count_state,:feature_count,:extent_state,
            CAST(:extent AS jsonb),:catalog_revision,:content_revision,:schema_revision,
            :created_at,:updated_at,:retired_at
        )
    """), metadata)
    session.execute(text("""
        INSERT INTO spatial.source_fields (
            organization_id,project_id,source_id,field_id,name,ordinal,type,nullable,
            schema_revision
        ) VALUES (
            :organization_id,:project_id,:source_id,:field_id,:name,:ordinal,:type,:nullable,
            :schema_revision
        )
    """), [asdict(field) for field in pinned.fields])
    if pinned.features:
        scope = {
            "org": pinned.source.organization_id, "project": pinned.source.project_id,
            "source": pinned.source.id, "content": pinned.source.content_revision,
            "schema": pinned.source.schema_revision,
        }
        rows = [
            {
                "feature_id": feature.feature_id, "attributes": dict(feature.attributes),
                "state": feature.geometry_state.value,
                "wkt": wkt,
                "srid": srid,
            }
            for feature in pinned.features
            for wkt, srid in (native_geometry_parameters(feature.geometry),)
        ]
        session.execute(text("""
            INSERT INTO spatial.reference_features (
                organization_id,project_id,source_id,feature_id,attributes,geometry,
                geometry_state,content_revision,schema_revision
            )
            SELECT :org,:project,:source,r.feature_id,r.attributes,
                CASE WHEN r.wkt IS NULL THEN NULL
                    ELSE ST_GeomFromText(r.wkt,COALESCE(r.srid,0)) END,
                r.state,:content,:schema
            FROM jsonb_to_recordset(CAST(:rows AS jsonb))
                AS r(feature_id text, attributes jsonb, wkt text, srid integer, state text)
        """), scope | {"rows": json.dumps(rows)})


def test_all_native_fixtures_verified_twice_in_one_restricted_transaction(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    assert set(REFERENCE_FIXTURES) == set(READY) | set(NEGATIVE)
    with get_session_factory()() as session:
        context = _prepare(session, world)
        assert tuple(session.execute(text("""
            SELECT current_user,rolsuper,rolbypassrls
            FROM pg_roles WHERE rolname=current_user
        """)).one()) == ("openvietgis_runtime", False, False)
        for key, fixture in REFERENCE_FIXTURES.items():
            candidate = _candidate(world, key)
            result = validate_reference_candidate(session, fixture, candidate)
            assert result.source.revision == candidate.source.revision
            assert result.source.manifest_digest == candidate.source.manifest_digest
            assert result.source.catalog_revision == 1
            assert result.fields == candidate.fields and result.features == candidate.features
            verify_source_pins(result.source, result.fields, result.features)
            if key in READY:
                count, bounds = READY[key]
                assert result.source.validation_state == ValidationState.READY
                assert result.source.feature_count == count
                assert result.source.validation_reason is None
                if count == 0:
                    assert result.source.extent_state == ExtentState.EMPTY
                    assert result.source.extent is None
                else:
                    assert result.source.extent_state == ExtentState.AVAILABLE
                    extent = result.source.extent
                    assert extent is not None
                    if bounds is not None:
                        assert (extent.min_x, extent.min_y, extent.max_x, extent.max_y) == bounds
            else:
                assert result.source.validation_state == (
                    ValidationState.NOT_READY if key == "reference_unknown_crs"
                    else ValidationState.INVALID
                )
                assert result.source.validation_reason is not None
                assert result.source.validation_reason.code == NEGATIVE[key]
                assert result.source.extent_state == ExtentState.UNAVAILABLE
                assert result.source.extent is None
            _insert(session, result)
            assert validate_persisted_reference_source(
                session, fixture, result, context=context
            ) == result
        session.rollback()


@pytest.mark.parametrize("rename_first", [False, True])
def test_native_correction_verifies_existing_manifest_and_leaves_rows_unchanged(
    migrated_database: None, rename_first: bool,
) -> None:
    world = seed_access_world()
    with get_session_factory()() as session:
        context = _prepare(session, world)
        key = "reference_correctable_crs"
        original = validate_reference_candidate(
            session, REFERENCE_FIXTURES[key], _candidate(world, key)
        )
        _insert(session, original)
        if rename_first:
            renamed = replace(
                original.source, name="Renamed", catalog_revision=2,
                updated_at=NOW + timedelta(seconds=1),
            )
            session.execute(text("""
                UPDATE spatial.data_sources
                SET name=:name,catalog_revision=:revision,updated_at=:now
                WHERE id=:source
            """), {
                "name": renamed.name, "revision": renamed.catalog_revision,
                "now": renamed.updated_at, "source": renamed.id,
            })
            original = replace(original, source=renamed)
        snapshot = text("""
            SELECT ST_AsEWKB(geometry),attributes,content_revision,schema_revision
            FROM spatial.reference_features WHERE source_id=:source
        """)
        params = {"source": original.source.id}
        before = session.execute(snapshot, params).one()
        with pytest.raises(ReferenceCorrectionUnverifiedError):
            validate_reference_correction(
                session, original.source, context=context, declared_crs=4756, now=NOW
            )
        corrected = validate_reference_correction(
            session, original.source, context=context, declared_crs=4326,
            now=NOW + timedelta(seconds=2)
        )
        assert corrected.validation_state == ValidationState.READY
        assert corrected.source_crs == corrected.stored_crs == 4326
        assert corrected.manifest_digest == original.source.manifest_digest
        assert corrected.content_revision != original.source.content_revision
        assert corrected.schema_revision == original.source.schema_revision
        assert corrected.catalog_revision == original.source.catalog_revision + 1
        assert corrected.name == original.source.name
        assert corrected.extent is not None
        assert corrected.extent.min_x == pytest.approx(107.160886111111, rel=0, abs=1e-11)
        assert corrected.extent.min_y == pytest.approx(20.710688888889, rel=0, abs=1e-11)
        assert session.execute(snapshot, params).one() == before
        session.rollback()


@pytest.mark.parametrize(
    "tamper", ["attributes", "geometry", "schema", "missing", "pins", "feature_id", "extra"]
)
def test_persisted_integrity_mismatch_fails_closed(migrated_database: None, tamper: str) -> None:
    world = seed_access_world()
    with get_session_factory()() as session:
        context = _prepare(session, world)
        key = "reference_correctable_crs"
        fixture = REFERENCE_FIXTURES[key]
        original = validate_reference_candidate(session, fixture, _candidate(world, key))
        if tamper == "feature_id":
            # The runtime role cannot rename an existing primary key. Seed an
            # independently constructed wrong row instead (same count and EWKB).
            _insert(
                session,
                replace(
                    original,
                    features=(replace(original.features[0], feature_id="unexpected-id"),),
                ),
            )
        elif tamper != "missing":
            _insert(session, original)
        statements = {
            "attributes": """
                UPDATE spatial.reference_features
                SET attributes=jsonb_set(attributes,'{fld_rank}','true')
                WHERE source_id=:source
            """,
            "geometry": """
                UPDATE spatial.reference_features
                SET geometry=ST_GeomFromText('POINT(107.16 20.71)',4326)
                WHERE source_id=:source
            """,
            "schema": """
                UPDATE spatial.source_fields SET name='Changed'
                WHERE source_id=:source AND ordinal=0
            """,
            "pins": """
                UPDATE spatial.reference_features SET content_revision=repeat('a',64)
                WHERE source_id=:source
            """,
            "extra": """
                INSERT INTO spatial.reference_features (
                    organization_id, project_id, source_id, feature_id, attributes,
                    geometry, geometry_state, content_revision, schema_revision
                )
                SELECT organization_id, project_id, source_id, 'unexpected-id', attributes,
                    geometry, geometry_state, content_revision, schema_revision
                FROM spatial.reference_features WHERE source_id=:source
            """,
        }
        if tamper in statements:
            session.execute(text(statements[tamper]), {"source": original.source.id})
        with pytest.raises(ReferenceFixtureIntegrityError):
            validate_persisted_reference_source(session, fixture, original, context=context)
        with pytest.raises(ReferenceCorrectionUnverifiedError):
            validate_reference_correction(
                session, original.source, context=context, declared_crs=4326, now=NOW
            )
        session.rollback()


def test_candidate_manifest_and_persisted_scope_guard_fail_closed(migrated_database: None) -> None:
    a, b = seed_access_world(), seed_access_world()
    with get_session_factory()() as session:
        context = _prepare(session, a)
        fixture = REFERENCE_FIXTURES["reference_points_5899"]
        candidate = _candidate(a, fixture.key)
        forged = replace(candidate, source=replace(candidate.source, manifest_digest="f" * 64))
        with pytest.raises(ReferenceFixtureIntegrityError):
            validate_reference_candidate(session, fixture, forged)
        foreign = _candidate(b, fixture.key)
        with pytest.raises(PermissionError):
            validate_persisted_reference_source(session, fixture, foreign, context=context)
        session.rollback()


def test_unknown_native_query_failure_is_not_an_accepted_correction_rejection(
    migrated_database: None, monkeypatch: pytest.MonkeyPatch,
) -> None:
    world = seed_access_world()
    with get_session_factory()() as session:
        context = _prepare(session, world)
        key = "reference_correctable_crs"
        original = validate_reference_candidate(
            session, REFERENCE_FIXTURES[key], _candidate(world, key)
        )
        _insert(session, original)
        execute = session.execute

        def fail_native(statement, *args, **kwargs):
            if "ST_Zmflag" in str(statement):
                raise SQLAlchemyError("synthetic internal marker must not escape")
            return execute(statement, *args, **kwargs)

        with monkeypatch.context() as patch:
            patch.setattr(session, "execute", fail_native)
            with pytest.raises(ReferenceFixtureQueryError) as caught:
                validate_reference_correction(
                    session, original.source, context=context, declared_crs=4326, now=NOW
                )
            assert "internal marker" not in str(caught.value)
        session.rollback()
