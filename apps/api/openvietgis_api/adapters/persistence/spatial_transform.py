"""Bounded response-copy transforms; fixed operations, scoped native rows, no writes."""

from collections.abc import Callable, Mapping, Sequence
from uuid import UUID

from sqlalchemy import text
from sqlalchemy.exc import SQLAlchemyError
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.spatial_fixtures import (
    FixtureProvenance,
    get_reference_fixture,
    materialize_reference_fixture,
)
from openvietgis_api.domains.spatial.crs import (
    TM3_PIPELINE,
    CoordinateBounds,
    PipelineStep,
    TransformationProfile,
    get_transformation_profile,
)
from openvietgis_api.domains.spatial.domain import (
    DataSource,
    NativeGeometry,
    ReferenceFeature,
    SpatialValidationError,
    validate_ready_geometry,
)
from openvietgis_api.domains.spatial.ports import (
    SpatialTransformationOutOfAreaError,
    SpatialTransformationUnavailableError,
    SpatialUnsupportedTransformationError,
)


def _area(geometries: Sequence[NativeGeometry], bounds: CoordinateBounds) -> None:
    for geometry in geometries:
        vertices = validate_ready_geometry(geometry, expected_srid=geometry.srid)
        if any(not bounds.contains(x, y) for x, y in vertices):
            raise SpatialTransformationOutOfAreaError("vertex outside validated area")


def _provenance(source: DataSource, profile: TransformationProfile) -> None:
    if not profile.requires_trusted_fixture:
        return
    fixture = get_reference_fixture(source.reference_key)
    if (
        fixture is None
        or not fixture.trusted_quang_ninh
        or fixture.version != source.fixture_version
        or fixture.stored_crs != source.stored_crs
        or (
            5899 in (profile.source_code, profile.target_code)
            and fixture.provenance != FixtureProvenance.QUANG_NINH_HVS
        )
    ):
        raise SpatialTransformationOutOfAreaError("operation evidence does not cover fixture")
    original = materialize_reference_fixture(
        fixture.key,
        organization_id=source.organization_id,
        project_id=source.project_id,
        source_id=source.id,
        name=source.name,
        now=source.created_at,
    )
    if source.manifest_digest != original.source.manifest_digest:
        raise SpatialTransformationOutOfAreaError("operation evidence binding changed")


def transform_response_geometries(
    session: Session,
    *,
    scope: Mapping[str, UUID],
    source: DataSource,
    features: Sequence[ReferenceFeature],
    target_code: int,
    decode: Callable[[bytes, int], NativeGeometry],
) -> tuple[NativeGeometry, ...]:
    """Caller holds current project/source read locks and verified transaction context.

    Every SQL step joins the original scoped, pinned rows and checks exact native
    WKB. Intermediate geometries are bounded response values. Neither a caller
    pipeline nor an implicit ST_Transform operation can enter this path.
    """
    if source.stored_crs is None:
        raise SpatialValidationError("ready source requires native SRID")
    profile = get_transformation_profile(source.stored_crs, target_code)
    if profile is None:
        raise SpatialUnsupportedTransformationError("no approved direct operation")
    if not source.readable or len(features) > 101:
        raise SpatialValidationError("invalid transform snapshot")
    _provenance(source, profile)
    originals: list[NativeGeometry] = []
    seen: set[str] = set()
    for feature in features:
        if (
            (feature.organization_id, feature.project_id, feature.source_id)
            != (source.organization_id, source.project_id, source.id)
            or (feature.content_revision, feature.schema_revision)
            != (source.content_revision, source.schema_revision)
            or feature.feature_id in seen
            or feature.geometry is None
        ):
            raise SpatialValidationError("transform feature scope or pins disagree")
        seen.add(feature.feature_id)
        validate_ready_geometry(feature.geometry, source.geometry_family, source.stored_crs)
        originals.append(feature.geometry)
    if not originals:
        return ()

    def apply(
        geometries: Sequence[NativeGeometry],
        step: PipelineStep,
    ) -> tuple[NativeGeometry, ...]:
        import json

        if any(geometry.srid != step.source_code for geometry in geometries):
            raise SpatialValidationError("explicit step input SRID disagrees")
        records = [
            {"id": feature.feature_id, "wkt": geometry.to_wkt(), "native": original.to_wkt()}
            for feature, geometry, original in zip(features, geometries, originals, strict=True)
        ]
        # Function identifier comes only from this boolean, never from data.
        function = "ST_InverseTransformPipeline" if step.inverse else "ST_TransformPipeline"
        query = (
            """
            WITH transformed AS MATERIALIZED (
                SELECT n.feature_id,
        """
            + function
            + """(
                    ST_GeomFromText(r.wkt,:input_srid), :pipeline, :output_srid
                ) AS geometry
                FROM jsonb_to_recordset(CAST(:records AS jsonb)) AS r(id text,wkt text,native text)
                JOIN spatial.reference_features AS n ON n.feature_id=r.id
                WHERE n.organization_id=:organization AND n.project_id=:project
                    AND n.source_id=:source AND n.content_revision=:content
                    AND n.schema_revision=:schema AND n.geometry_state='present'
                    AND ST_AsBinary(n.geometry,'NDR') = ST_AsBinary(
                        ST_GeomFromText(r.native,:native_srid),'NDR')
            )
            SELECT feature_id, ST_SRID(geometry) AS srid,
                CASE WHEN ST_Zmflag(geometry)=0 AND ST_NPoints(geometry) BETWEEN 1 AND 4096
                    AND NOT ST_IsEmpty(geometry) THEN ST_IsValid(geometry)
                    ELSE false END AS valid,
                ST_AsBinary(geometry,'NDR') AS wkb
            FROM transformed
        """
        )
        try:
            rows = (
                session.execute(
                    text(query),
                    dict(scope)
                    | {
                        "source": source.id,
                        "content": source.content_revision,
                        "schema": source.schema_revision,
                        "records": json.dumps(records, allow_nan=False),
                        "native_srid": source.stored_crs,
                        "input_srid": step.source_code,
                        "output_srid": step.target_code,
                        "pipeline": step.pipeline,
                    },
                )
                .mappings()
                .all()
            )
        except SQLAlchemyError:
            raise SpatialTransformationUnavailableError("explicit operation failed") from None
        if len(rows) != len(features) or {row["feature_id"] for row in rows} != seen:
            raise SpatialValidationError("transform snapshot no longer agrees")
        result = {}
        for row in rows:
            if row["srid"] != step.target_code or not row["valid"] or row["wkb"] is None:
                raise SpatialTransformationUnavailableError("invalid operation output")
            geometry = decode(bytes(row["wkb"]), step.target_code)
            validate_ready_geometry(
                geometry, source.geometry_family, step.target_code, output_roundoff=True
            )
            result[row["feature_id"]] = geometry
        return tuple(result[feature.feature_id] for feature in features)

    current = tuple(originals)
    # Projected native domains are validated before inverse projection. The
    # regional predicate is applied as soon as geographic vertices exist, and
    # before each following step. No input clipping, output repair or fallback.
    if source.stored_crs in (4326, 4756):
        _area(current, profile.geographic_bounds)
    for step in profile.steps:
        current = apply(current, step)
        if step.target_code in (4326, 4756):
            _area(current, profile.geographic_bounds)
    if target_code == 5899:
        checked = apply(current, PipelineStep(5899, 4756, TM3_PIPELINE, True))
        _area(checked, profile.geographic_bounds)
    return current
