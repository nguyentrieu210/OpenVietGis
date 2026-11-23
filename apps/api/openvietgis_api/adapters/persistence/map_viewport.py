"""Pinned R004 presentation reads using approved fixed PostGIS operations only."""

from __future__ import annotations

from collections.abc import Iterator
from itertools import chain
from typing import cast
from uuid import UUID

from sqlalchemy import text
from openvietgis_api.adapters.persistence.gis_access import scoped
from sqlalchemy.exc import SQLAlchemyError
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.spatial_repository import (
    SqlSpatialRepository,
    _read_native_wkb,
)
from openvietgis_api.adapters.persistence.spatial_transform import _provenance
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.maps.viewport import (
    MAP_STATEMENT_TIMEOUT_MS,
    MAX_RENDER_SOURCE_FEATURES,
    MAX_VIEWPORT_POSITIONS,
    DisplayExtentWire,
    MapReadUnavailableError,
    RenderFeature,
    ViewportBounds,
    ViewportWire,
    build_viewport_response,
)
from openvietgis_api.domains.spatial.crs import (
    WEB_MERCATOR_MAX_LATITUDE,
    get_transformation_profile,
)
from openvietgis_api.domains.spatial.domain import (
    SUPPORTED_FAMILIES,
    DataSource,
    GeometryDimensions,
    SourceKind,
)
from openvietgis_api.domains.spatial.ports import (
    SpatialTransformationOutOfAreaError,
    SpatialUnsupportedTransformationError,
)
from openvietgis_api.domains.spatial.service import (
    FeatureReadContextWire,
    SpatialFeatureService,
)


def _display_ctes(source: DataSource) -> tuple[str, dict[str, object]]:
    """Build identifiers from fixed code only; all pipeline values are bound."""
    profile = (
        get_transformation_profile(source.stored_crs, 4326)
        if source.stored_crs is not None and source.stored_crs != 4326
        else None
    )
    if source.stored_crs != 4326 and profile is None:
        raise SpatialUnsupportedTransformationError("no approved geographic presentation edge")
    params: dict[str, object] = {}
    ctes = [
        """
        native AS NOT MATERIALIZED (
            SELECT feature_id,geometry,
                content_revision=:content AND schema_revision=:schema
                AND geometry_state='present'
                AND ST_SRID(geometry)=:native_srid AND ST_Zmflag(geometry)=0
                AND ST_GeometryType(geometry)=:geometry_type
                AND ST_NPoints(geometry) BETWEEN 1 AND 4096
                AND NOT ST_IsEmpty(geometry) AND ST_IsValid(geometry) AS valid
            FROM spatial.reference_features
            WHERE organization_id=:organization AND project_id=:project AND source_id=:source
            ORDER BY feature_id COLLATE "C" LIMIT 10001
        )
    """
    ]
    previous = "native"
    current_code = source.stored_crs
    for index, step in enumerate(profile.steps if profile is not None else ()):
        assert profile is not None
        function = "ST_InverseTransformPipeline" if step.inverse else "ST_TransformPipeline"
        # Check every geographic intermediate before the following fixed step.
        check = "valid"
        if current_code in (4326, 4756):
            bounds = profile.geographic_bounds
            params |= {
                f"minx{index}": bounds.min_x,
                f"miny{index}": bounds.min_y,
                f"maxx{index}": bounds.max_x,
                f"maxy{index}": bounds.max_y,
            }
            check += (
                " AND ST_CoveredBy(geometry,ST_MakeEnvelope("
                f":minx{index},:miny{index},:maxx{index},:maxy{index},{current_code}))"
            )
        params |= {f"pipeline{index}": step.pipeline, f"target{index}": step.target_code}
        name = f"step{index}"
        ctes.append(f"""
            {name} AS NOT MATERIALIZED (
                SELECT feature_id,CASE WHEN {check}
                    THEN {function}(geometry,:pipeline{index},:target{index}) END AS geometry,
                    {check} AS valid FROM {previous}
            )
        """)
        previous = name
        current_code = step.target_code
    # For a native 4326 source the geometry hasn't changed. The native check
    # already enforces the exact SRID, XY, cardinality, nonempty and validity
    # predicates; repeat them only after a transformation.
    display_check = "valid"
    if profile is not None:
        display_check += (
            " AND geometry IS NOT NULL AND ST_SRID(geometry)=4326"
            " AND ST_Zmflag(geometry)=0 AND ST_NPoints(geometry) BETWEEN 1 AND 4096"
            " AND NOT ST_IsEmpty(geometry) AND ST_IsValid(geometry)"
        )
    display_check += (
        " AND ST_CoveredBy(geometry,ST_MakeEnvelope(-180,:display_south,180,:display_north,4326))"
    )
    params |= {
        "display_south": -WEB_MERCATOR_MAX_LATITUDE,
        "display_north": WEB_MERCATOR_MAX_LATITUDE,
    }
    if profile is not None:
        bounds = profile.geographic_bounds
        params |= {
            "area_west": bounds.min_x,
            "area_south": bounds.min_y,
            "area_east": bounds.max_x,
            "area_north": bounds.max_y,
        }
        display_check += (
            " AND ST_CoveredBy(geometry,ST_MakeEnvelope("
            ":area_west,:area_south,:area_east,:area_north,4326))"
        )
    ctes.append(f"""
        display AS MATERIALIZED (
            SELECT feature_id,geometry,{display_check} AS valid FROM {previous}
        )
    """)
    return "WITH " + ",".join(ctes), params


