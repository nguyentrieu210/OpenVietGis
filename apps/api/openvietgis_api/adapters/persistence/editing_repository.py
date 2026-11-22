"""R006 RLS-enforced PostGIS repository for atomic feature editing & change history."""

from __future__ import annotations

import json
from collections.abc import Sequence
from datetime import UTC, datetime
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import text
from sqlalchemy.engine import RowMapping
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.gis_access import (
    deny,
    enable_internal_edit,
    feature_allowed,
    verify_edit,
)
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.adapters.persistence.spatial_repository import (
    SqlSpatialRepository,
    _source_from_row,
)
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.editing.domain import (
    ChangesetOperationRecord,
    ChangesetRecord,
    EditOperationInput,
    EditOpType,
    changeset_request_digest,
    compute_content_revision,
    geometry_to_wkt,
    validate_feature_id,
    validate_geometry_payload,
    validate_operation_id,
    validate_typed_attributes,
)
from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformForbiddenError,
    PlatformNotFoundError,
    PlatformValidationError,
)
from openvietgis_api.domains.spatial.domain import (
    FieldType,
    Lifecycle,
    SourceField,
    SourceKind,
    ValidationState,
)


def _changeset_from_row(
    row: RowMapping,
    operations: Sequence[ChangesetOperationRecord] = (),
) -> ChangesetRecord:
    return ChangesetRecord(
        id=row["id"],
        organization_id=row["organization_id"],
        project_id=row["project_id"],
        source_id=row["source_id"],
        actor_type=str(row["actor_type"]),
        actor_id=row["actor_id"],
        actor_display_snapshot=str(row["actor_display_snapshot"]),
        operation_id=str(row["operation_id"]),
        base_content_revision=str(row["base_content_revision"]),
        resulting_content_revision=str(row["resulting_content_revision"]),
        base_catalog_revision=int(row["base_catalog_revision"]),
        resulting_catalog_revision=int(row["resulting_catalog_revision"]),
        summary=str(row["summary"]),
        status=str(row["status"]),
        operation_count=int(row["operation_count"]),
        audit_event_id=row["audit_event_id"],
        reverted_from_changeset_id=row["reverted_from_changeset_id"],
        created_at=row["created_at"],
        operations=tuple(operations),
    )


