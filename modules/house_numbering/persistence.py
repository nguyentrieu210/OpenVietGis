"""Typed RLS persistence for the House Numbering official module."""

from __future__ import annotations

import hashlib
import json
from datetime import UTC, datetime
from typing import Any
from uuid import UUID, uuid4

from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformNotFoundError,
    PlatformValidationError,
)
from openvietgis_api.module_sdk import (
    AuthorizationContext,
    append_project_audit,
    prepare_module_scope,
    set_extension_private_access,
)
from sqlalchemy import text
from sqlalchemy.engine import RowMapping
from sqlalchemy.orm import Session
from modules.house_numbering.address_prefix import canonical_prefix
from modules.house_numbering.numbering_audit import find_tree_collisions



class HouseNumberingRepository:
    def __init__(self, session: Session, context: AuthorizationContext) -> None:
        self.session = session
        self.context = context

    @property
    def org_id(self) -> UUID:
        assert self.context.organization_id is not None
        return self.context.organization_id

    @property
    def project_id(self) -> UUID:
        assert self.context.project_id is not None
        return self.context.project_id

    @property
    def actor_id(self) -> UUID:
        assert self.context.actor.actor_id is not None
        return self.context.actor.actor_id

    def prepare(self, access: str = "read") -> None:
        if access not in {"read", "edit", "manage"}:
            raise ValueError("invalid module access level")
        prepare_module_scope(self.session, self.context, access=access)  # type: ignore[arg-type]
        active = self.session.execute(
            text("""
                SELECT 1 FROM extensions.project_activations
                WHERE organization_id=:org AND project_id=:project
                  AND extension_key='house_numbering' AND status='active'
            """),
            {"org": self.org_id, "project": self.project_id},
        ).first()
        if active is None:
            raise PlatformNotFoundError(
                "house_numbering module is not active in this project"
            )

    def audit(
        self,
        action: str,
        resource_type: str,
        resource_id: UUID,
        metadata: dict[str, Any],
    ) -> None:
        append_project_audit(
            self.session,
            self.context,
            audit_id=uuid4(),
            action=action,
            resource_type=resource_type,
            resource_id=resource_id,
            metadata=metadata,
            created_at=datetime.now(UTC),
        )

    def spatial_feature(
        self, source_id: UUID, feature_id: str, *, expected: set[str]
    ) -> dict[str, Any]:
        row = (
            self.session.execute(
                text("""
                SELECT ds.stored_crs, ds.lifecycle, ds.validation_state,
                       ST_SRID(rf.geometry) AS srid,
                       ST_GeometryType(rf.geometry) AS geometry_type,
                       ST_AsGeoJSON(rf.geometry)::jsonb AS geometry
                FROM spatial.data_sources ds
                JOIN spatial.reference_features rf
                  ON rf.organization_id=ds.organization_id
                 AND rf.project_id=ds.project_id
                 AND rf.source_id=ds.id
                WHERE ds.organization_id=:org AND ds.project_id=:project
                  AND ds.id=:source AND rf.feature_id=:feature
                  AND rf.geometry_state='present'
            """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "source": source_id,
                    "feature": feature_id,
                },
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError(
                "linked spatial feature not found or not usable"
            )
        if row["lifecycle"] != "active" or row["validation_state"] != "ready":
            raise PlatformValidationError(
                "linked spatial source is not ready", code="SOURCE_NOT_READY"
            )
        if str(row["geometry_type"]) not in expected:
            raise PlatformValidationError(
                f"linked feature geometry {row['geometry_type']!r} is not valid for this entity",
                code="GEOMETRY_TYPE_MISMATCH",
            )
        srid = int(row["srid"] or row["stored_crs"] or 0)
        if srid <= 0:
            raise PlatformValidationError(
                "linked geometry has no explicit CRS", code="MISSING_CRS"
            )
        return {
            "srid": srid,
            "geometry_type": str(row["geometry_type"]),
            "geometry": dict(row["geometry"]),
        }

    def create_rule(
        self,
        *,
        version: str,
        status: str,
        authority_ref: str | None,
        left_odd: bool,
        suffix_style: str,
        max_distance_m: float,
    ) -> RowMapping:
        self.prepare("manage")
        now = datetime.now(UTC)
        row = (
            self.session.execute(
                text("""
                INSERT INTO house_numbering.rules (
                    id, organization_id, project_id, version, status, authority_ref,
                    left_odd, suffix_style, max_distance_m, sealed, revision,
                    created_by_actor_id, updated_by_actor_id, created_at, updated_at
                ) VALUES (
                    :id,:org,:project,:version,:status,:authority,
                    :left_odd,:suffix,:distance,true,1,:actor,:actor,:now,:now
                ) RETURNING *
            """),
                {
                    "id": uuid4(),
                    "org": self.org_id,
                    "project": self.project_id,
                    "version": version,
                    "status": status,
                    "authority": authority_ref,
                    "left_odd": left_odd,
                    "suffix": suffix_style,
                    "distance": max_distance_m,
                    "actor": self.actor_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        self.audit(
            "house_numbering.rule.create",
            "house_numbering_rule",
            row["id"],
            {"version": version},
        )
        return row

    def list_rules(self) -> list[RowMapping]:
        self.prepare("read")
        return (
            self.session.execute(
                text("""
                SELECT * FROM house_numbering.rules
                WHERE organization_id=:org AND project_id=:project
                ORDER BY created_at DESC
            """),
                {"org": self.org_id, "project": self.project_id},
            )
            .mappings()
            .all()
        )

    def get_rule(self, rule_id: UUID) -> RowMapping:
        self.prepare("read")
        row = (
            self.session.execute(
                text("""
                SELECT * FROM house_numbering.rules
                WHERE organization_id=:org AND project_id=:project AND id=:id
            """),
                {"org": self.org_id, "project": self.project_id, "id": rule_id},
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError("numbering rule not found")
        return row

    def create_area(
        self,
        *,
        name: str,
        kind: str,
        parent_id: UUID | None,
        source_id: UUID | None,
        feature_id: str | None,
    ) -> RowMapping:
        self.prepare("edit")
        if (source_id is None) != (feature_id is None):
            raise PlatformValidationError(
                "area spatial source and feature must be supplied together"
            )
        if source_id is not None and feature_id is not None:
            self.spatial_feature(source_id, feature_id, expected={"ST_Polygon"})
        now = datetime.now(UTC)
        row = (
            self.session.execute(
                text("""
                INSERT INTO house_numbering.areas (
                    id,organization_id,project_id,name,kind,parent_id,
                    linked_source_id,linked_feature_id,revision,
                    created_by_actor_id,updated_by_actor_id,created_at,updated_at
                ) VALUES (
                    :id,:org,:project,:name,:kind,:parent,:source,:feature,1,
                    :actor,:actor,:now,:now
                ) RETURNING *
            """),
                {
                    "id": uuid4(),
                    "org": self.org_id,
                    "project": self.project_id,
                    "name": name.strip(),
                    "kind": kind,
                    "parent": parent_id,
                    "source": source_id,
                    "feature": feature_id,
                    "actor": self.actor_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        self.audit(
            "house_numbering.area.create",
            "house_numbering_area",
            row["id"],
            {"name": row["name"]},
        )
        return row

    def list_areas(self) -> list[RowMapping]:
        self.prepare("read")
        return (
            self.session.execute(
                text("""
                SELECT * FROM house_numbering.areas
                WHERE organization_id=:org AND project_id=:project
                ORDER BY name,id
            """),
                {"org": self.org_id, "project": self.project_id},
            )
            .mappings()
            .all()
        )

    def get_area(self, area_id: UUID, *, for_update: bool = False) -> RowMapping:
        self.prepare("edit" if for_update else "read")
        lock = " FOR UPDATE" if for_update else ""
        row = (
            self.session.execute(
                text(
                    """
                    SELECT * FROM house_numbering.areas
                    WHERE organization_id=:org AND project_id=:project AND id=:id
                    """
                    + lock
                ),
                {"org": self.org_id, "project": self.project_id, "id": area_id},
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError("area not found")
        return row

    def update_area(
        self,
        area_id: UUID,
        *,
        expected_revision: int,
        name: str,
        kind: str,
        parent_id: UUID | None,
        source_id: UUID | None,
        feature_id: str | None,
    ) -> RowMapping:
        current = self.get_area(area_id, for_update=True)
        if int(current["revision"]) != expected_revision:
            raise PlatformConflictError("area revision is stale", code="STALE_AREA")
        if parent_id == area_id:
            raise PlatformValidationError(
                "area cannot be its own parent", code="AREA_PARENT"
            )
        if (source_id is None) != (feature_id is None):
            raise PlatformValidationError(
                "area spatial source and feature must be supplied together"
            )
        if source_id is not None and feature_id is not None:
            self.spatial_feature(source_id, feature_id, expected={"ST_Polygon"})
        if parent_id is not None:
            self.get_area(parent_id)
            cycle = self.session.execute(
                text("""
                    WITH RECURSIVE descendants AS (
                        SELECT id FROM house_numbering.areas
                        WHERE organization_id=:org AND project_id=:project
                          AND parent_id=:area
                        UNION ALL
                        SELECT child.id
                        FROM house_numbering.areas child
                        JOIN descendants parent ON child.parent_id=parent.id
                        WHERE child.organization_id=:org AND child.project_id=:project
                    )
                    SELECT 1 FROM descendants WHERE id=:parent LIMIT 1
                """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "area": area_id,
                    "parent": parent_id,
                },
            ).first()
            if cycle is not None:
                raise PlatformConflictError(
                    "area parent would create a cycle", code="AREA_PARENT_CYCLE"
                )
        now = datetime.now(UTC)
        row = (
            self.session.execute(
                text("""
                    UPDATE house_numbering.areas
                    SET name=:name,kind=:kind,parent_id=:parent,
                        linked_source_id=:source,linked_feature_id=:feature,
                        revision=revision+1,updated_by_actor_id=:actor,updated_at=:now
                    WHERE organization_id=:org AND project_id=:project AND id=:id
                    RETURNING *
                """),
                {
                    "name": name.strip(),
                    "kind": kind,
                    "parent": parent_id,
                    "source": source_id,
                    "feature": feature_id,
                    "actor": self.actor_id,
                    "now": now,
                    "org": self.org_id,
                    "project": self.project_id,
                    "id": area_id,
                },
            )
            .mappings()
            .one()
        )
        self.audit(
            "house_numbering.area.update",
            "house_numbering_area",
            area_id,
            {
                "before": {
                    "name": current["name"],
                    "kind": current["kind"],
                    "parent_id": str(current["parent_id"])
                    if current["parent_id"]
                    else None,
                    "linked_source_id": str(current["linked_source_id"])
                    if current["linked_source_id"]
                    else None,
                    "linked_feature_id": current["linked_feature_id"],
                },
                "after_revision": int(row["revision"]),
            },
        )
        return row

    def delete_area(self, area_id: UUID, *, expected_revision: int) -> None:
        current = self.get_area(area_id, for_update=True)
        if int(current["revision"]) != expected_revision:
            raise PlatformConflictError("area revision is stale", code="STALE_AREA")
        dependency = (
            self.session.execute(
                text("""
                SELECT kind FROM (
                    SELECT 'child_area' AS kind FROM house_numbering.areas
                    WHERE organization_id=:org AND project_id=:project AND parent_id=:area
                    UNION ALL
                    SELECT 'building_history' AS kind FROM house_numbering.buildings
                    WHERE organization_id=:org AND project_id=:project
                      AND area_id=:area
                ) dependencies LIMIT 1
            """),
                {"org": self.org_id, "project": self.project_id, "area": area_id},
            )
            .mappings()
            .first()
        )
        if dependency is not None:
            raise PlatformConflictError(
                f"area has dependent {dependency['kind']}", code="AREA_HAS_DEPENDENCIES"
            )
        self.session.execute(
            text("""
                DELETE FROM house_numbering.areas
                WHERE organization_id=:org AND project_id=:project AND id=:id
            """),
            {"org": self.org_id, "project": self.project_id, "id": area_id},
        )
        self.audit(
            "house_numbering.area.delete",
            "house_numbering_area",
            area_id,
            {
                "deleted_snapshot": {
                    "name": current["name"],
                    "kind": current["kind"],
                    "parent_id": str(current["parent_id"])
                    if current["parent_id"]
                    else None,
                    "linked_source_id": str(current["linked_source_id"])
                    if current["linked_source_id"]
                    else None,
                    "linked_feature_id": current["linked_feature_id"],
                    "revision": int(current["revision"]),
                }
            },
        )

    def validate_route_segments(self, source_id: UUID, feature_id: str, segments: list[dict[str, Any]] | None) -> list[dict[str, str]]:
        links = [{"source_id": str(source_id), "feature_id": feature_id}]
        if segments:
            links = [{"source_id": str(item["source_id"]), "feature_id": str(item["feature_id"]),
                      "code": str(item.get("code") or "")} for item in segments]
            if links[0]["source_id"] != str(source_id) or links[0]["feature_id"] != feature_id:
                raise PlatformValidationError("first segment must be the primary linked feature", code="INVALID_ROUTE_SEGMENTS")
        pairs = {(item["source_id"], item["feature_id"]) for item in links}
        codes = [item.get("code") for item in links if item.get("code")]
        if len(links)>200 or len(pairs)!=len(links) or len(set(codes))!=len(codes) or any(len(c)>160 for c in codes):
            raise PlatformValidationError("duplicate or excessive route segments", code="INVALID_ROUTE_SEGMENTS")
        # Serialize membership creation in project; prevent reusing an already linked segment.
        self.session.execute(text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),
            {"key": f"house-numbering-segments:{self.project_id}"})
        conflict = self.session.execute(text("""
            SELECT 1 FROM house_numbering.routes r
            CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN jsonb_array_length(r.segments)>0 THEN r.segments
              ELSE jsonb_build_array(jsonb_build_object('source_id',r.linked_source_id,'feature_id',r.linked_feature_id)) END) e(v)
            JOIN jsonb_array_elements(CAST(:links AS jsonb)) l(v)
              ON (e.v->>'source_id', e.v->>'feature_id')=(l.v->>'source_id',l.v->>'feature_id')
            WHERE r.organization_id=:org AND r.project_id=:project
              AND (jsonb_array_length(r.segments)>0 OR :grouped)
            LIMIT 1
        """),{"org":self.org_id,"project":self.project_id,"links":json.dumps(links),"grouped":bool(segments)}).first()
        if conflict:
            raise PlatformConflictError("segment is already linked to a route", code="ROUTE_SEGMENT_OCCUPIED")
        if codes:
            alias_conflict = self.session.execute(text("""
                SELECT 1 FROM house_numbering.routes r
                WHERE r.organization_id=:org AND r.project_id=:project AND (
                  (r.source_label LIKE 'route-tree-import:%' AND substring(r.source_label from 19)=ANY(:codes))
                  OR EXISTS (SELECT 1 FROM jsonb_array_elements(r.segments) e(v)
                             WHERE e.v->>'code'=ANY(:codes)))
                LIMIT 1
            """),{"org":self.org_id,"project":self.project_id,"codes":codes}).first()
            if alias_conflict:
                raise PlatformConflictError("segment code is already linked to another route",code="ROUTE_SEGMENT_CODE_OCCUPIED")
        # Resolve against Core under normal RLS, requiring all links ready and a single continuous path.
        usable = self.session.execute(text("""
            SELECT house_numbering.resolve_route_line(array_agg(f.geometry ORDER BY l.ord)) IS NOT NULL
            FROM jsonb_array_elements(CAST(:links AS jsonb)) WITH ORDINALITY l(v,ord)
            LEFT JOIN spatial.reference_features f ON f.organization_id=:org AND f.project_id=:project
              AND f.source_id=(l.v->>'source_id')::uuid AND f.feature_id=l.v->>'feature_id'
            LEFT JOIN spatial.data_sources s ON s.organization_id=:org AND s.project_id=:project AND s.id=f.source_id
            HAVING bool_and(COALESCE(f.geometry_state='present' AND s.lifecycle='active' AND s.validation_state='ready',false))
        """),{"org":self.org_id,"project":self.project_id,"links":json.dumps(links)}).scalar_one_or_none()
        if not usable:
            raise PlatformValidationError("route segments must form one continuous non-branching line in one CRS", code="ROUTE_SEGMENTS_DISCONNECTED")
        return links if segments else []

    def create_route(
        self,
        *,
        name: str,
        route_type: str,
        parent_route_id: UUID | None,
        prefix: str | None,
        source_id: UUID,
        feature_id: str,
        direction_state: str,
        source_label: str,
        status: str,
        segments: list[dict[str, Any]] | None = None,
    ) -> RowMapping:
        self.prepare("edit")
        if prefix is not None:
            try:
                prefix = canonical_prefix(prefix)
            except ValueError as exc:
                raise PlatformValidationError("invalid alley prefix",code="INVALID_PREFIX") from exc
        self.spatial_feature(source_id, feature_id, expected={"ST_LineString", "ST_MultiLineString"})
        links = self.validate_route_segments(source_id, feature_id, segments)
        now = datetime.now(UTC)
        row = (
            self.session.execute(
                text("""
                INSERT INTO house_numbering.routes (
                    id,organization_id,project_id,name,route_type,parent_route_id,prefix,
                    linked_source_id,linked_feature_id,direction_state,source_label,status,revision,segments,
                    created_by_actor_id,updated_by_actor_id,created_at,updated_at
                ) VALUES (
                    :id,:org,:project,:name,:rtype,:parent,:prefix,:source,:feature,
                    :direction,:source_label,:status,1,CAST(:segments AS jsonb),:actor,:actor,:now,:now
                ) RETURNING *
            """),
                {
                    "id": uuid4(),
                    "org": self.org_id,
                    "project": self.project_id,
                    "name": name.strip(),
                    "rtype": route_type,
                    "parent": parent_route_id,
                    "prefix": prefix,
                    "source": source_id,
                    "feature": feature_id,
                    "direction": direction_state,
                    "source_label": source_label.strip(),
                    "status": status,
                    "segments": json.dumps(links),
                    "actor": self.actor_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        self.audit(
            "house_numbering.route.create",
            "house_numbering_route",
            row["id"],
            {"name": row["name"], "segment_count": len(links) or 1},
        )
        return row

    def list_routes(self) -> list[RowMapping]:
        self.prepare("read")
        return (
            self.session.execute(
                text("""
                SELECT r.*,
                       (SELECT count(*) FROM house_numbering.buildings b
                        WHERE b.organization_id=r.organization_id AND b.project_id=r.project_id
                          AND b.route_id=r.id AND b.state='active') AS building_count
                FROM house_numbering.routes r
                WHERE r.organization_id=:org AND r.project_id=:project
                ORDER BY r.route_type,r.name,r.id
            """),
                {"org": self.org_id, "project": self.project_id},
            )
            .mappings()
            .all()
        )

    def get_route(self, route_id: UUID, *, for_update: bool = False) -> RowMapping:
        self.prepare("edit" if for_update else "read")
        lock = " FOR UPDATE" if for_update else ""
        row = (
            self.session.execute(
                text(
                    """
                SELECT * FROM house_numbering.routes
                WHERE organization_id=:org AND project_id=:project AND id=:id
            """
                    + lock
                ),
                {"org": self.org_id, "project": self.project_id, "id": route_id},
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError("route not found")
        return row

    def update_route(
        self,
        route_id: UUID,
        *,
        expected_revision: int,
        name: str,
        direction_state: str,
        status: str,
    ) -> RowMapping:
        current = self.get_route(route_id, for_update=True)
        if int(current["revision"]) != expected_revision:
            raise PlatformConflictError("route revision is stale", code="STALE_ROUTE")
        if direction_state != current["direction_state"]:
            locked = self.session.execute(
                text("""
                    SELECT 1 FROM house_numbering.address_numbers
                    WHERE organization_id=:org AND project_id=:project AND route_id=:route
                      AND retired_at IS NULL AND kind IN ('submitted','approved') LIMIT 1
                """),
                {"org": self.org_id, "project": self.project_id, "route": route_id},
            ).first()
            if locked is not None:
                raise PlatformConflictError(
                    "route direction is locked by submitted or approved numbers",
                    code="ROUTE_DIRECTION_LOCKED",
                )
        row = (
            self.session.execute(
                text("""
                UPDATE house_numbering.routes
                SET name=:name,direction_state=:direction,status=:status,
                    revision=revision+1,updated_by_actor_id=:actor,updated_at=:now
                WHERE organization_id=:org AND project_id=:project AND id=:id
                RETURNING *
            """),
                {
                    "name": name.strip(),
                    "direction": direction_state,
                    "status": status,
                    "actor": self.actor_id,
                    "now": datetime.now(UTC),
                    "org": self.org_id,
                    "project": self.project_id,
                    "id": route_id,
                },
            )
            .mappings()
            .one()
        )
        self.audit(
            "house_numbering.route.update",
            "house_numbering_route",
            route_id,
            {"revision": row["revision"]},
        )
        return row

    def delete_route(self, route_id: UUID) -> None:
        self.prepare("edit")
        dependency = (
            self.session.execute(
                text("""
                SELECT kind FROM (
                    SELECT 'child_route' AS kind FROM house_numbering.routes
                     WHERE organization_id=:org AND project_id=:project
                       AND parent_route_id=:route
                    UNION ALL
                    SELECT 'building' AS kind FROM house_numbering.buildings
                     WHERE organization_id=:org AND project_id=:project
                       AND route_id=:route
                    UNION ALL
                    SELECT 'number' AS kind FROM house_numbering.address_numbers
                     WHERE organization_id=:org AND project_id=:project
                       AND route_id=:route
                ) dependencies
                LIMIT 1
            """),
                {"org": self.org_id, "project": self.project_id, "route": route_id},
            )
            .mappings()
            .first()
        )
        if dependency is not None:
            raise PlatformConflictError(
                f"route has dependent {dependency['kind']}",
                code="ROUTE_HAS_DEPENDENCIES",
            )
        deleted = self.session.execute(
            text("""
                DELETE FROM house_numbering.routes
                WHERE organization_id=:org AND project_id=:project AND id=:id
                RETURNING id
            """),
            {"org": self.org_id, "project": self.project_id, "id": route_id},
        ).scalar_one_or_none()
        if deleted is None:
            raise PlatformNotFoundError("route not found")
        self.audit(
            "house_numbering.route.delete", "house_numbering_route", route_id, {}
        )

    def create_building(
        self,
        *,
        route_id: UUID | None,
        area_id: UUID | None,
        parcel_ref: str | None,
        source_id: UUID,
        feature_id: str,
        eligibility: str,
        ineligibility_reason: str | None,
    ) -> RowMapping:
        self.prepare("edit")
        self.spatial_feature(source_id, feature_id, expected={"ST_Point", "ST_Polygon"})
        now = datetime.now(UTC)
        row = (
            self.session.execute(
                text("""
                INSERT INTO house_numbering.buildings (
                    id,organization_id,project_id,route_id,area_id,parcel_ref,
                    linked_source_id,linked_feature_id,eligibility,ineligibility_reason,
                    state,revision,created_by_actor_id,updated_by_actor_id,created_at,updated_at
                ) VALUES (
                    :id,:org,:project,:route,:area,:parcel,:source,:feature,:eligibility,:reason,
                    'active',1,:actor,:actor,:now,:now
                ) RETURNING *
            """),
                {
                    "id": uuid4(),
                    "org": self.org_id,
                    "project": self.project_id,
                    "route": route_id,
                    "area": area_id,
                    "parcel": parcel_ref,
                    "source": source_id,
                    "feature": feature_id,
                    "eligibility": eligibility,
                    "reason": ineligibility_reason,
                    "actor": self.actor_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        self.audit(
            "house_numbering.building.create", "house_numbering_building", row["id"], {}
        )
        return row

    def list_buildings(self, *, route_id: UUID | None = None) -> list[RowMapping]:
        self.prepare("read")
        sql = """
            SELECT b.*,
                   n.number_text AS current_number,n.kind AS number_kind,n.id AS number_id
            FROM house_numbering.buildings b
            LEFT JOIN LATERAL (
                SELECT id,number_text,kind FROM house_numbering.address_numbers n
                WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
                  AND n.building_id=b.id AND n.retired_at IS NULL
                ORDER BY CASE n.kind
                    WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2 WHEN 'proposed' THEN 3
                    WHEN 'certificate' THEN 4 WHEN 'survey' THEN 5 WHEN 'cad' THEN 6 ELSE 7 END,
                    n.created_at DESC LIMIT 1
            ) n ON true
            WHERE b.organization_id=:org AND b.project_id=:project AND b.state='active'
        """
        params: dict[str, Any] = {"org": self.org_id, "project": self.project_id}
        if route_id is not None:
            sql += " AND b.route_id=:route"
            params["route"] = route_id
        sql += " ORDER BY b.created_at,b.id"
        return self.session.execute(text(sql), params).mappings().all()

    def get_building(
        self, building_id: UUID, *, for_update: bool = False
    ) -> RowMapping:
        self.prepare("edit" if for_update else "read")
        lock = " FOR UPDATE" if for_update else ""
        row = (
            self.session.execute(
                text(
                    """
                SELECT * FROM house_numbering.buildings
                WHERE organization_id=:org AND project_id=:project AND id=:id
            """
                    + lock
                ),
                {"org": self.org_id, "project": self.project_id, "id": building_id},
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError("building not found")
        return row

    def update_building(
        self,
        building_id: UUID,
        *,
        expected_revision: int,
        route_id: UUID | None,
        area_id: UUID | None,
        parcel_ref: str | None,
        source_id: UUID,
        feature_id: str,
    ) -> RowMapping:
        current = self.get_building(building_id, for_update=True)
        if current["state"] != "active":
            raise PlatformConflictError(
                "building is not active", code="BUILDING_NOT_ACTIVE"
            )
        if int(current["revision"]) != expected_revision:
            raise PlatformConflictError(
                "building revision is stale", code="STALE_BUILDING"
            )
        self.spatial_feature(source_id, feature_id, expected={"ST_Point", "ST_Polygon"})
        if route_id is not None:
            self.get_route(route_id)
        if area_id is not None:
            self.get_area(area_id)
        if route_id != current["route_id"]:
            locked = self.session.execute(
                text("""
                    SELECT 1 FROM house_numbering.address_numbers
                    WHERE organization_id=:org AND project_id=:project
                      AND building_id=:building AND retired_at IS NULL
                      AND kind IN ('submitted','approved','reserved')
                    LIMIT 1
                """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "building": building_id,
                },
            ).first()
            if locked is not None:
                raise PlatformConflictError(
                    "building route is locked by an official number",
                    code="BUILDING_ROUTE_LOCKED",
                )
        now = datetime.now(UTC)
        row = (
            self.session.execute(
                text("""
                    UPDATE house_numbering.buildings
                    SET route_id=:route,area_id=:area,parcel_ref=:parcel,
                        linked_source_id=:source,linked_feature_id=:feature,
                        revision=revision+1,updated_by_actor_id=:actor,updated_at=:now
                    WHERE organization_id=:org AND project_id=:project AND id=:id
                    RETURNING *
                """),
                {
                    "route": route_id,
                    "area": area_id,
                    "parcel": parcel_ref.strip() if parcel_ref else None,
                    "source": source_id,
                    "feature": feature_id,
                    "actor": self.actor_id,
                    "now": now,
                    "org": self.org_id,
                    "project": self.project_id,
                    "id": building_id,
                },
            )
            .mappings()
            .one()
        )
        self.audit(
            "house_numbering.building.update",
            "house_numbering_building",
            building_id,
            {
                "before": {
                    "route_id": str(current["route_id"])
                    if current["route_id"]
                    else None,
                    "area_id": str(current["area_id"]) if current["area_id"] else None,
                    "parcel_ref": current["parcel_ref"],
                    "linked_source_id": str(current["linked_source_id"]),
                    "linked_feature_id": current["linked_feature_id"],
                },
                "after_revision": int(row["revision"]),
            },
        )
        return row

    def retire_building(self, building_id: UUID, *, expected_revision: int) -> None:
        current = self.get_building(building_id, for_update=True)
        if current["state"] != "active":
            raise PlatformConflictError(
                "building is not active", code="BUILDING_NOT_ACTIVE"
            )
        if int(current["revision"]) != expected_revision:
            raise PlatformConflictError(
                "building revision is stale", code="STALE_BUILDING"
            )
        locked = self.session.execute(
            text("""
                SELECT kind FROM house_numbering.address_numbers
                WHERE organization_id=:org AND project_id=:project
                  AND building_id=:building AND retired_at IS NULL
                  AND kind IN ('submitted','approved','reserved')
                LIMIT 1
            """),
            {"org": self.org_id, "project": self.project_id, "building": building_id},
        ).scalar_one_or_none()
        if locked is not None:
            raise PlatformConflictError(
                "building with an official number must use the split/merge or approval workflow",
                code="BUILDING_OFFICIAL_NUMBER_LOCKED",
            )
        now = datetime.now(UTC)
        self.session.execute(
            text("""
                UPDATE house_numbering.buildings
                SET state='removed',
                    plaque_status=CASE WHEN plaque_status='not_planned' THEN plaque_status ELSE 'removed' END,
                    plaque_updated_at=CASE WHEN plaque_status='not_planned' THEN plaque_updated_at ELSE :now END,
                    revision=revision+1,updated_by_actor_id=:actor,updated_at=:now
                WHERE organization_id=:org AND project_id=:project AND id=:id
            """),
            {
                "now": now,
                "actor": self.actor_id,
                "org": self.org_id,
                "project": self.project_id,
                "id": building_id,
            },
        )
        self.audit(
            "house_numbering.building.retire",
            "house_numbering_building",
            building_id,
            {
                "snapshot": {
                    "route_id": str(current["route_id"])
                    if current["route_id"]
                    else None,
                    "area_id": str(current["area_id"]) if current["area_id"] else None,
                    "parcel_ref": current["parcel_ref"],
                    "linked_source_id": str(current["linked_source_id"]),
                    "linked_feature_id": current["linked_feature_id"],
                    "revision": int(current["revision"]),
                }
            },
        )

    def set_building_eligibility(
        self,
        building_id: UUID,
        *,
        expected_revision: int,
        eligibility: str,
        reason: str | None,
    ) -> RowMapping:
        current = self.get_building(building_id, for_update=True)
        if int(current["revision"]) != expected_revision:
            raise PlatformConflictError(
                "building revision is stale", code="STALE_BUILDING"
            )
        row = (
            self.session.execute(
                text("""
                UPDATE house_numbering.buildings
                SET eligibility=:eligibility,ineligibility_reason=:reason,
                    revision=revision+1,updated_by_actor_id=:actor,updated_at=:now
                WHERE organization_id=:org AND project_id=:project AND id=:id
                RETURNING *
            """),
                {
                    "eligibility": eligibility,
                    "reason": reason,
                    "actor": self.actor_id,
                    "now": datetime.now(UTC),
                    "org": self.org_id,
                    "project": self.project_id,
                    "id": building_id,
                },
            )
            .mappings()
            .one()
        )
        self.audit(
            "house_numbering.building.eligibility",
            "house_numbering_building",
            building_id,
            {"eligibility": eligibility},
        )
        return row

    def inspection_context(
        self, building_id: UUID | None, route_id: UUID
    ) -> dict[str, Any]:
        self.prepare("read")
        if building_id is None:
            return {"building": None, "evidence": []}
        building = (
            self.session.execute(
                text("""
                    SELECT b.id,b.parcel_ref,b.linked_source_id,b.linked_feature_id,
                           f.attributes AS core_feature_attributes
                    FROM house_numbering.buildings b
                    LEFT JOIN spatial.reference_features f
                      ON f.organization_id=b.organization_id
                     AND f.project_id=b.project_id
                     AND f.source_id=b.linked_source_id
                     AND f.feature_id=b.linked_feature_id
                    WHERE b.organization_id=:org AND b.project_id=:project
                      AND b.id=:building AND b.route_id=:route
                """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "building": building_id,
                    "route": route_id,
                },
            )
            .mappings()
            .first()
        )
        if building is None:
            raise PlatformNotFoundError("building is not on the inspected route")
        evidence = (
            self.session.execute(
                text("""
                    SELECT id,origin,raw_number,source_reference,evidence_date,
                           confidence,created_at
                    FROM house_numbering.evidence
                    WHERE organization_id=:org AND project_id=:project
                      AND (building_id=:building OR (
                          building_id IS NULL AND route_id=:route
                      ))
                    ORDER BY evidence_date NULLS LAST,created_at,id
                """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "building": building_id,
                    "route": route_id,
                },
            )
            .mappings()
            .all()
        )
        return {"building": dict(building), "evidence": [dict(row) for row in evidence]}

    def set_plaque_status(
        self,
        building_id: UUID,
        *,
        expected_revision: int,
        status: str,
    ) -> RowMapping:
        self.prepare("manage")
        if status not in {"not_planned", "planned", "installed", "replaced", "removed"}:
            raise PlatformValidationError("invalid plaque status", code="PLAQUE_STATUS")
        current = (
            self.session.execute(
                text("""
                    SELECT * FROM house_numbering.buildings
                    WHERE organization_id=:org AND project_id=:project AND id=:id
                    FOR UPDATE
                """),
                {"org": self.org_id, "project": self.project_id, "id": building_id},
            )
            .mappings()
            .first()
        )
        if current is None:
            raise PlatformNotFoundError("building not found")
        if int(current["revision"]) != expected_revision:
            raise PlatformConflictError(
                "building revision is stale", code="STALE_BUILDING"
            )
        if status in {"installed", "replaced"}:
            approved = self.session.execute(
                text("""
                    SELECT id FROM house_numbering.address_numbers
                    WHERE organization_id=:org AND project_id=:project
                      AND building_id=:building AND kind='approved'
                      AND retired_at IS NULL
                    LIMIT 1
                """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "building": building_id,
                },
            ).scalar_one_or_none()
            if approved is None:
                raise PlatformConflictError(
                    "plaque installation requires an approved house number",
                    code="PLAQUE_REQUIRES_APPROVED_NUMBER",
                )
        now = datetime.now(UTC)
        row = (
            self.session.execute(
                text("""
                    UPDATE house_numbering.buildings
                    SET plaque_status=:status,plaque_updated_at=:now,
                        revision=revision+1,updated_by_actor_id=:actor,updated_at=:now
                    WHERE organization_id=:org AND project_id=:project AND id=:id
                    RETURNING *
                """),
                {
                    "status": status,
                    "now": now,
                    "actor": self.actor_id,
                    "org": self.org_id,
                    "project": self.project_id,
                    "id": building_id,
                },
            )
            .mappings()
            .one()
        )
        self.audit(
            "house_numbering.building.plaque_status",
            "house_numbering_building",
            building_id,
            {"old_status": current["plaque_status"], "new_status": status},
        )
        return row

    def record_building_lineage(
        self,
        *,
        relation_kind: str,
        operation_id: UUID,
        predecessor_ids: list[UUID],
        successor_ids: list[UUID],
        expected_revisions: dict[UUID, int],
    ) -> list[RowMapping]:
        self.prepare("manage")
        if relation_kind not in {"split", "merge"}:
            raise PlatformValidationError("lineage relation must be split or merge")
        predecessors = set(predecessor_ids)
        successors = set(successor_ids)
        if relation_kind == "split" and (len(predecessors) != 1 or len(successors) < 2):
            raise PlatformValidationError(
                "split requires one predecessor and at least two successors"
            )
        if relation_kind == "merge" and (len(predecessors) < 2 or len(successors) != 1):
            raise PlatformValidationError(
                "merge requires at least two predecessors and one successor"
            )
        if predecessors & successors:
            raise PlatformValidationError(
                "lineage predecessor and successor sets must be disjoint"
            )
        all_ids = predecessors | successors
        if set(expected_revisions) != all_ids:
            raise PlatformValidationError(
                "expected_revisions must cover every lineage building",
                code="LINEAGE_REVISION_SET",
            )

        existing = (
            self.session.execute(
                text("""
                    SELECT * FROM house_numbering.building_lineage
                    WHERE organization_id=:org AND project_id=:project
                      AND operation_id=:operation
                    ORDER BY predecessor_building_id,successor_building_id
                """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "operation": operation_id,
                },
            )
            .mappings()
            .all()
        )
        if existing:
            existing_kind = {str(row["relation_kind"]) for row in existing}
            existing_pre = {
                UUID(str(row["predecessor_building_id"])) for row in existing
            }
            existing_succ = {
                UUID(str(row["successor_building_id"])) for row in existing
            }
            if (
                existing_kind == {relation_kind}
                and existing_pre == predecessors
                and existing_succ == successors
            ):
                return existing
            raise PlatformConflictError(
                "lineage operation_id was already used with different participants",
                code="LINEAGE_OPERATION_CONFLICT",
            )

        rows = (
            self.session.execute(
                text("""
                    SELECT * FROM house_numbering.buildings
                    WHERE organization_id=:org AND project_id=:project
                      AND id = ANY(:ids)
                    ORDER BY id
                    FOR UPDATE
                """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "ids": sorted(all_ids, key=str),
                },
            )
            .mappings()
            .all()
        )
        by_id = {UUID(str(row["id"])): row for row in rows}
        if set(by_id) != all_ids:
            raise PlatformNotFoundError("one or more lineage buildings were not found")
        for building_id, expected_revision in expected_revisions.items():
            row = by_id[building_id]
            if int(row["revision"]) != expected_revision:
                raise PlatformConflictError(
                    f"building {building_id} revision is stale",
                    code="STALE_BUILDING",
                )
            if row["state"] != "active":
                raise PlatformConflictError(
                    f"building {building_id} is not active",
                    code="BUILDING_NOT_ACTIVE",
                )

        now = datetime.now(UTC)
        for predecessor in predecessors:
            self.session.execute(
                text("""
                    UPDATE house_numbering.buildings
                    SET state='removed',
                        plaque_status=CASE
                            WHEN plaque_status='not_planned' THEN plaque_status
                            ELSE 'removed'
                        END,
                        plaque_updated_at=CASE
                            WHEN plaque_status='not_planned' THEN plaque_updated_at
                            ELSE :now
                        END,
                        revision=revision+1,updated_by_actor_id=:actor,updated_at=:now
                    WHERE organization_id=:org AND project_id=:project AND id=:id
                """),
                {
                    "now": now,
                    "actor": self.actor_id,
                    "org": self.org_id,
                    "project": self.project_id,
                    "id": predecessor,
                },
            )

        inserted: list[RowMapping] = []
        for predecessor in sorted(predecessors, key=str):
            for successor in sorted(successors, key=str):
                inserted.append(
                    self.session.execute(
                        text("""
                            INSERT INTO house_numbering.building_lineage (
                                id,organization_id,project_id,operation_id,relation_kind,
                                predecessor_building_id,successor_building_id,
                                created_by_actor_id,created_at
                            ) VALUES (
                                :id,:org,:project,:operation,:kind,:predecessor,:successor,
                                :actor,:now
                            ) RETURNING *
                        """),
                        {
                            "id": uuid4(),
                            "org": self.org_id,
                            "project": self.project_id,
                            "operation": operation_id,
                            "kind": relation_kind,
                            "predecessor": predecessor,
                            "successor": successor,
                            "actor": self.actor_id,
                            "now": now,
                        },
                    )
                    .mappings()
                    .one()
                )
        self.audit(
            "house_numbering.building.lineage",
            "house_numbering_building_lineage",
            operation_id,
            {
                "relation_kind": relation_kind,
                "predecessor_ids": [
                    str(value) for value in sorted(predecessors, key=str)
                ],
                "successor_ids": [str(value) for value in sorted(successors, key=str)],
            },
        )
        return inserted

    def list_building_lineage(self, building_id: UUID) -> list[RowMapping]:
        self.prepare("read")
        self.get_building(building_id)
        return (
            self.session.execute(
                text("""
                    SELECT * FROM house_numbering.building_lineage
                    WHERE organization_id=:org AND project_id=:project
                      AND (predecessor_building_id=:building OR successor_building_id=:building)
                    ORDER BY created_at,id
                """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "building": building_id,
                },
            )
            .mappings()
            .all()
        )

    def route_geometry(self, route_id: UUID) -> dict[str, Any]:
        self.prepare("read")
        row = (
            self.session.execute(
                text("""
                SELECT r.id,r.prefix,r.direction_state,r.revision,f.content_revision,
                       ST_SRID(f.geometry) AS srid,
                       ST_AsGeoJSON(
                           CASE WHEN r.direction_state='reverse' THEN ST_Reverse(f.geometry) ELSE f.geometry END
                       )::jsonb AS geometry
                FROM house_numbering.routes r
                JOIN house_numbering.route_lines f
                  ON f.organization_id=r.organization_id AND f.project_id=r.project_id AND f.route_id=r.id
                WHERE r.organization_id=:org AND r.project_id=:project AND r.id=:route
            """),
                {"org": self.org_id, "project": self.project_id, "route": route_id},
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError("route geometry not available")
        srid = int(row["srid"] or 0)
        projected = self.session.execute(
            text("""
                SELECT NOT (COALESCE(srtext,'') ILIKE 'GEOGCS%' OR COALESCE(proj4text,'') LIKE '%+proj=longlat%')
                FROM spatial_ref_sys WHERE srid=:srid
            """),
            {"srid": srid},
        ).scalar_one_or_none()
        if projected is not True:
            raise PlatformValidationError(
                "numbering requires a verified projected route CRS",
                code="PROJECTED_CRS_REQUIRED",
            )
        return dict(row)

    def transform_point(
        self, x: float, y: float, source_crs: int, target_crs: int
    ) -> tuple[float, float]:
        row = self.session.execute(
            text("""
                SELECT ST_X(g),ST_Y(g) FROM (
                    SELECT ST_Transform(ST_SetSRID(ST_MakePoint(:x,:y),:source_crs),:target_crs) g
                ) q
            """),
            {"x": x, "y": y, "source_crs": source_crs, "target_crs": target_crs},
        ).one()
        return float(row[0]), float(row[1])

    def numbering_buildings(self, ids: list[UUID]) -> list[dict[str, Any]]:
        self.prepare("read")
        return [dict(row) for row in self.session.execute(text("""
            SELECT b.*, EXISTS (
                SELECT 1 FROM house_numbering.address_numbers n
                WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
                  AND n.building_id=b.id AND n.retired_at IS NULL
                  AND n.kind IN ('submitted','approved','reserved','proposed')
            ) AS protected_number
            FROM house_numbering.buildings b
            WHERE b.organization_id=:org AND b.project_id=:project
              AND b.id=ANY(CAST(:ids AS uuid[]))
        """), {"org":self.org_id,"project":self.project_id,"ids":ids}).mappings()]

    def numbering_route_snapshot(self, route_id: UUID) -> str:
        self.prepare("read")
        params={"org":self.org_id,"project":self.project_id,"route":route_id}
        buildings=self.session.execute(text("""
            SELECT b.id,b.revision,f.content_revision,f.geometry_state,s.lifecycle,s.validation_state
            FROM house_numbering.buildings b
            LEFT JOIN spatial.reference_features f
              ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
            LEFT JOIN spatial.data_sources s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
            WHERE b.organization_id=:org AND b.project_id=:project AND b.route_id=:route AND b.state='active'
            ORDER BY b.id
        """),params).mappings().all()
        numbers=self.session.execute(text("""
            SELECT id,building_id,revision,number_text,kind,side,measure_m FROM house_numbering.address_numbers
            WHERE organization_id=:org AND project_id=:project AND route_id=:route AND retired_at IS NULL
            ORDER BY id
        """),params).mappings().all()
        payload=[[dict(row) for row in rows] for rows in (buildings,numbers)]
        return hashlib.sha256(json.dumps(payload,default=str,sort_keys=True).encode()).hexdigest()

    def numbering_targets(self,route_id: UUID, include_descendants: bool, limit: int=5000) -> dict[str, Any]:
        self.get_route(route_id)
        rows=self.session.execute(text("""
            WITH RECURSIVE tree(id) AS (
                SELECT CAST(:route AS uuid)
                UNION SELECT r.id FROM house_numbering.routes r JOIN tree t ON r.parent_route_id=t.id
                WHERE r.organization_id=:org AND r.project_id=:project AND :descendants
            )
            SELECT b.id,b.route_id,b.parcel_ref,b.eligibility,count(*) OVER () AS total
            FROM house_numbering.buildings b JOIN tree t ON t.id=b.route_id
            WHERE b.organization_id=:org AND b.project_id=:project AND b.state='active'
              AND b.eligibility!='ineligible'
              AND NOT EXISTS (
                SELECT 1 FROM house_numbering.address_numbers n WHERE n.organization_id=b.organization_id
                  AND n.project_id=b.project_id AND n.building_id=b.id AND n.retired_at IS NULL
                  AND n.kind IN ('proposed','submitted','approved','reserved')
              )
            ORDER BY b.route_id,b.id LIMIT :limit
        """),{"org":self.org_id,"project":self.project_id,"route":route_id,"descendants":include_descendants,"limit":limit}).mappings().all()
        total=int(rows[0]["total"]) if rows else 0
        return {"items":[{k:v for k,v in dict(row).items() if k!="total"} for row in rows],"total":total,"truncated":total>limit}

    def building_points_for_route(
        self, route_id: UUID, target_crs: int
    ) -> list[dict[str, Any]]:
        self.prepare("read")
        rows = (
            self.session.execute(
                text("""
                SELECT b.id,b.revision,n.id AS number_id,n.number_text,n.kind,n.side,n.measure_m,
                       ST_X(ST_Transform(ST_PointOnSurface(f.geometry),:target_crs)) AS x,
                       ST_Y(ST_Transform(ST_PointOnSurface(f.geometry),:target_crs)) AS y
                FROM house_numbering.buildings b
                JOIN spatial.reference_features f
                  ON f.organization_id=b.organization_id AND f.project_id=b.project_id
                 AND f.source_id=b.linked_source_id AND f.feature_id=b.linked_feature_id
                JOIN spatial.data_sources s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
                LEFT JOIN LATERAL (
                    SELECT * FROM house_numbering.address_numbers n
                    WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
                      AND n.building_id=b.id AND n.route_id=b.route_id AND n.retired_at IS NULL
                    ORDER BY CASE n.kind
                        WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2 WHEN 'reserved' THEN 3
                        WHEN 'proposed' THEN 4 WHEN 'certificate' THEN 5 WHEN 'survey' THEN 6
                        WHEN 'cad' THEN 7 ELSE 8 END, n.created_at DESC LIMIT 1
                ) n ON true
                WHERE b.organization_id=:org AND b.project_id=:project
                  AND b.route_id=:route AND b.state='active' AND f.geometry_state='present'
                  AND s.lifecycle='active' AND s.validation_state='ready'
                  AND ST_SRID(f.geometry)>0 AND NOT ST_IsEmpty(f.geometry)
                  AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon','ST_MultiPolygon')
            """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "route": route_id,
                    "target_crs": target_crs,
                },
            )
            .mappings()
            .all()
        )
        return [dict(row) for row in rows]

    def list_route_numbers(self, route_id: UUID) -> list[RowMapping]:
        self.prepare("read")
        return (
            self.session.execute(
                text("""
                SELECT * FROM house_numbering.address_numbers
                WHERE organization_id=:org AND project_id=:project AND route_id=:route
                  AND retired_at IS NULL
                ORDER BY measure_m NULLS LAST,created_at,id
            """),
                {"org": self.org_id, "project": self.project_id, "route": route_id},
            )
            .mappings()
            .all()
        )

    def create_preview(
        self,
        *,
        operation_id: UUID,
        rule_id: UUID,
        base_revisions: dict[str, Any],
        changes: list[dict[str, Any]],
        warnings: list[str],
        expires_at: datetime,
    ) -> RowMapping:
        self.prepare("edit")
        existing = (
            self.session.execute(
                text("""
                SELECT * FROM house_numbering.previews
                WHERE organization_id=:org AND project_id=:project AND operation_id=:operation
            """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "operation": operation_id,
                },
            )
            .mappings()
            .first()
        )
        if existing is not None:
            return existing
        now = datetime.now(UTC)
        return (
            self.session.execute(
                text("""
                INSERT INTO house_numbering.previews (
                    id,organization_id,project_id,operation_id,rule_id,base_revisions,
                    candidate_changes,warnings,expires_at,created_by_actor_id,created_at
                ) VALUES (
                    :id,:org,:project,:operation,:rule,CAST(:base AS jsonb),CAST(:changes AS jsonb),
                    CAST(:warnings AS jsonb),:expires,:actor,:now
                ) RETURNING *
            """),
                {
                    "id": uuid4(),
                    "org": self.org_id,
                    "project": self.project_id,
                    "operation": operation_id,
                    "rule": rule_id,
                    "base": json.dumps(base_revisions),
                    "changes": json.dumps(changes),
                    "warnings": json.dumps(warnings),
                    "expires": expires_at,
                    "actor": self.actor_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )

    def get_preview(self, preview_id: UUID, *, for_update: bool = False) -> RowMapping:
        self.prepare("edit" if for_update else "read")
        lock = " FOR UPDATE" if for_update else ""
        row = (
            self.session.execute(
                text(
                    """
                SELECT * FROM house_numbering.previews
                WHERE organization_id=:org AND project_id=:project AND id=:id
            """
                    + lock
                ),
                {"org": self.org_id, "project": self.project_id, "id": preview_id},
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError("numbering preview not found")
        return row

    def proposed_number_for_building(self, building_id: UUID) -> RowMapping | None:
        self.prepare("read")
        return (
            self.session.execute(
                text("""
                SELECT * FROM house_numbering.address_numbers
                WHERE organization_id=:org AND project_id=:project AND building_id=:building
                  AND retired_at IS NULL AND kind IN ('proposed','submitted','approved')
                ORDER BY CASE kind WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2 ELSE 3 END
                LIMIT 1
            """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "building": building_id,
                },
            )
            .mappings()
            .first()
        )

    def apply_preview(self, preview_id: UUID) -> list[RowMapping]:
        self.prepare("edit")
        # Number assignments in different branches must serialize so two previews
        # cannot allocate the same full address concurrently under one street.
        self.session.execute(text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),
                             {"key": f"house-numbering-number-apply:{self.project_id}"})
        preview = self.get_preview(preview_id, for_update=True)
        if preview["applied_at"] is not None:
            changes = list(preview["candidate_changes"])
            ids = [
                UUID(str(change["result_number_id"]))
                for change in changes
                if change.get("result_number_id")
            ]
            if not ids:
                return []
            return (
                self.session.execute(
                    text("""
                    SELECT * FROM house_numbering.address_numbers
                    WHERE organization_id=:org AND project_id=:project AND id = ANY(:ids)
                    ORDER BY created_at,id
                """),
                    {"org": self.org_id, "project": self.project_id, "ids": ids},
                )
                .mappings()
                .all()
            )
        if preview["expires_at"] <= datetime.now(UTC):
            raise PlatformConflictError(
                "numbering preview expired", code="PREVIEW_EXPIRED"
            )
        base = dict(preview["base_revisions"])
        changes = [dict(item) for item in preview["candidate_changes"]]
        results: list[RowMapping] = []
        if len({change["building_id"] for change in changes}) != len(changes) or len({(change["route_id"],change["number"]) for change in changes}) != len(changes):
            raise PlatformConflictError("duplicate in numbering preview", code="NUMBER_OCCUPIED")
        for route_id in sorted({UUID(str(change["route_id"])) for change in changes}):
            route = self.get_route(route_id, for_update=True)
            if int(route["revision"]) != int(base[f"route:{route_id}"]):
                raise PlatformConflictError("route changed since preview", code="STALE_PREVIEW")
            geometry_key = f"route_geometry:{route_id}"
            try:
                current_geometry_revision = self.route_geometry(route_id)["content_revision"]
            except (PlatformNotFoundError, PlatformValidationError) as exc:
                raise PlatformConflictError("route geometry no longer usable",code="STALE_PREVIEW") from exc
            if geometry_key in base and current_geometry_revision != base[geometry_key]:
                raise PlatformConflictError("route geometry changed since preview", code="STALE_PREVIEW")
            snapshot_key=f"route_snapshot:{route_id}"
            if snapshot_key in base and self.numbering_route_snapshot(route_id) != base[snapshot_key]:
                raise PlatformConflictError("route addresses or building geometry changed since preview",code="STALE_PREVIEW")
        if changes:
            scope = {"org": self.org_id, "project": self.project_id}
            parents = {str(r["id"]): str(r["parent_route_id"]) if r["parent_route_id"] else None
                for r in self.session.execute(text("""
                    SELECT id,parent_route_id FROM house_numbering.routes
                    WHERE organization_id=:org AND project_id=:project
                """), scope).mappings()}
            existing = self.session.execute(text("""
                SELECT route_id,number_text,building_id FROM house_numbering.address_numbers
                WHERE organization_id=:org AND project_id=:project AND retired_at IS NULL
                  AND number_text=ANY(:numbers)
            """), {**scope, "numbers": sorted({str(c["number"]) for c in changes})}).mappings().all()
            occupants = [{"route_id": r["route_id"], "number_text": r["number_text"],
                "building_id": r["building_id"], "origin": "existing"} for r in existing]
            occupants.extend({"route_id": c["route_id"], "number_text": c["number"],
                "building_id": c["building_id"], "origin": "proposal"} for c in changes)
            cross = find_tree_collisions(parents, occupants)
            if any(any(o["origin"] == "proposal" for o in issue["occupants"]) for issue in cross):
                raise PlatformConflictError("full number already exists on another route in the same street tree",
                                            code="CROSS_ROUTE_NUMBER_OCCUPIED")
        for change in changes:
            building_id = UUID(str(change["building_id"]))
            route_id = UUID(str(change["route_id"]))
            building = self.get_building(building_id, for_update=True)
            if int(building["revision"]) != int(base[f"building:{building_id}"]) or building["route_id"] != route_id or building["state"] != "active":
                raise PlatformConflictError("building changed since preview", code="STALE_PREVIEW")
            if building["eligibility"] == "ineligible":
                raise PlatformConflictError("ineligible building cannot receive a number",code="INELIGIBLE_BUILDING")
            # Never silently replace a current proposal or a protected number, even
            # if it was created on another route after the preview.
            occupied = self.session.execute(text("""
                SELECT 1 FROM house_numbering.address_numbers
                WHERE organization_id=:org AND project_id=:project AND retired_at IS NULL AND (
                    (building_id=:building AND kind IN ('proposed','submitted','approved','reserved'))
                    OR (route_id=:route AND number_text=:number AND building_id IS DISTINCT FROM :building)
                ) LIMIT 1
            """),{"org":self.org_id,"project":self.project_id,"building":building_id,
                   "route":route_id,"number":change["number"]}).first()
            if occupied is not None:
                raise PlatformConflictError("number became occupied or building already numbered",code="NUMBER_OCCUPIED")
            now = datetime.now(UTC)
            self.session.execute(
                text("""
                    UPDATE house_numbering.address_numbers SET retired_at=:now,
                        revision=revision+1,updated_by_actor_id=:actor,updated_at=:now
                    WHERE organization_id=:org AND project_id=:project AND building_id=:building
                      AND retired_at IS NULL AND kind='proposed'
                """),
                {
                    "now": now,
                    "actor": self.actor_id,
                    "org": self.org_id,
                    "project": self.project_id,
                    "building": building_id,
                },
            )
            number_id = uuid4()
            row = (
                self.session.execute(
                    text("""
                    INSERT INTO house_numbering.address_numbers (
                        id,organization_id,project_id,building_id,route_id,number_text,kind,
                        side,measure_m,rule_id,revision,created_by_actor_id,updated_by_actor_id,
                        created_at,updated_at
                    ) VALUES (
                        :id,:org,:project,:building,:route,:number,'proposed',:side,:measure,
                        :rule,1,:actor,:actor,:now,:now
                    ) RETURNING *
                """),
                    {
                        "id": number_id,
                        "org": self.org_id,
                        "project": self.project_id,
                        "building": building_id,
                        "route": route_id,
                        "number": change["number"],
                        "side": change["side"],
                        "measure": change["measure"],
                        "rule": preview["rule_id"],
                        "actor": self.actor_id,
                        "now": now,
                    },
                )
                .mappings()
                .one()
            )
            change["result_number_id"] = str(number_id)
            results.append(row)
        self.session.execute(
            text("""
                UPDATE house_numbering.previews
                SET candidate_changes=CAST(:changes AS jsonb),applied_at=:now
                WHERE organization_id=:org AND project_id=:project AND id=:id
            """),
            {
                "changes": json.dumps(changes),
                "now": datetime.now(UTC),
                "org": self.org_id,
                "project": self.project_id,
                "id": preview_id,
            },
        )
        self.audit(
            "house_numbering.preview.apply",
            "house_numbering_preview",
            preview_id,
            {"count": len(results)},
        )
        return results

    def create_proposal(
        self,
        *,
        scope_kind: str,
        scope_id: UUID,
        rule_id: UUID,
        building_ids: list[UUID],
    ) -> tuple[RowMapping, list[RowMapping]]:
        self.prepare("edit")
        if not building_ids:
            raise PlatformValidationError("proposal requires at least one building")
        self.get_rule(rule_id)
        number_rows: list[RowMapping] = []
        for building_id in building_ids:
            row = self.proposed_number_for_building(building_id)
            if row is None or row["kind"] != "proposed":
                raise PlatformConflictError(
                    "every proposal item needs a proposed number",
                    code="MISSING_PROPOSED_NUMBER",
                )
            number_rows.append(row)
        now = datetime.now(UTC)
        proposal_id = uuid4()
        proposal = (
            self.session.execute(
                text("""
                INSERT INTO house_numbering.proposals (
                    id,organization_id,project_id,scope_kind,scope_id,rule_id,status,revision,
                    created_by_actor_id,updated_by_actor_id,created_at,updated_at
                ) VALUES (:id,:org,:project,:kind,:scope,:rule,'draft',1,:actor,:actor,:now,:now)
                RETURNING *
            """),
                {
                    "id": proposal_id,
                    "org": self.org_id,
                    "project": self.project_id,
                    "kind": scope_kind,
                    "scope": scope_id,
                    "rule": rule_id,
                    "actor": self.actor_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        items: list[RowMapping] = []
        for number in number_rows:
            items.append(
                self.session.execute(
                    text("""
                        INSERT INTO house_numbering.proposal_items (
                            id,organization_id,project_id,proposal_id,building_id,proposed_number,
                            source_number_id,created_at
                        ) VALUES (:id,:org,:project,:proposal,:building,:number,:source,:now)
                        RETURNING *
                    """),
                    {
                        "id": uuid4(),
                        "org": self.org_id,
                        "project": self.project_id,
                        "proposal": proposal_id,
                        "building": number["building_id"],
                        "number": number["number_text"],
                        "source": number["id"],
                        "now": now,
                    },
                )
                .mappings()
                .one()
            )
        self.audit(
            "house_numbering.proposal.create",
            "house_numbering_proposal",
            proposal_id,
            {"items": len(items)},
        )
        return proposal, items

    def proposal(
        self, proposal_id: UUID, *, for_update: bool = False
    ) -> tuple[RowMapping, list[RowMapping]]:
        self.prepare("edit" if for_update else "read")
        lock = " FOR UPDATE" if for_update else ""
        proposal = (
            self.session.execute(
                text(
                    """
                SELECT p.*,r.status AS rule_status,r.authority_ref
                FROM house_numbering.proposals p
                JOIN house_numbering.rules r
                  ON r.organization_id=p.organization_id AND r.project_id=p.project_id AND r.id=p.rule_id
                WHERE p.organization_id=:org AND p.project_id=:project AND p.id=:id
            """
                    + lock
                ),
                {"org": self.org_id, "project": self.project_id, "id": proposal_id},
            )
            .mappings()
            .first()
        )
        if proposal is None:
            raise PlatformNotFoundError("proposal not found")
        items = (
            self.session.execute(
                text("""
                SELECT i.*,b.eligibility,n.kind AS number_kind,n.route_id
                FROM house_numbering.proposal_items i
                JOIN house_numbering.buildings b
                  ON b.organization_id=i.organization_id AND b.project_id=i.project_id AND b.id=i.building_id
                LEFT JOIN house_numbering.address_numbers n
                  ON n.organization_id=i.organization_id AND n.project_id=i.project_id AND n.id=i.source_number_id
                WHERE i.organization_id=:org AND i.project_id=:project AND i.proposal_id=:id
                ORDER BY i.created_at,i.id
            """),
                {"org": self.org_id, "project": self.project_id, "id": proposal_id},
            )
            .mappings()
            .all()
        )
        return proposal, items

    def transition_proposal(
        self,
        proposal_id: UUID,
        *,
        expected_revision: int,
        action: str,
        comment: str | None,
        reviewer: bool,
    ) -> RowMapping:
        self.prepare("manage" if reviewer else "edit")
        proposal, items = self.proposal(proposal_id, for_update=True)
        if int(proposal["revision"]) != expected_revision:
            raise PlatformConflictError(
                "proposal revision is stale", code="STALE_PROPOSAL"
            )
        state = str(proposal["status"])
        now = datetime.now(UTC)
        if action == "submit" and state == "draft":
            if any(
                item["eligibility"] == "ineligible" or item["number_kind"] != "proposed"
                for item in items
            ):
                raise PlatformConflictError(
                    "proposal contains ineligible or stale items",
                    code="INVALID_PROPOSAL_ITEM",
                )
            route_ids = sorted(
                {
                    UUID(str(item["route_id"]))
                    for item in items
                    if item["route_id"] is not None
                },
                key=str,
            )
            if route_ids:
                self.session.execute(
                    text("""
                        SELECT id FROM house_numbering.routes
                        WHERE organization_id=:org AND project_id=:project
                          AND id = ANY(:routes)
                        ORDER BY id
                        FOR UPDATE
                    """),
                    {
                        "org": self.org_id,
                        "project": self.project_id,
                        "routes": route_ids,
                    },
                ).all()
            duplicate = self.session.execute(
                text("""
                    SELECT n.route_id,n.number_text
                    FROM house_numbering.proposal_items i
                    JOIN house_numbering.address_numbers n
                      ON n.organization_id=i.organization_id
                     AND n.project_id=i.project_id
                     AND n.id=i.source_number_id
                    WHERE i.organization_id=:org AND i.project_id=:project
                      AND i.proposal_id=:proposal
                    GROUP BY n.route_id,n.number_text
                    HAVING count(*) > 1
                    LIMIT 1
                """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "proposal": proposal_id,
                },
            ).first()
            occupied = self.session.execute(
                text("""
                    SELECT n.route_id,n.number_text
                    FROM house_numbering.proposal_items i
                    JOIN house_numbering.address_numbers n
                      ON n.organization_id=i.organization_id
                     AND n.project_id=i.project_id
                     AND n.id=i.source_number_id
                    JOIN house_numbering.address_numbers existing
                      ON existing.organization_id=n.organization_id
                     AND existing.project_id=n.project_id
                     AND existing.route_id=n.route_id
                     AND existing.number_text=n.number_text
                     AND existing.id<>n.id
                     AND existing.retired_at IS NULL
                     AND existing.kind IN ('submitted','approved','reserved')
                    WHERE i.organization_id=:org AND i.project_id=:project
                      AND i.proposal_id=:proposal
                    LIMIT 1
                """),
                {
                    "org": self.org_id,
                    "project": self.project_id,
                    "proposal": proposal_id,
                },
            ).first()
            if duplicate is not None or occupied is not None:
                conflict = duplicate or occupied
                raise PlatformConflictError(
                    f"number {conflict[1]!r} is already occupied on this route",
                    code="NUMBER_OCCUPIED",
                )
            self.session.execute(
                text("""
                    UPDATE house_numbering.address_numbers n SET kind='submitted',revision=revision+1,
                        updated_by_actor_id=:actor,updated_at=:now
                    FROM house_numbering.proposal_items i
                    WHERE i.organization_id=:org AND i.project_id=:project AND i.proposal_id=:proposal
                      AND n.organization_id=i.organization_id AND n.project_id=i.project_id
                      AND n.id=i.source_number_id AND n.kind='proposed'
                """),
                {
                    "actor": self.actor_id,
                    "now": now,
                    "org": self.org_id,
                    "project": self.project_id,
                    "proposal": proposal_id,
                },
            )
            new_state, submitted_at, decided_at = "submitted", now, None
        elif action == "withdraw" and state == "submitted" and not reviewer:
            if not comment or not comment.strip():
                raise PlatformValidationError("withdrawal requires a reason")
            self.session.execute(
                text("""
                    UPDATE house_numbering.address_numbers n
                    SET kind='proposed',revision=revision+1,
                        updated_by_actor_id=:actor,updated_at=:now
                    FROM house_numbering.proposal_items i
                    WHERE i.organization_id=:org AND i.project_id=:project
                      AND i.proposal_id=:proposal
                      AND n.organization_id=i.organization_id
                      AND n.project_id=i.project_id
                      AND n.id=i.source_number_id
                      AND n.kind='submitted'
                """),
                {
                    "actor": self.actor_id,
                    "now": now,
                    "org": self.org_id,
                    "project": self.project_id,
                    "proposal": proposal_id,
                },
            )
            new_state, submitted_at, decided_at = "draft", None, now
        elif action == "reject" and state == "submitted" and reviewer:
            if not comment or not comment.strip():
                raise PlatformValidationError("rejection requires a reason")
            self.session.execute(
                text("""
                    UPDATE house_numbering.address_numbers n SET kind='proposed',revision=revision+1,
                        updated_by_actor_id=:actor,updated_at=:now
                    FROM house_numbering.proposal_items i
                    WHERE i.organization_id=:org AND i.project_id=:project AND i.proposal_id=:proposal
                      AND n.organization_id=i.organization_id AND n.project_id=i.project_id
                      AND n.id=i.source_number_id AND n.kind='submitted'
                """),
                {
                    "actor": self.actor_id,
                    "now": now,
                    "org": self.org_id,
                    "project": self.project_id,
                    "proposal": proposal_id,
                },
            )
            new_state, submitted_at, decided_at = "draft", None, now
        elif action == "approve" and state == "submitted" and reviewer:
            if (
                proposal["rule_status"] != "authoritative"
                or not proposal["authority_ref"]
            ):
                raise PlatformConflictError(
                    "approval requires authoritative rule",
                    code="RULE_NOT_AUTHORITATIVE",
                )
            if any(
                item["eligibility"] == "ineligible"
                or item["number_kind"] != "submitted"
                for item in items
            ):
                raise PlatformConflictError(
                    "proposal contains ineligible or stale items",
                    code="INVALID_PROPOSAL_ITEM",
                )
            self.session.execute(
                text("""
                    UPDATE house_numbering.address_numbers n SET kind='approved',revision=revision+1,
                        updated_by_actor_id=:actor,updated_at=:now
                    FROM house_numbering.proposal_items i
                    WHERE i.organization_id=:org AND i.project_id=:project AND i.proposal_id=:proposal
                      AND n.organization_id=i.organization_id AND n.project_id=i.project_id
                      AND n.id=i.source_number_id AND n.kind='submitted'
                """),
                {
                    "actor": self.actor_id,
                    "now": now,
                    "org": self.org_id,
                    "project": self.project_id,
                    "proposal": proposal_id,
                },
            )
            new_state, submitted_at, decided_at = (
                "approved",
                proposal["submitted_at"],
                now,
            )
        else:
            raise PlatformConflictError(
                "invalid proposal transition", code="INVALID_PROPOSAL_TRANSITION"
            )
        row = (
            self.session.execute(
                text("""
                UPDATE house_numbering.proposals
                SET status=:status,decision_comment=:comment,revision=revision+1,
                    updated_by_actor_id=:actor,submitted_at=:submitted,decided_at=:decided,updated_at=:now
                WHERE organization_id=:org AND project_id=:project AND id=:id
                RETURNING *
            """),
                {
                    "status": new_state,
                    "comment": comment,
                    "actor": self.actor_id,
                    "submitted": submitted_at,
                    "decided": decided_at,
                    "now": now,
                    "org": self.org_id,
                    "project": self.project_id,
                    "id": proposal_id,
                },
            )
            .mappings()
            .one()
        )
        self.audit(
            f"house_numbering.proposal.{action}",
            "house_numbering_proposal",
            proposal_id,
            {"state": new_state},
        )
        return row

    def enable_private_access(self) -> None:
        self.prepare("manage")
        set_extension_private_access(
            self.session, extension_key="house_numbering", enabled=True
        )
