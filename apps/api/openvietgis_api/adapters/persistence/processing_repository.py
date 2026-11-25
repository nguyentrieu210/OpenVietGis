"""R008 RLS-enforced PostGIS spatial processing runner (Buffer, Clip, Dissolve)."""

from __future__ import annotations

import hashlib
import json
from datetime import UTC, datetime
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import text
from sqlalchemy.engine import RowMapping
from sqlalchemy.exc import SQLAlchemyError
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.adapters.persistence.spatial_repository import (
    _read_native_wkb,
    _source_from_row,
)
from openvietgis_api.adapters.persistence.web_transform_guard import (
    require_web_domain,
    web_transform_sql,
)
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.editing.domain import validate_operation_id
from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformDomainError,
    PlatformNotFoundError,
    PlatformValidationError,
)
from openvietgis_api.domains.processing.domain import (
    AlgorithmId,
    ProcessingJobRecord,
    ProcessingStatus,
    validate_algorithm_parameters,
)
from openvietgis_api.domains.spatial.domain import (
    DataSource,
    FieldType,
    GeometryDimensions,
    GeometryFamily,
    GeometryState,
    Lifecycle,
    NativeGeometry,
    ReferenceFeature,
    SourceField,
    SpatialValidationError,
    ValidationState,
    canonical_attribute_value,
    content_revision_digest,
    schema_revision_digest,
    validate_ready_geometry,
)

# Bounds for the first synchronous processing slice. Output is also constrained
# by the Core reader's per-feature geometry limits and transfer feature cap.
MAX_PROCESSING_FEATURES = 50_000
MAX_PROCESSING_POSITIONS = 1_000_000
PROCESSING_STATEMENT_TIMEOUT = "30s"

# Return no binary geometry when the result exceeds either resource bound.
# All output rows are counted *before* the LIMIT, never silently truncated.
_RESULT_GEOMETRY_COLUMNS = """
    ST_SRID(geom) AS srid,
    COUNT(*) OVER () AS total_count,
    SUM(ST_NPoints(geom)) OVER () AS total_positions,
    CASE WHEN COUNT(*) OVER () <= :max_features
        AND SUM(ST_NPoints(geom)) OVER () <= :max_positions
        AND ST_Zmflag(geom)=0
        AND ST_NPoints(geom) BETWEEN 1 AND 4096
        AND ST_IsValid(geom)
      THEN ST_AsBinary(geom, 'NDR')
      ELSE NULL END AS wkb
"""


def _bounded_native_geometry(
    row: RowMapping, expected_family: GeometryFamily, srid: int
) -> NativeGeometry:
    if (
        row["total_count"] > MAX_PROCESSING_FEATURES
        or row["total_positions"] > MAX_PROCESSING_POSITIONS
    ):
        raise PlatformValidationError(
            "processing output exceeds resource bounds", code="PROCESSING_LIMIT"
        )
    if row["wkb"] is None or row["srid"] != srid:
        raise PlatformValidationError(
            "invalid derived output geometry", code="INVALID_DERIVED_OUTPUT"
        )
    try:
        geometry = _read_native_wkb(bytes(row["wkb"]), srid)
        validate_ready_geometry(geometry, expected_family, srid)
    except SpatialValidationError as exc:
        raise PlatformValidationError(
            "invalid derived output geometry", code="INVALID_DERIVED_OUTPUT"
        ) from exc
    return geometry


def _job_from_row(row: RowMapping) -> ProcessingJobRecord:
    return ProcessingJobRecord(
        id=row["id"],
        organization_id=row["organization_id"],
        project_id=row["project_id"],
        algorithm_id=AlgorithmId(str(row["algorithm_id"])),
        operation_id=str(row["operation_id"]),
        status=ProcessingStatus(str(row["status"])),
        actor_type=str(row["actor_type"]),
        actor_id=row["actor_id"],
        input_refs=dict(row["input_refs"]),
        parameters=dict(row["parameters"]),
        output_name=str(row["output_name"]),
        output_crs=int(row["output_crs"]),
        engine_version=str(row["engine_version"]),
        attempt_count=int(row["attempt_count"]),
        output_source_id=row["output_source_id"],
        output_layer_id=row["output_layer_id"],
        output_content_revision=str(row["output_content_revision"])
        if row["output_content_revision"]
        else None,
        feature_count=int(row["feature_count"]) if row["feature_count"] is not None else None,
        error_code=str(row["error_code"]) if row["error_code"] else None,
        error_message=str(row["error_message"]) if row["error_message"] else None,
        created_at=row["created_at"],
        updated_at=row["updated_at"],
        completed_at=row["completed_at"],
    )