class SqlMapViewportRepository(SqlSpatialRepository):
    def __init__(self, session: Session) -> None:
        super().__init__(session)

    def _snapshot(
        self,
        *,
        context: AuthorizationContext,
        layer_id: UUID,
        content_revision: str,
        schema_revision: str,
    ) -> tuple[DataSource, FeatureReadContextWire, str, dict[str, object]]:
        self._session.execute(
            text("SELECT set_config('statement_timeout',:timeout,true)"),
            {"timeout": f"{MAP_STATEMENT_TIMEOUT_MS}ms"},
        )
        detail, wire = SpatialFeatureService(self)._snapshot(
            context=context,
            layer_id=layer_id,
            content_revision=content_revision,
            schema_revision=schema_revision,
            include_geometry=True,
            response_crs=4326,
        )
        source = detail.source
        expected_count=source.feature_count
        if scoped(self._session,context):
            expected_count=self._session.execute(text("SELECT count(*) FROM spatial.reference_features WHERE organization_id=:org AND project_id=:proj AND source_id=:src"),{"org":context.organization_id,"proj":context.project_id,"src":source.id}).scalar_one()
        if (
            source.kind
            not in (
                SourceKind.REFERENCE_FIXTURE,
                SourceKind.IMPORTED_FILE,
                SourceKind.DERIVED_PROCESSING,
            )
            or source.dimensions != GeometryDimensions.XY
            or source.geometry_family not in SUPPORTED_FAMILIES
            or expected_count is None
            or expected_count > MAX_RENDER_SOURCE_FEATURES
        ):
            raise SpatialUnsupportedTransformationError("source exceeds supported render profile")
        if source.stored_crs != 4326:
            assert source.stored_crs is not None
            profile = get_transformation_profile(source.stored_crs, 4326)
            if profile is None:
                raise SpatialUnsupportedTransformationError("no approved direct operation")
            _provenance(source, profile)
        ctes, params = _display_ctes(source)
        params |= dict(self._scope(context, permission=Permission.PROJECT_READ))
        params |= {
            "source": source.id,
            "content": content_revision,
            "schema": schema_revision,
            "native_srid": source.stored_crs,
            "geometry_type": "ST_" + source.geometry_family.value,
            "expected_count": expected_count,
        }
        return source, wire, ctes, params

    @staticmethod
    def _validate_summary(expected_count: int | None, count: int, valid: bool) -> None:
        # Both viewport and extent validate all source rows, including those
        # outside the current view. The summary and payload share a statement
        # snapshot under the original per-request authorization/pin locks.
        if count != expected_count or count > MAX_RENDER_SOURCE_FEATURES:
            raise MapReadUnavailableError("render snapshot count disagrees")
        if not valid:
            raise SpatialTransformationOutOfAreaError("source is outside approved display evidence")

    def viewport(
        self,
        *,
        context: AuthorizationContext,
        layer_id: UUID,
        content_revision: str,
        schema_revision: str,
        bounds: ViewportBounds,
        limit: int = 500,
    ) -> ViewportWire:
        try:
            source, wire, ctes, params = self._snapshot(
                context=context,
                layer_id=layer_id,
                content_revision=content_revision,
                schema_revision=schema_revision,
            )
            # A bounded full-source materialized display is reused by summary
            # and candidate, avoiding repeated transforms and validation. The
            # candidate holds at most limit+1 geometries; only the <=20000-position
            # prefix is encoded as WKB plus a metadata-only over-budget marker.
            # The summary gates intersection work and is always returned, even
            # on empty/invalid sources. No all-row WKB or cross-request cache.
            query = (
                ctes
                + """
                ,summary AS MATERIALIZED (
                    SELECT count(*) AS snapshot_count,
                        COALESCE(bool_and(valid IS TRUE),true) AS snapshot_valid
                    FROM display
                ),candidate AS MATERIALIZED (
                    SELECT feature_id,geometry,ST_NPoints(geometry) AS positions
                    FROM display CROSS JOIN summary
                    WHERE CASE WHEN snapshot_count=:expected_count
                        AND snapshot_valid AND valid THEN
                        ST_Intersects(geometry,ST_MakeEnvelope(:west,:south,:east,:north,4326))
                        ELSE false END
                    ORDER BY feature_id COLLATE "C" LIMIT :lookahead
                ),prefix AS MATERIALIZED (
                    SELECT *,SUM(positions) OVER (
                        ORDER BY feature_id COLLATE "C" ROWS UNBOUNDED PRECEDING
                    ) AS total_positions FROM candidate
                )
                SELECT summary.snapshot_count,summary.snapshot_valid,
                    prefix.feature_id,positions,
                    CASE WHEN total_positions<=:position_budget
                        THEN ST_AsBinary(prefix.geometry,'NDR') END AS wkb
                FROM summary LEFT JOIN prefix ON total_positions-positions<=:position_budget
                ORDER BY prefix.feature_id COLLATE "C"
            """
            )
            rows = self._session.execute(
                text(query),
                params
                | {
                    "west": bounds.west,
                    "south": bounds.south,
                    "east": bounds.east,
                    "north": bounds.north,
                    "lookahead": limit + 1,
                    "position_budget": MAX_VIEWPORT_POSITIONS,
                },
            ).mappings()

            iterator = iter(rows)
            first = next(iterator, None)
            if first is None:
                raise MapReadUnavailableError("missing render snapshot summary")
            self._validate_summary(
                params["expected_count"], first["snapshot_count"], first["snapshot_valid"]
            )

            def features() -> Iterator[RenderFeature]:
                for row in chain((first,), iterator):
                    # LEFT JOIN summary marker is not a feature; a genuine
                    # empty viewport remains complete only after validation.
                    if row["feature_id"] is None:
                        continue
                    wkb = row["wkb"]
                    yield RenderFeature(
                        row["feature_id"],
                        _read_native_wkb(bytes(wkb), 4326) if wkb is not None else None,
                        row["positions"],
                    )

            return build_viewport_response(wire, features(), limit=limit)
        except SQLAlchemyError:
            self._session.rollback()
            raise MapReadUnavailableError("bounded map read unavailable") from None

    def display_extent(
        self,
        *,
        context: AuthorizationContext,
        layer_id: UUID,
        content_revision: str,
        schema_revision: str,
    ) -> DisplayExtentWire:
        try:
            source, wire, ctes, params = self._snapshot(
                context=context,
                layer_id=layer_id,
                content_revision=content_revision,
                schema_revision=schema_revision,
            )
            row = (
                self._session.execute(
                    text(
                        ctes
                        + """
                    SELECT snapshot_count,snapshot_valid,
                        ST_XMin(extent) AS west,ST_YMin(extent) AS south,
                        ST_XMax(extent) AS east,ST_YMax(extent) AS north
                    FROM (
                        SELECT count(*) AS snapshot_count,
                            COALESCE(bool_and(valid IS TRUE),true) AS snapshot_valid,
                            ST_Extent(geometry) FILTER (WHERE valid IS TRUE) AS extent
                        FROM display
                    ) AS actual
                """
                    ),
                    params,
                )
                .mappings()
                .one()
            )
            self._validate_summary(
                params["expected_count"], row["snapshot_count"], row["snapshot_valid"]
            )
            if row["west"] is None:
                return {"context": wire, "state": "empty", "bounds": None}
            return {
                "context": wire,
                "state": "available",
                "bounds": (row["west"], row["south"], row["east"], row["north"]),
            }
        except SQLAlchemyError:
            self._session.rollback()
            raise MapReadUnavailableError("bounded map extent unavailable") from None