class SqlEditingRepository:
    def __init__(self, session: Session) -> None:
        self._session = session
        self._spatial = SqlSpatialRepository(session)

    def _prepare(self, context: AuthorizationContext, *, permission: Permission) -> None:
        prepare_project_scope(self._session, context, permission=permission)

    def _load_fields(self, org_id: UUID, project_id: UUID, source_id: UUID) -> list[SourceField]:
        rows = (
            self._session.execute(
                text("""
                SELECT field_id, name, ordinal, type, nullable, schema_revision
                FROM spatial.source_fields
                WHERE organization_id = :org AND project_id = :proj AND source_id = :src
                ORDER BY ordinal ASC
            """),
                {"org": org_id, "proj": project_id, "src": source_id},
            )
            .mappings()
            .all()
        )
        return [
            SourceField(
                organization_id=org_id,
                project_id=project_id,
                source_id=source_id,
                field_id=str(r["field_id"]),
                name=str(r["name"]),
                ordinal=int(r["ordinal"]),
                type=FieldType(str(r["type"])),
                nullable=bool(r["nullable"]),
                schema_revision=str(r["schema_revision"]),
            )
            for r in rows
        ]

    def _load_operations(
        self, org_id: UUID, project_id: UUID, changeset_id: UUID
    ) -> list[ChangesetOperationRecord]:
        rows = (
            self._session.execute(
                text("""
                SELECT id, ordinal, op_type, feature_id,
                       CASE WHEN before_geometry_wkb IS NOT NULL
                            THEN ST_AsGeoJSON(ST_GeomFromEWKB(before_geometry_wkb))::jsonb
                            ELSE NULL END AS before_geom,
                       CASE WHEN after_geometry_wkb IS NOT NULL
                            THEN ST_AsGeoJSON(ST_GeomFromEWKB(after_geometry_wkb))::jsonb
                            ELSE NULL END AS after_geom,
                       before_properties, after_properties, created_at
                FROM editing.changeset_operations
                WHERE organization_id = :org AND project_id = :proj AND changeset_id = :cid
                ORDER BY ordinal ASC
            """),
                {"org": org_id, "proj": project_id, "cid": changeset_id},
            )
            .mappings()
            .all()
        )
        return [
            ChangesetOperationRecord(
                id=r["id"],
                ordinal=int(r["ordinal"]),
                op_type=EditOpType(str(r["op_type"])),
                feature_id=str(r["feature_id"]),
                before_geometry=dict(r["before_geom"]) if r["before_geom"] is not None else None,
                after_geometry=dict(r["after_geom"]) if r["after_geom"] is not None else None,
                before_properties=dict(r["before_properties"])
                if r["before_properties"] is not None
                else None,
                after_properties=dict(r["after_properties"])
                if r["after_properties"] is not None
                else None,
                created_at=r["created_at"],
            )
            for r in rows
        ]

    def commit_changeset(
        self,
        *,
        context: AuthorizationContext,
        source_id: UUID,
        base_content_revision: str,
        operation_id: str,
        summary: str,
        operations: Sequence[EditOperationInput],
        reverted_from_changeset_id: UUID | None = None,
        base_schema_revision: str | None = None,
    ) -> ChangesetRecord:
        self._prepare(context, permission=Permission.PROJECT_EDIT)
        assert context.organization_id is not None and context.project_id is not None
        assert context.actor.actor_id is not None
        org_id, project_id = context.organization_id, context.project_id
        restricted = enable_internal_edit(self._session, context, source_id)
        if restricted and reverted_from_changeset_id is not None:
            deny()
        clean_op_id = validate_operation_id(operation_id)
        clean_summary = summary.strip()[:500] or "Feature changeset"
        if len(operations) > 2000:
            raise PlatformValidationError(
                "changeset exceeds maximum 2000 operations", code="CHANGESET_TOO_LARGE"
            )
        request_digest = changeset_request_digest(
            base_content_revision=base_content_revision,
            summary=clean_summary,
            operations=operations,
            reverted_from_changeset_id=reverted_from_changeset_id,
            base_schema_revision=base_schema_revision,
        )
        # Serialize replay lookup with source mutation so concurrent identical
        # retries observe the winner's receipt instead of a stale-revision error.
        src_row = (
            self._session.execute(
                text("""
                SELECT * FROM spatial.data_sources
                WHERE organization_id = :org AND project_id = :proj AND id = :src
                FOR UPDATE
            """),
                {"org": org_id, "proj": project_id, "src": source_id},
            )
            .mappings()
            .first()
        )
        if src_row is None:
            raise PlatformNotFoundError("data source not found")
        # The immutable audit row binds the exact request; legacy rows without a
        # digest cannot safely be re-acknowledged from a different payload.
        existing = (
            self._session.execute(
                text("""
                SELECT cs.*, ae.metadata->>'request_sha256' AS request_digest
                FROM editing.changesets cs JOIN audit.audit_events ae
                  ON ae.organization_id = cs.organization_id AND ae.id = cs.audit_event_id
                WHERE cs.organization_id = :org AND cs.project_id = :proj
                  AND cs.source_id = :src AND cs.operation_id = :op
            """),
                {"org": org_id, "proj": project_id, "src": source_id, "op": clean_op_id},
            )
            .mappings()
            .first()
        )
        if existing is not None:
            if (
                existing["actor_id"] != context.actor.actor_id
                or existing["actor_type"] != str(context.actor.actor_type)
                or existing["request_digest"] != request_digest
            ):
                raise PlatformConflictError(
                    "operation_id already committed with different or unverified request identity",
                    code="OPERATION_ID_CONFLICT",
                )
            if restricted:
                replay = self._session.execute(
                    text(
                        "SELECT feature_id,op_type,before_geometry_wkb AS "
                        "before_wkb,after_geometry_wkb AS after_wkb,before_prope"
                        "rties AS before_props,after_properties AS after_props "
                        "FROM editing.changeset_operations WHERE "
                        "organization_id=:org AND project_id=:proj AND changeset_id=:cid"
                    ),
                    {"org": org_id, "proj": project_id, "cid": existing["id"]},
                ).mappings()
                for item in replay:
                    verify_edit(self._session, context, source_id, item)
            return _changeset_from_row(
                existing, self._load_operations(org_id, project_id, existing["id"])
            )

        if not operations:
            raise PlatformValidationError(
                "changeset requires at least one operation", code="EMPTY_CHANGESET"
            )
        source = _source_from_row(src_row)
        if source.lifecycle != Lifecycle.ACTIVE or source.validation_state != ValidationState.READY:
            raise PlatformValidationError("source is not active and ready", code="SOURCE_NOT_READY")
        if source.kind == SourceKind.REFERENCE_FIXTURE:
            raise PlatformForbiddenError(
                "reference fixture sources are read-only; import or derive a source to edit",
                code="SOURCE_READ_ONLY",
            )
        assert source.stored_crs is not None

        if source.content_revision != base_content_revision:
            raise PlatformConflictError(
                "stale content revision; refresh and reapply your local draft",
                code="STALE_CONTENT_REVISION",
                details={
                    "expected_content_revision": base_content_revision,
                    "current_content_revision": source.content_revision,
                    "current_catalog_revision": source.catalog_revision,
                    "draft_preserved": True,
                },
            )

        if base_schema_revision is not None and source.schema_revision != base_schema_revision:
            raise PlatformConflictError(
                "stale schema revision; refresh and review field identities before reapplying",
                code="STALE_SCHEMA_REVISION",
                details={
                    "expected_schema_revision": base_schema_revision,
                    "current_schema_revision": source.schema_revision,
                    "draft_preserved": True,
                },
            )
        fields = self._load_fields(org_id, project_id, source_id)
        if any(field.schema_revision != source.schema_revision for field in fields):
            raise PlatformConflictError(
                "source field schema does not match its current revision",
                code="STALE_SCHEMA_REVISION",
            )
        now = datetime.now(UTC)

        # Snapshot and validate every operation before mutating
        prepared_ops: list[dict[str, Any]] = []
        fingerprint: list[dict[str, Any]] = []
        seen_ids: set[str] = set()

        for ordinal, op in enumerate(operations):
            fid = validate_feature_id(op.feature_id)
            if fid in seen_ids:
                raise PlatformValidationError(
                    f"duplicate feature_id {fid!r} in same changeset",
                    code="DUPLICATE_FEATURE_OP",
                )
            seen_ids.add(fid)

            cur_row = (
                self._session.execute(
                    text("""
                    SELECT feature_id, attributes, ST_AsEWKB(geometry) AS geom_wkb
                    FROM spatial.reference_features
                    WHERE organization_id = :org AND project_id = :proj
                      AND source_id = :src AND feature_id = :fid
                """),
                    {"org": org_id, "proj": project_id, "src": source_id, "fid": fid},
                )
                .mappings()
                .first()
            )

            if (
                restricted
                and cur_row is not None
                and not feature_allowed(
                    self._session,
                    context,
                    source_id,
                    fid,
                    cur_row["attributes"],
                    cur_row["geom_wkb"],
                )
            ):
                deny()
            if restricted and cur_row is None and op.op_type != EditOpType.CREATE:
                deny()
            if op.op_type == EditOpType.CREATE:
                if cur_row is not None:
                    raise PlatformConflictError(
                        f"feature {fid!r} already exists", code="FEATURE_EXISTS"
                    )
                validate_geometry_payload(op.geometry, source.geometry_family)
                assert op.geometry is not None
                wkt = geometry_to_wkt(op.geometry)
                after_wkb = self._validate_and_make_wkb(wkt, source.stored_crs)
                clean_props = validate_typed_attributes(op.properties, fields)
                prepared_ops.append(
                    {
                        "ordinal": ordinal,
                        "op_type": op.op_type.value,
                        "feature_id": fid,
                        "before_wkb": None,
                        "after_wkb": after_wkb,
                        "before_props": None,
                        "after_props": clean_props,
                    }
                )
                fingerprint.append({"op": "create", "fid": fid, "props": clean_props, "wkt": wkt})

            elif op.op_type == EditOpType.UPDATE:
                if cur_row is None:
                    raise PlatformNotFoundError(f"feature {fid!r} not found")
                before_wkb = bytes(cur_row["geom_wkb"]) if cur_row["geom_wkb"] is not None else None
                before_props = dict(cur_row["attributes"])
                after_wkb_opt: bytes | None
                if op.geometry is not None:
                    validate_geometry_payload(op.geometry, source.geometry_family)
                    wkt = geometry_to_wkt(op.geometry)
                    after_wkb_opt = self._validate_and_make_wkb(wkt, source.stored_crs)
                else:
                    after_wkb_opt = before_wkb
                    wkt = "UNCHANGED"
                merged_props = (
                    validate_typed_attributes(op.properties, fields)
                    if op.properties is not None
                    else before_props
                )
                prepared_ops.append(
                    {
                        "ordinal": ordinal,
                        "op_type": op.op_type.value,
                        "feature_id": fid,
                        "before_wkb": before_wkb,
                        "after_wkb": after_wkb_opt,
                        "before_props": before_props,
                        "after_props": merged_props,
                    }
                )
                fingerprint.append({"op": "update", "fid": fid, "props": merged_props, "wkt": wkt})

            elif op.op_type == EditOpType.DELETE:
                if cur_row is None:
                    raise PlatformNotFoundError(f"feature {fid!r} not found")
                before_wkb = bytes(cur_row["geom_wkb"]) if cur_row["geom_wkb"] is not None else None
                before_props = dict(cur_row["attributes"])
                prepared_ops.append(
                    {
                        "ordinal": ordinal,
                        "op_type": op.op_type.value,
                        "feature_id": fid,
                        "before_wkb": before_wkb,
                        "after_wkb": None,
                        "before_props": before_props,
                        "after_props": None,
                    }
                )
                fingerprint.append({"op": "delete", "fid": fid})

        if restricted:
            for item in prepared_ops:
                verify_edit(self._session, context, source_id, item)
        if all(
            item["op_type"] == "update"
            and item["before_wkb"] == item["after_wkb"]
            and item["before_props"] == item["after_props"]
            for item in prepared_ops
        ):
            raise PlatformConflictError(
                "changeset does not contain a material change", code="NO_MATERIAL_CHANGE"
            )

        new_content_rev = compute_content_revision(source_id, source.content_revision, fingerprint)
        new_catalog_rev = source.catalog_revision + 1

        # Apply feature mutations (fk_features_pins is DEFERRABLE INITIALLY DEFERRED)
        for item in prepared_ops:
            if item["op_type"] == "create":
                self._session.execute(
                    text("""
                        INSERT INTO spatial.reference_features (
                            organization_id, project_id, source_id, feature_id,
                            attributes, geometry, geometry_state, content_revision, schema_revision
                        ) VALUES (
                            :org, :proj, :src, :fid,
                            CAST(:attrs AS jsonb), ST_GeomFromEWKB(:wkb), 'present', :crev, :srev
                        )
                    """),
                    {
                        "org": org_id,
                        "proj": project_id,
                        "src": source_id,
                        "fid": item["feature_id"],
                        "attrs": json.dumps(item["after_props"]),
                        "wkb": item["after_wkb"],
                        "crev": new_content_rev,
                        "srev": source.schema_revision,
                    },
                )
            elif item["op_type"] == "update":
                self._session.execute(
                    text("""
                        UPDATE spatial.reference_features
                        SET attributes = CAST(:attrs AS jsonb),
                            geometry = ST_GeomFromEWKB(:wkb),
                            content_revision = :crev
                        WHERE organization_id = :org AND project_id = :proj
                          AND source_id = :src AND feature_id = :fid
                    """),
                    {
                        "org": org_id,
                        "proj": project_id,
                        "src": source_id,
                        "fid": item["feature_id"],
                        "attrs": json.dumps(item["after_props"]),
                        "wkb": item["after_wkb"],
                        "crev": new_content_rev,
                    },
                )
            elif item["op_type"] == "delete":
                self._session.execute(
                    text("""
                        DELETE FROM spatial.reference_features
                        WHERE organization_id = :org AND project_id = :proj
                          AND source_id = :src AND feature_id = :fid
                    """),
                    {
                        "org": org_id,
                        "proj": project_id,
                        "src": source_id,
                        "fid": item["feature_id"],
                    },
                )

        # Bump content_revision on all untouched features for this source
        self._session.execute(
            text("""
                UPDATE spatial.reference_features
                SET content_revision = :crev
                WHERE organization_id = :org AND project_id = :proj AND source_id = :src
                  AND content_revision != :crev
            """),
            {"org": org_id, "proj": project_id, "src": source_id, "crev": new_content_rev},
        )

        # Recompute feature count & extent cleanly
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
                {"org": org_id, "proj": project_id, "src": source_id},
            )
            .mappings()
            .one()
        )
        new_count = int(stats["cnt"])
        if new_count == 0:
            extent_state = "empty"
            extent_json: str | None = None
        else:
            extent_state = "available"
            extent_json = json.dumps(
                {
                    "min_x": float(stats["min_x"]),
                    "min_y": float(stats["min_y"]),
                    "max_x": float(stats["max_x"]),
                    "max_y": float(stats["max_y"]),
                    "crs_code": int(source.stored_crs),
                }
            )

        self._session.execute(
            text("""
                UPDATE spatial.data_sources
                SET content_revision = :crev,
                    catalog_revision = :cat_rev,
                    feature_count = :cnt,
                    extent_state = :ext_state,
                    extent = CAST(:ext AS jsonb),
                    updated_at = :now
                WHERE organization_id = :org AND project_id = :proj AND id = :src
            """),
            {
                "org": org_id,
                "proj": project_id,
                "src": source_id,
                "crev": new_content_rev,
                "cat_rev": new_catalog_rev,
                "cnt": new_count,
                "ext_state": extent_state,
                "ext": extent_json,
                "now": now,
            },
        )

        # Audit event
        audit_id = uuid4()
        append_project_audit(
            self._session,
            context,
            audit_id=audit_id,
            action="editing.changeset.commit",
            resource_type="data_source",
            resource_id=source_id,
            metadata={
                "operation_id": clean_op_id,
                "request_sha256": request_digest,
                "operation_count": len(prepared_ops),
                "base_content_revision": source.content_revision,
                "base_schema_revision": source.schema_revision,
                "submitted_schema_revision": base_schema_revision,
                "resulting_content_revision": new_content_rev,
            },
            created_at=now,
        )

        # Record spatial.catalog_changes entry
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
                    'edit_features', :now, :audit_id,
                    CAST(:prior_rev AS jsonb), CAST(:new_rev AS jsonb), CAST(:changes AS jsonb)
                )
            """),
            {
                "id": uuid4(),
                "org": org_id,
                "proj": project_id,
                "src": source_id,
                "atype": str(context.actor.actor_type),
                "aid": context.actor.actor_id,
                "adisp": actor_label[:200],
                "now": now,
                "audit_id": audit_id,
                "prior_rev": json.dumps(
                    {
                        "catalog_revision": source.catalog_revision,
                        "content_revision": source.content_revision,
                        "schema_revision": source.schema_revision,
                    }
                ),
                "new_rev": json.dumps(
                    {
                        "catalog_revision": new_catalog_rev,
                        "content_revision": new_content_rev,
                        "schema_revision": source.schema_revision,
                    }
                ),
                "changes": json.dumps(
                    [
                        {
                            "field": "content_revision",
                            "old_value": source.content_revision,
                            "new_value": new_content_rev,
                        },
                    ]
                ),
            },
        )

        # Insert immutable changeset + operations
        changeset_id = uuid4()
        cs_row = (
            self._session.execute(
                text("""
                INSERT INTO editing.changesets (
                    id, organization_id, project_id, source_id,
                    actor_type, actor_id, actor_display_snapshot, operation_id,
                    base_content_revision, resulting_content_revision,
                    base_catalog_revision, resulting_catalog_revision,
                    summary, status, operation_count, audit_event_id,
                    reverted_from_changeset_id, created_at
                ) VALUES (
                    :id, :org, :proj, :src,
                    :atype, :aid, :adisp, :op_id,
                    :base_crev, :res_crev,
                    :base_cat, :res_cat,
                    :summary, :status, :op_cnt, :audit_id,
                    :rev_from, :now
                )
                RETURNING *
            """),
                {
                    "id": changeset_id,
                    "org": org_id,
                    "proj": project_id,
                    "src": source_id,
                    "atype": str(context.actor.actor_type),
                    "aid": context.actor.actor_id,
                    "adisp": actor_label[:200],
                    "op_id": clean_op_id,
                    "base_crev": source.content_revision,
                    "res_crev": new_content_rev,
                    "base_cat": source.catalog_revision,
                    "res_cat": new_catalog_rev,
                    "summary": clean_summary,
                    "status": "reverted" if reverted_from_changeset_id else "committed",
                    "op_cnt": len(prepared_ops),
                    "audit_id": audit_id,
                    "rev_from": reverted_from_changeset_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )

        for item in prepared_ops:
            self._session.execute(
                text("""
                    INSERT INTO editing.changeset_operations (
                        id, organization_id, project_id, changeset_id, ordinal,
                        op_type, feature_id, before_geometry_wkb, after_geometry_wkb,
                        before_properties, after_properties, created_at
                    ) VALUES (
                        :id, :org, :proj, :cid, :ord,
                        :op_type, :fid, :bwkb, :awkb,
                        CAST(:bprops AS jsonb), CAST(:aprops AS jsonb), :now
                    )
                """),
                {
                    "id": uuid4(),
                    "org": org_id,
                    "proj": project_id,
                    "cid": changeset_id,
                    "ord": item["ordinal"],
                    "op_type": item["op_type"],
                    "fid": item["feature_id"],
                    "bwkb": item["before_wkb"],
                    "awkb": item["after_wkb"],
                    "bprops": json.dumps(item["before_props"])
                    if item["before_props"] is not None
                    else None,
                    "aprops": json.dumps(item["after_props"])
                    if item["after_props"] is not None
                    else None,
                    "now": now,
                },
            )

        ops_records = self._load_operations(org_id, project_id, changeset_id)
        self._session.commit()
        return _changeset_from_row(cs_row, ops_records)

    def _validate_and_make_wkb(self, wkt: str, srid: int) -> bytes:
        row = (
            self._session.execute(
                text("""
                WITH g AS (SELECT ST_SetSRID(ST_GeomFromText(:wkt), :srid) AS geom)
                SELECT ST_IsValid(geom) AS valid,
                       ST_IsValidReason(geom) AS reason,
                       ST_NPoints(geom) AS npts,
                       ST_AsEWKB(geom) AS wkb
                FROM g
            """),
                {"wkt": wkt, "srid": srid},
            )
            .mappings()
            .one()
        )
        if not row["valid"]:
            raise PlatformValidationError(
                f"invalid geometry rejected (no silent repair): {row['reason']}",
                code="INVALID_GEOMETRY",
            )
        if int(row["npts"]) > 4096:
            raise PlatformValidationError(
                "geometry exceeds 4096 vertex limit", code="BOUNDS_EXCEEDED"
            )
        return bytes(row["wkb"])

    def list_changesets(
        self,
        *,
        context: AuthorizationContext,
        source_id: UUID,
        limit: int = 50,
    ) -> list[ChangesetRecord]:
        self._prepare(context, permission=Permission.PROJECT_READ)
        assert context.organization_id is not None and context.project_id is not None
        rows = (
            self._session.execute(
                text("""
                SELECT * FROM editing.changesets
                WHERE organization_id = :org AND project_id = :proj AND source_id = :src
                ORDER BY created_at DESC, resulting_catalog_revision DESC
                LIMIT :lim
            """),
                {
                    "org": context.organization_id,
                    "proj": context.project_id,
                    "src": source_id,
                    "lim": max(1, min(200, limit)),
                },
            )
            .mappings()
            .all()
        )
        return [_changeset_from_row(r) for r in rows]

    def get_changeset(
        self,
        *,
        context: AuthorizationContext,
        changeset_id: UUID,
    ) -> ChangesetRecord:
        self._prepare(context, permission=Permission.PROJECT_READ)
        assert context.organization_id is not None and context.project_id is not None
        row = (
            self._session.execute(
                text("""
                SELECT * FROM editing.changesets
                WHERE organization_id = :org AND project_id = :proj AND id = :cid
            """),
                {
                    "org": context.organization_id,
                    "proj": context.project_id,
                    "cid": changeset_id,
                },
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError("changeset not found")
        ops = self._load_operations(context.organization_id, context.project_id, changeset_id)
        return _changeset_from_row(row, ops)

    def revert_changeset(
        self,
        *,
        context: AuthorizationContext,
        changeset_id: UUID,
        operation_id: str,
        summary: str | None = None,
    ) -> ChangesetRecord:
        target = self.get_changeset(context=context, changeset_id=changeset_id)
        inverse_ops: list[EditOperationInput] = []
        for op in reversed(target.operations):
            if op.op_type == EditOpType.CREATE:
                inverse_ops.append(EditOperationInput(EditOpType.DELETE, op.feature_id))
            elif op.op_type == EditOpType.DELETE:
                inverse_ops.append(
                    EditOperationInput(
                        EditOpType.CREATE,
                        op.feature_id,
                        geometry=op.before_geometry,
                        properties=op.before_properties,
                    )
                )
            elif op.op_type == EditOpType.UPDATE:
                inverse_ops.append(
                    EditOperationInput(
                        EditOpType.UPDATE,
                        op.feature_id,
                        geometry=op.before_geometry,
                        properties=op.before_properties,
                    )
                )
        return self.commit_changeset(
            context=context,
            source_id=target.source_id,
            base_content_revision=target.resulting_content_revision,
            operation_id=operation_id,
            summary=summary or f"Revert changeset {target.id}",
            operations=inverse_ops,
            reverted_from_changeset_id=target.id,
        )