class SqlProcessingRepository:
    def __init__(self, session: Session) -> None:
        self._session = session

    def _prepare(self, context: AuthorizationContext, *, permission: Permission) -> None:
        prepare_project_scope(self._session, context, permission=permission)

    def _resolve_layer(self, org_id: UUID, project_id: UUID, layer_id: UUID) -> DataSource:
        row = (
            self._session.execute(
                text("""
                SELECT s.*
                FROM spatial.layers l
                JOIN spatial.data_sources s
                  ON s.organization_id = l.organization_id
                 AND s.project_id = l.project_id
                 AND s.id = l.source_id
                WHERE l.organization_id = :org AND l.project_id = :proj AND l.id = :lid
                  AND l.lifecycle = 'active'
                FOR SHARE OF s, l
            """),
                {"org": org_id, "proj": project_id, "lid": layer_id},
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError(f"layer {layer_id} not found")
        source = _source_from_row(row)
        if source.lifecycle != Lifecycle.ACTIVE or source.validation_state != ValidationState.READY:
            raise PlatformValidationError(
                "input layer source is not ready", code="SOURCE_NOT_READY"
            )
        return source

    def _validate_pinned_source(self, job: ProcessingJobRecord, prefix: str) -> DataSource:
        refs = job.input_refs
        # Legacy queued results without schema pins cannot be executed honestly.
        # Resubmit using a new operation_id rather than repinning in place.
        if not refs.get(f"{prefix}_schema_revision"):
            raise PlatformConflictError(
                "job input has no immutable schema pin; submit a new job",
                code="MISSING_INPUT_PIN",
            )
        try:
            source = self._resolve_layer(
                job.organization_id, job.project_id, UUID(refs[f"{prefix}_layer_id"])
            )
        except (PlatformNotFoundError, PlatformValidationError) as exc:
            raise PlatformConflictError(
                "job input is no longer active and ready", code="STALE_INPUT_PIN"
            ) from exc
        if (
            str(source.id) != refs[f"{prefix}_source_id"]
            or source.content_revision != refs[f"{prefix}_content_revision"]
            or source.schema_revision != refs[f"{prefix}_schema_revision"]
            or source.stored_crs != refs[f"{prefix}_stored_crs"]
        ):
            raise PlatformConflictError(
                "job input changed; submit a new job", code="STALE_INPUT_PIN"
            )
        stats = (
            self._session.execute(
                text("""
                SELECT COUNT(*) AS cnt, COALESCE(SUM(ST_NPoints(geometry)),0) AS positions,
                    COUNT(*) FILTER (WHERE
                        content_revision <> :crev OR schema_revision <> :srev
                        OR geometry_state <> 'present' OR geometry IS NULL
                        OR ST_SRID(geometry) <> :srid
                        OR ST_GeometryType(geometry) <> :gtype
                        OR ST_Zmflag(geometry) <> 0 OR ST_IsEmpty(geometry)
                        OR NOT ST_IsValid(geometry)) AS invalid
                FROM spatial.reference_features
                WHERE organization_id=:org AND project_id=:proj AND source_id=:src
            """),
                {
                    "org": job.organization_id,
                    "proj": job.project_id,
                    "src": source.id,
                    "crev": source.content_revision,
                    "srev": source.schema_revision,
                    "srid": source.stored_crs,
                    "gtype": f"ST_{source.geometry_family.value}",
                },
            )
            .mappings()
            .one()
        )
        if stats["cnt"] != source.feature_count or stats["invalid"]:
            raise PlatformValidationError(
                "input content does not match ready pins", code="INVALID_INPUT"
            )
        if stats["cnt"] > MAX_PROCESSING_FEATURES or stats["positions"] > MAX_PROCESSING_POSITIONS:
            raise PlatformValidationError(
                "processing input exceeds resource bounds", code="PROCESSING_LIMIT"
            )
        return source

    def submit_and_run(
        self,
        *,
        context: AuthorizationContext,
        algorithm_id: AlgorithmId,
        input_layer_id: UUID,
        overlay_layer_id: UUID | None,
        parameters: dict[str, Any],
        output_name: str,
        operation_id: str,
        execute_now: bool = True,
    ) -> ProcessingJobRecord:
        self._prepare(context, permission=Permission.PROJECT_EDIT)
        assert context.organization_id is not None and context.project_id is not None
        assert context.actor.actor_id is not None
        org_id, project_id = context.organization_id, context.project_id
        clean_op_id = validate_operation_id(operation_id)
        clean_out_name = output_name.strip()[:200] or f"{algorithm_id.value} output"
        clean_params = validate_algorithm_parameters(algorithm_id, parameters)
        if algorithm_id != AlgorithmId.CLIP and overlay_layer_id is not None:
            raise PlatformValidationError(
                "only clip accepts overlay_layer_id", code="INVALID_PARAMETERS"
            )

        # Idempotency check: same operation_id must denote the same submitted request.
        # Source pins themselves stay immutable even when the input changes later.
        existing = (
            self._session.execute(
                text("""
                SELECT * FROM processing.jobs
                WHERE organization_id = :org AND project_id = :proj AND operation_id = :op
            """),
                {"org": org_id, "proj": project_id, "op": clean_op_id},
            )
            .mappings()
            .first()
        )
        if existing is not None:
            previous_refs = existing["input_refs"]
            if (
                str(existing["algorithm_id"]) != algorithm_id.value
                or previous_refs.get("input_layer_id") != str(input_layer_id)
                or previous_refs.get("overlay_layer_id")
                != (str(overlay_layer_id) if overlay_layer_id is not None else None)
                or existing["parameters"] != clean_params
                or existing["output_name"] != clean_out_name
            ):
                raise PlatformConflictError(
                    "operation_id already used for a different processing request",
                    code="OPERATION_ID_CONFLICT",
                )
            return _job_from_row(existing)

        input_source = self._resolve_layer(org_id, project_id, input_layer_id)
        assert input_source.stored_crs is not None
        input_refs: dict[str, Any] = {
            "input_layer_id": str(input_layer_id),
            "input_source_id": str(input_source.id),
            "input_content_revision": input_source.content_revision,
            "input_schema_revision": input_source.schema_revision,
            "input_stored_crs": input_source.stored_crs,
        }
        if algorithm_id == AlgorithmId.CLIP:
            if overlay_layer_id is None:
                raise PlatformValidationError(
                    "clip requires overlay_layer_id", code="MISSING_OVERLAY_LAYER"
                )
            overlay_source = self._resolve_layer(org_id, project_id, overlay_layer_id)
            if overlay_source.geometry_family != GeometryFamily.POLYGON:
                raise PlatformValidationError(
                    "clip overlay must be Polygon", code="INVALID_OVERLAY"
                )
            input_refs["overlay_layer_id"] = str(overlay_layer_id)
            input_refs["overlay_source_id"] = str(overlay_source.id)
            input_refs["overlay_content_revision"] = overlay_source.content_revision
            input_refs["overlay_schema_revision"] = overlay_source.schema_revision
            input_refs["overlay_stored_crs"] = overlay_source.stored_crs

        pg_ver = str(self._session.execute(text("SELECT PostGIS_Lib_Version()")).scalar_one())
        engine_version = f"PostGIS/{pg_ver}"
        now = datetime.now(UTC)
        job_id = uuid4()

        row = (
            self._session.execute(
                text("""
                INSERT INTO processing.jobs (
                    id, organization_id, project_id, algorithm_id, operation_id, status,
                    actor_type, actor_id, input_refs, parameters, output_name, output_crs,
                    engine_version, attempt_count, created_at, updated_at
                ) VALUES (
                    :id, :org, :proj, :algo, :op, 'queued',
                    :atype, :aid, CAST(:refs AS jsonb), CAST(:params AS jsonb),
                    :oname, :ocrs, :eng, 1, :now, :now
                )
                RETURNING *
            """),
                {
                    "id": job_id,
                    "org": org_id,
                    "proj": project_id,
                    "algo": algorithm_id.value,
                    "op": clean_op_id,
                    "atype": str(context.actor.actor_type),
                    "aid": context.actor.actor_id,
                    "refs": json.dumps(input_refs),
                    "params": json.dumps(clean_params),
                    "oname": clean_out_name,
                    "ocrs": input_source.stored_crs,
                    "eng": engine_version,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        self._session.commit()

        if not execute_now:
            return _job_from_row(row)
        return self.execute_job(context=context, job_id=job_id)

    def execute_job(self, *, context: AuthorizationContext, job_id: UUID) -> ProcessingJobRecord:
        self._prepare(context, permission=Permission.PROJECT_EDIT)
        assert context.organization_id is not None and context.project_id is not None
        assert context.actor.actor_id is not None
        org_id, project_id = context.organization_id, context.project_id

        row = (
            self._session.execute(
                text("""
                SELECT * FROM processing.jobs
                WHERE organization_id = :org AND project_id = :proj AND id = :jid
                FOR UPDATE
            """),
                {"org": org_id, "proj": project_id, "jid": job_id},
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError("processing job not found")
        job = _job_from_row(row)
        if job.error_code == "RECOVERY_QUARANTINED":
            raise PlatformConflictError(
                "historical restored processing job cannot resume; submit a new operation",
                code="RECOVERY_QUARANTINED",
            )
        if job.status in (ProcessingStatus.SUCCEEDED, ProcessingStatus.CANCELLED):
            return job

        # Keep the locked job and scope outside a savepoint. Any computation or
        # publication failure rolls back every output write; the job itself is
        # thereafter persisted as failed without exposing database internals.
        try:
            with self._session.begin_nested():
                # RLS authorization is scoped to the current savepoint. Rebind
                # with verified permission, never reuse outer transaction claims.
                self._prepare(context, permission=Permission.PROJECT_EDIT)
                self._session.execute(
                    text("SELECT set_config('statement_timeout', :timeout, true)"),
                    {"timeout": PROCESSING_STATEMENT_TIMEOUT},
                )
                done = self._execute_locked_job(context, job)
                # Validate deferred pin FKs within the savepoint, not after success.
                self._session.execute(text("SET CONSTRAINTS ALL IMMEDIATE"))
        except PlatformDomainError as exc:
            self._prepare(context, permission=Permission.PROJECT_EDIT)
            done = self._fail_job(job, exc.code, exc.message)
        except (SpatialValidationError, SQLAlchemyError):
            self._prepare(context, permission=Permission.PROJECT_EDIT)
            done = self._fail_job(
                job, "PROCESSING_FAILED", "processing could not safely publish output"
            )
        self._session.commit()
        return done

    def _fail_job(self, job: ProcessingJobRecord, code: str, message: str) -> ProcessingJobRecord:
        row = (
            self._session.execute(
                text("""
                UPDATE processing.jobs
                SET status='failed', error_code=:code, error_message=:message,
                    output_source_id=NULL, output_layer_id=NULL, output_content_revision=NULL,
                    feature_count=NULL, updated_at=:now, completed_at=:now
                WHERE organization_id=:org AND project_id=:proj AND id=:jid
                RETURNING *
            """),
                {
                    "org": job.organization_id,
                    "proj": job.project_id,
                    "jid": job.id,
                    "code": code[:64],
                    "message": message[:500],
                    "now": datetime.now(UTC),
                },
            )
            .mappings()
            .one()
        )
        return _job_from_row(row)

    def _execute_locked_job(
        self, context: AuthorizationContext, job: ProcessingJobRecord
    ) -> ProcessingJobRecord:
        org_id, project_id, job_id = job.organization_id, job.project_id, job.id
        input_source = self._validate_pinned_source(job, "input")
        if job.algorithm_id == AlgorithmId.CLIP:
            overlay_source = self._validate_pinned_source(job, "overlay")
            if overlay_source.geometry_family != GeometryFamily.POLYGON:
                raise PlatformValidationError(
                    "clip overlay must be Polygon", code="INVALID_OVERLAY"
                )
        now = datetime.now(UTC)
        self._session.execute(
            text("""
                UPDATE processing.jobs
                SET status = 'running', updated_at = :now, completed_at=NULL
                WHERE organization_id = :org AND project_id = :proj AND id = :jid
            """),
            {"org": org_id, "proj": project_id, "jid": job_id, "now": now},
        )

        input_src_id = UUID(str(job.input_refs["input_source_id"]))
        input_srid = int(job.input_refs["input_stored_crs"])

        # Compute derived geometries via PostGIS without mutating input sources
        derived_rows: list[tuple[str, dict[str, Any], bytes, NativeGeometry]] = []
        output_family = input_source.geometry_family
        bound_params: dict[str, Any] = {
            "max_features": MAX_PROCESSING_FEATURES,
            "max_positions": MAX_PROCESSING_POSITIONS,
            "max_rows": MAX_PROCESSING_FEATURES + 1,
        }
        output_fields: list[tuple[str, str, FieldType, bool]] = []

        if job.algorithm_id == AlgorithmId.BUFFER:
            dist = float(job.parameters["distance_meters"])
            quad_segs = int(job.parameters.get("quad_segs", 8))
            # Only approved analytic WGS84/Web Mercator pipeline is usable on
            # arbitrary geographic sources. Native metric inputs need no transform.
            if input_srid in (4326, 4756):
                projected, forward_params, _ = web_transform_sql(
                    "geometry", input_srid, 3857, "buffer_forward"
                )
                require_web_domain(
                    self._session,
                    organization_id=org_id,
                    project_id=project_id,
                    source_id=input_src_id,
                    native_srid=input_srid,
                )
                require_web_domain(
                    self._session,
                    organization_id=org_id,
                    project_id=project_id,
                    source_id=input_src_id,
                    native_srid=input_srid,
                    expression=projected,
                    expression_params=forward_params,
                    expression_srid=3857,
                    margin=dist,
                )
                projected, _, _ = web_transform_sql(
                    "rf.geometry", input_srid, 3857, "buffer_forward"
                )
                buf_expr, inverse_params, _ = web_transform_sql(
                    f"ST_Buffer({projected}, :dist, format('quad_segs=%s', :qsegs))",
                    3857,
                    input_srid,
                    "buffer_inverse",
                )
                bound_params.update(forward_params)
                bound_params.update(inverse_params)
            else:
                buf_expr = "ST_Buffer(rf.geometry, :dist, format('quad_segs=%s', :qsegs))"

            qrows = (
                self._session.execute(
                    text(f"""
                    WITH dumped AS (
                        SELECT rf.feature_id, part.geom, part.path
                        FROM spatial.reference_features rf
                        CROSS JOIN LATERAL ST_Dump(ST_MakeValid({buf_expr})) AS part
                        WHERE rf.organization_id = :org AND rf.project_id = :proj
                          AND rf.source_id = :src
                    )
                    SELECT feature_id, {_RESULT_GEOMETRY_COLUMNS}
                    FROM dumped
                    WHERE NOT ST_IsEmpty(geom)
                    ORDER BY feature_id, path LIMIT :max_rows
                """),
                    {
                        "org": org_id,
                        "proj": project_id,
                        "src": input_src_id,
                        "dist": dist,
                        "qsegs": quad_segs,
                        "srid": input_srid,
                        **bound_params,
                    },
                )
                .mappings()
                .all()
            )
            output_family = GeometryFamily.POLYGON
            output_fields = [
                ("source_feature_id", "Source Feature ID", FieldType.TEXT, False),
                ("buffer_meters", "Buffer Distance (m)", FieldType.DECIMAL, False),
            ]
            for idx, qr in enumerate(qrows):
                derived_rows.append(
                    (
                        f"buf_{idx + 1}",
                        {
                            "source_feature_id": str(qr["feature_id"]),
                            "buffer_meters": str(dist),
                        },
                        bytes(qr["wkb"]) if qr["wkb"] is not None else b"",
                        _bounded_native_geometry(qr, output_family, input_srid),
                    )
                )

        elif job.algorithm_id == AlgorithmId.CLIP:
            overlay_src_id = UUID(str(job.input_refs["overlay_source_id"]))
            overlay_srid = int(job.input_refs["overlay_stored_crs"])
            if overlay_srid == input_srid:
                mask_expr = "ST_Union(ov.geometry)"
            else:
                transformed, transform_params, _ = web_transform_sql(
                    "ov.geometry", overlay_srid, input_srid, "clip"
                )
                require_web_domain(
                    self._session,
                    organization_id=org_id,
                    project_id=project_id,
                    source_id=overlay_src_id,
                    native_srid=overlay_srid,
                )
                bound_params.update(transform_params)
                mask_expr = f"ST_Union({transformed})"
            qrows = (
                self._session.execute(
                    text(f"""
                    WITH mask AS (
                        SELECT {mask_expr} AS mgeom
                        FROM spatial.reference_features ov
                        WHERE ov.organization_id = :org AND ov.project_id = :proj
                          AND ov.source_id = :ov_src
                    ),
                    clipped AS (
                        SELECT rf.feature_id, part.geom, part.path
                        FROM spatial.reference_features rf CROSS JOIN mask
                        CROSS JOIN LATERAL ST_Dump(ST_Intersection(rf.geometry, mask.mgeom)) AS part
                        WHERE rf.organization_id = :org AND rf.project_id = :proj
                          AND rf.source_id = :src
                          AND mask.mgeom IS NOT NULL
                          AND ST_Intersects(rf.geometry, mask.mgeom)
                    )
                    SELECT feature_id, {_RESULT_GEOMETRY_COLUMNS}
                    FROM clipped
                    -- A polygon touching a mask along an edge is not a clipped
                    -- polygon. Keep the input dimension; discard lower-dimensional contacts.
                    WHERE ST_GeometryType(geom) = :gtype AND NOT ST_IsEmpty(geom)
                    ORDER BY feature_id, path LIMIT :max_rows
                """),
                    {
                        "org": org_id,
                        "proj": project_id,
                        "src": input_src_id,
                        "ov_src": overlay_src_id,
                        "srid": input_srid,
                        "gtype": f"ST_{output_family.value}",
                        **bound_params,
                    },
                )
                .mappings()
                .all()
            )
            output_fields = [
                ("source_feature_id", "Source Feature ID", FieldType.TEXT, False),
            ]
            for idx, qr in enumerate(qrows):
                derived_rows.append(
                    (
                        f"clip_{idx + 1}",
                        {"source_feature_id": str(qr["feature_id"])},
                        bytes(qr["wkb"]) if qr["wkb"] is not None else b"",
                        _bounded_native_geometry(qr, output_family, input_srid),
                    )
                )

        elif job.algorithm_id == AlgorithmId.DISSOLVE:
            group_field = job.parameters.get("group_by_field")
            if group_field:
                # Verify group field exists in source schema
                exists = self._session.execute(
                    text("""
                        SELECT 1 FROM spatial.source_fields
                        WHERE organization_id = :org AND project_id = :proj
                          AND source_id = :src AND field_id = :fid
                    """),
                    {"org": org_id, "proj": project_id, "src": input_src_id, "fid": group_field},
                ).first()
                if exists is None:
                    raise PlatformValidationError(
                        f"unknown dissolve group_by_field {group_field!r}",
                        code="UNKNOWN_FIELD",
                    )
                group_expr = "rf.attributes ->> CAST(:group_field AS text)"
            else:
                group_expr = "'all'"

            qrows = (
                self._session.execute(
                    text(f"""
                    WITH grouped AS (
                        SELECT {group_expr} AS grp_val,
                               COUNT(*)::int AS merged_count,
                               ST_MakeValid(ST_Union(rf.geometry)) AS ugeom
                        FROM spatial.reference_features rf
                        WHERE rf.organization_id = :org AND rf.project_id = :proj
                          AND rf.source_id = :src
                        GROUP BY 1
                    ),
                    dumped AS (
                        SELECT grp_val, merged_count, part.geom, part.path
                        FROM grouped CROSS JOIN LATERAL ST_Dump(ugeom) AS part
                    )
                    SELECT grp_val, merged_count, {_RESULT_GEOMETRY_COLUMNS}
                    FROM dumped
                    WHERE NOT ST_IsEmpty(geom)
                    ORDER BY grp_val NULLS FIRST, path LIMIT :max_rows
                """),
                    {
                        "org": org_id,
                        "proj": project_id,
                        "src": input_src_id,
                        "group_field": group_field,
                        **bound_params,
                    },
                )
                .mappings()
                .all()
            )
            output_fields = [
                ("group_value", "Group Value", FieldType.TEXT, True),
                ("merged_count", "Merged Feature Count", FieldType.INTEGER, False),
            ]
            for idx, qr in enumerate(qrows):
                derived_rows.append(
                    (
                        f"dis_{idx + 1}",
                        {
                            "group_value": qr["grp_val"],
                            "merged_count": int(qr["merged_count"]),
                        },
                        bytes(qr["wkb"]) if qr["wkb"] is not None else b"",
                        _bounded_native_geometry(qr, output_family, input_srid),
                    )
                )

        # Atomically publish a NEW project-owned data_source + layer
        out_source_id = uuid4()
        out_layer_id = uuid4()
        manifest_digest = hashlib.sha256(
            json.dumps(
                {"job_id": str(job_id), "refs": job.input_refs, "params": job.parameters},
                sort_keys=True,
            ).encode()
        ).hexdigest()
        dummy_rev = "0" * 64
        schema_fields = [
            SourceField(
                organization_id=org_id,
                project_id=project_id,
                source_id=out_source_id,
                field_id=fid,
                name=fname,
                ordinal=ordinal,
                type=ftype,
                nullable=nullable,
                schema_revision=dummy_rev,
            )
            for ordinal, (fid, fname, ftype, nullable) in enumerate(output_fields)
        ]
        # Enforce the same logical attribute contract as the pinned Core reader
        # before any derived source, layer, or feature is published.
        expected_fields = {field.field_id for field in schema_fields}
        canonical_rows = []
        try:
            for feat_id, attrs, wkb, geometry in derived_rows:
                if attrs.keys() != expected_fields:
                    raise SpatialValidationError("derived attributes do not match output schema")
                canonical_rows.append(
                    (
                        feat_id,
                        {
                            field.field_id: canonical_attribute_value(field, attrs[field.field_id])
                            for field in schema_fields
                        },
                        wkb,
                        geometry,
                    )
                )
        except SpatialValidationError as exc:
            raise PlatformValidationError(
                "invalid derived output attributes", code="INVALID_DERIVED_OUTPUT"
            ) from exc
        derived_rows = canonical_rows
        schema_rev = schema_revision_digest(schema_fields, output_family, GeometryDimensions.XY)
        # Content pins cover actual canonical attributes and native geometry,
        # not just job identity and an output row count.
        content_rev = content_revision_digest(
            [
                ReferenceFeature(
                    organization_id=org_id,
                    project_id=project_id,
                    source_id=out_source_id,
                    feature_id=feat_id,
                    attributes=attrs,
                    geometry=geometry,
                    geometry_state=GeometryState.PRESENT,
                    content_revision=dummy_rev,
                    schema_revision=schema_rev,
                )
                for feat_id, attrs, _, geometry in derived_rows
            ],
            schema_fields,
            input_srid,
            input_srid,
            geometry_family=output_family,
            dimensions=GeometryDimensions.XY,
        )
        ref_key = f"proc_{job.algorithm_id.value}_{str(out_source_id).replace('-', '')[:12]}"

        # Initialize as empty then compute exact extent before the transaction commits.
        init_ext_state = "empty"

        self._session.execute(
            text("""
                INSERT INTO spatial.data_sources (
                    id, organization_id, project_id, name, kind, reference_key,
                    fixture_version, manifest_digest, source_asset_id, lifecycle,
                    validation_state, validation_reason, source_crs, stored_crs,
                    schema_state, geometry_family, dimensions, feature_count_state,
                    feature_count, extent_state, extent, catalog_revision,
                    content_revision, schema_revision, created_at, updated_at, retired_at
                ) VALUES (
                    :id, :org, :proj, :name, 'derived_processing', :rkey,
                    :fver, :mdig, NULL, 'active',
                    'ready', NULL, :crs, :crs,
                    'known', :gfam, 'XY', 'known',
                    0, :estate, NULL, 1,
                    :crev, :srev, :now, :now, NULL
                )
            """),
            {
                "id": out_source_id,
                "org": org_id,
                "proj": project_id,
                "name": job.output_name,
                "rkey": ref_key,
                "fver": f"{job.algorithm_id.value}@{job.engine_version}",
                "mdig": manifest_digest,
                "crs": input_srid,
                "gfam": output_family.value,
                "estate": init_ext_state,
                "crev": content_rev,
                "srev": schema_rev,
                "now": now,
            },
        )

        for ordinal, (fid, fname, ftype, nullable) in enumerate(output_fields):
            self._session.execute(
                text("""
                    INSERT INTO spatial.source_fields (
                        organization_id, project_id, source_id, field_id,
                        name, ordinal, type, nullable, schema_revision
                    ) VALUES (:org, :proj, :src, :fid, :fname, :ord, :ftype, :null, :srev)
                """),
                {
                    "org": org_id,
                    "proj": project_id,
                    "src": out_source_id,
                    "fid": fid,
                    "fname": fname,
                    "ord": ordinal,
                    "ftype": ftype.value,
                    "null": nullable,
                    "srev": schema_rev,
                },
            )

        for feat_id, attrs, wkb, _ in derived_rows:
            self._session.execute(
                text("""
                    INSERT INTO spatial.reference_features (
                        organization_id, project_id, source_id, feature_id,
                        attributes, geometry, geometry_state, content_revision, schema_revision
                    ) VALUES (
                        :org, :proj, :src, :fid,
                        CAST(:attrs AS jsonb), ST_GeomFromWKB(:wkb, :srid), 'present', :crev, :srev
                    )
                """),
                {
                    "org": org_id,
                    "proj": project_id,
                    "src": out_source_id,
                    "fid": feat_id,
                    "attrs": json.dumps(attrs),
                    "wkb": wkb,
                    "srid": input_srid,
                    "crev": content_rev,
                    "srev": schema_rev,
                },
            )

        if derived_rows:
            stats = (
                self._session.execute(
                    text("""
                    SELECT COUNT(*)::int AS cnt,
                           ST_XMin(ST_Extent(geometry)) AS min_x,
                           ST_YMin(ST_Extent(geometry)) AS min_y,
                           ST_XMax(ST_Extent(geometry)) AS max_x,
                           ST_YMax(ST_Extent(geometry)) AS max_y
                    FROM spatial.reference_features
                    WHERE organization_id = :org AND project_id = :proj AND source_id = :src
                """),
                    {"org": org_id, "proj": project_id, "src": out_source_id},
                )
                .mappings()
                .one()
            )
            self._session.execute(
                text("""
                    UPDATE spatial.data_sources
                    SET feature_count = :cnt,
                        extent_state = 'available',
                        extent = CAST(:ext AS jsonb)
                    WHERE organization_id = :org AND project_id = :proj AND id = :src
                """),
                {
                    "org": org_id,
                    "proj": project_id,
                    "src": out_source_id,
                    "cnt": int(stats["cnt"]),
                    "ext": json.dumps(
                        {
                            "min_x": float(stats["min_x"]),
                            "min_y": float(stats["min_y"]),
                            "max_x": float(stats["max_x"]),
                            "max_y": float(stats["max_y"]),
                            "crs_code": int(input_srid),
                        }
                    ),
                },
            )

        self._session.execute(
            text("""
                INSERT INTO spatial.layers (
                    id, organization_id, project_id, source_id, name,
                    lifecycle, layer_revision, created_at, updated_at, retired_at
                ) VALUES (
                    :lid, :org, :proj, :src, :name,
                    'active', 1, :now, :now, NULL
                )
            """),
            {
                "lid": out_layer_id,
                "org": org_id,
                "proj": project_id,
                "src": out_source_id,
                "name": job.output_name,
                "now": now,
            },
        )

        audit_id = uuid4()
        append_project_audit(
            self._session,
            context,
            audit_id=audit_id,
            action="processing.job.succeeded",
            resource_type="data_source",
            resource_id=out_source_id,
            metadata={
                "job_id": str(job_id),
                "algorithm_id": job.algorithm_id.value,
                "feature_count": len(derived_rows),
            },
            created_at=now,
        )

        actor_label = context.actor.display_name or str(context.actor.actor_id)
        self._session.execute(
            text("""
                INSERT INTO spatial.catalog_changes (
                    id, organization_id, project_id, resource_type, resource_id,
                    source_id, layer_id, actor_type, actor_id, actor_display_snapshot,
                    operation, created_at, audit_event_id, prior_revision, new_revision, changes
                ) VALUES (
                    :id, :org, :proj, 'data_source', :src,
                    :src, NULL, :atype, :aid, :adisp,
                    'processing_publish', :now, :audit_id, NULL,
                    CAST(:new_rev AS jsonb), CAST(:changes AS jsonb)
                )
            """),
            {
                "id": uuid4(),
                "org": org_id,
                "proj": project_id,
                "src": out_source_id,
                "atype": str(context.actor.actor_type),
                "aid": context.actor.actor_id,
                "adisp": actor_label[:200],
                "now": now,
                "audit_id": audit_id,
                "new_rev": json.dumps(
                    {
                        "catalog_revision": 1,
                        "content_revision": content_rev,
                        "schema_revision": schema_rev,
                    }
                ),
                "changes": json.dumps(
                    [{"field": "name", "old_value": None, "new_value": job.output_name}]
                ),
            },
        )
        self._session.execute(
            text("""
                INSERT INTO spatial.catalog_changes (
                    id, organization_id, project_id, resource_type, resource_id,
                    source_id, layer_id, actor_type, actor_id, actor_display_snapshot,
                    operation, created_at, audit_event_id, prior_revision, new_revision, changes
                ) VALUES (
                    :id, :org, :proj, 'layer', :lid,
                    NULL, :lid, :atype, :aid, :adisp,
                    'create_layer', :now, :audit_id, NULL,
                    CAST(:new_rev AS jsonb), CAST(:changes AS jsonb)
                )
            """),
            {
                "id": uuid4(),
                "org": org_id,
                "proj": project_id,
                "lid": out_layer_id,
                "atype": str(context.actor.actor_type),
                "aid": context.actor.actor_id,
                "adisp": actor_label[:200],
                "now": now,
                "audit_id": audit_id,
                "new_rev": json.dumps({"layer_revision": 1}),
                "changes": json.dumps(
                    [{"field": "name", "old_value": None, "new_value": job.output_name}]
                ),
            },
        )

        done_at = datetime.now(UTC)
        done_row = (
            self._session.execute(
                text("""
                UPDATE processing.jobs
                SET status = 'succeeded',
                    output_source_id = :osrc,
                    output_layer_id = :olyr,
                    output_content_revision = :crev,
                    feature_count = :fcnt,
                    updated_at = :now,
                    completed_at = :now
                WHERE organization_id = :org AND project_id = :proj AND id = :jid
                RETURNING *
            """),
                {
                    "org": org_id,
                    "proj": project_id,
                    "jid": job_id,
                    "osrc": out_source_id,
                    "olyr": out_layer_id,
                    "crev": content_rev,
                    "fcnt": len(derived_rows),
                    "now": done_at,
                },
            )
            .mappings()
            .one()
        )
        return _job_from_row(done_row)

    def cancel_job(self, *, context: AuthorizationContext, job_id: UUID) -> ProcessingJobRecord:
        self._prepare(context, permission=Permission.PROJECT_EDIT)
        assert context.organization_id is not None and context.project_id is not None
        row = (
            self._session.execute(
                text("""
                SELECT * FROM processing.jobs
                WHERE organization_id = :org AND project_id = :proj AND id = :jid
                FOR UPDATE
            """),
                {"org": context.organization_id, "proj": context.project_id, "jid": job_id},
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError("processing job not found")
        if str(row["status"]) in ("succeeded", "cancelled"):
            raise PlatformConflictError(
                f"cannot cancel job in status {row['status']}", code="JOB_TERMINAL"
            )
        now = datetime.now(UTC)
        upd = (
            self._session.execute(
                text("""
                UPDATE processing.jobs
                SET status = 'cancelled', updated_at = :now, completed_at = :now
                WHERE organization_id = :org AND project_id = :proj AND id = :jid
                RETURNING *
            """),
                {
                    "org": context.organization_id,
                    "proj": context.project_id,
                    "jid": job_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        self._session.commit()
        return _job_from_row(upd)

    def retry_job(self, *, context: AuthorizationContext, job_id: UUID) -> ProcessingJobRecord:
        self._prepare(context, permission=Permission.PROJECT_EDIT)
        assert context.organization_id is not None and context.project_id is not None
        row = (
            self._session.execute(
                text("""
                SELECT * FROM processing.jobs
                WHERE organization_id = :org AND project_id = :proj AND id = :jid
                FOR UPDATE
            """),
                {"org": context.organization_id, "proj": context.project_id, "jid": job_id},
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError("processing job not found")
        if row["error_code"] == "RECOVERY_QUARANTINED":
            raise PlatformConflictError(
                "historical restored processing job cannot retry; submit a new operation",
                code="RECOVERY_QUARANTINED",
            )
        if str(row["status"]) == "succeeded":
            return _job_from_row(row)
        now = datetime.now(UTC)
        self._session.execute(
            text("""
                UPDATE processing.jobs
                SET status = 'queued', attempt_count = attempt_count + 1,
                    error_code = NULL, error_message = NULL, updated_at = :now, completed_at = NULL
                WHERE organization_id = :org AND project_id = :proj AND id = :jid
            """),
            {"org": context.organization_id, "proj": context.project_id, "jid": job_id, "now": now},
        )
        self._session.commit()
        return self.execute_job(context=context, job_id=job_id)

    def list_jobs(
        self, *, context: AuthorizationContext, limit: int = 50
    ) -> list[ProcessingJobRecord]:
        self._prepare(context, permission=Permission.PROJECT_READ)
        assert context.organization_id is not None and context.project_id is not None
        rows = (
            self._session.execute(
                text("""
                SELECT * FROM processing.jobs
                WHERE organization_id = :org AND project_id = :proj
                ORDER BY created_at DESC LIMIT :lim
            """),
                {
                    "org": context.organization_id,
                    "proj": context.project_id,
                    "lim": max(1, min(200, limit)),
                },
            )
            .mappings()
            .all()
        )
        return [_job_from_row(r) for r in rows]

    def get_job(self, *, context: AuthorizationContext, job_id: UUID) -> ProcessingJobRecord:
        self._prepare(context, permission=Permission.PROJECT_READ)
        assert context.organization_id is not None and context.project_id is not None
        row = (
            self._session.execute(
                text("""
                SELECT * FROM processing.jobs
                WHERE organization_id = :org AND project_id = :proj AND id = :jid
            """),
                {"org": context.organization_id, "proj": context.project_id, "jid": job_id},
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError("processing job not found")
        return _job_from_row(row)
