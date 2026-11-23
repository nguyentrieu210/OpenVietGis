"""Application services for House Numbering US1-US3."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from typing import Any
from uuid import UUID

from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformNotFoundError,
    PlatformValidationError,
)
from sqlalchemy import text
from sqlalchemy.engine import RowMapping
from sqlalchemy.orm import Session

from modules.house_numbering.batch_numbering import build_route_plan
from modules.house_numbering.domain import (
    Address,
    DomainError,
    Rule,
    locate_on_route,
    suggest_number,
)
from modules.house_numbering.numbering_audit import (
    audit_route_numbering,
    find_tree_collisions,
    summarize_audit,
)
from modules.house_numbering.persistence import HouseNumberingRepository


def _wire(row: RowMapping | dict[str, Any]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for key, value in dict(row).items():
        if isinstance(value, UUID):
            out[key] = str(value)
        elif isinstance(value, datetime):
            out[key] = value.isoformat()
        else:
            out[key] = value
    return out


def _domain_rule(row: RowMapping) -> Rule:
    return Rule(
        version=str(row["version"]),
        left_odd=bool(row["left_odd"]),
        suffix_style=str(row["suffix_style"]),
        max_distance=float(row["max_distance_m"]),
        authority=str(row["authority_ref"]) if row["authority_ref"] else None,
    )


def _route_vertices(route_geometry: dict[str, Any]) -> list[tuple[float, float]]:
    geometry = dict(route_geometry["geometry"])
    if geometry.get("type") != "LineString":
        raise PlatformValidationError(
            "route must resolve to LineString geometry", code="ROUTE_GEOMETRY_INVALID"
        )
    coordinates = geometry.get("coordinates")
    if not isinstance(coordinates, list) or len(coordinates) < 2:
        raise PlatformValidationError(
            "route has insufficient vertices", code="ROUTE_GEOMETRY_INVALID"
        )
    vertices: list[tuple[float, float]] = []
    for coordinate in coordinates:
        if (
            not isinstance(coordinate, list)
            or len(coordinate) < 2
            or isinstance(coordinate[0], bool)
            or isinstance(coordinate[1], bool)
            or not isinstance(coordinate[0], (int, float))
            or not isinstance(coordinate[1], (int, float))
        ):
            raise PlatformValidationError(
                "route contains invalid coordinates", code="ROUTE_GEOMETRY_INVALID"
            )
        vertices.append((float(coordinate[0]), float(coordinate[1])))
    return vertices


class HouseNumberingService:
    def __init__(self, session: Session, context: AuthorizationContext) -> None:
        self.session = session
        self.repo = HouseNumberingRepository(session, context)

    def commit(self) -> None:
        self.session.commit()

    def create_rule(self, **kwargs: Any) -> dict[str, Any]:
        row = self.repo.create_rule(**kwargs)
        self.commit()
        return _wire(row)

    def list_rules(self) -> list[dict[str, Any]]:
        return [_wire(row) for row in self.repo.list_rules()]

    def create_area(self, **kwargs: Any) -> dict[str, Any]:
        row = self.repo.create_area(**kwargs)
        self.commit()
        return _wire(row)

    def list_areas(self) -> list[dict[str, Any]]:
        return [_wire(row) for row in self.repo.list_areas()]

    def update_area(self, area_id: UUID, **kwargs: Any) -> dict[str, Any]:
        row = self.repo.update_area(area_id, **kwargs)
        self.commit()
        return _wire(row)

    def delete_area(self, area_id: UUID, *, expected_revision: int) -> None:
        self.repo.delete_area(area_id, expected_revision=expected_revision)
        self.commit()

    def create_route(self, **kwargs: Any) -> dict[str, Any]:
        row = self.repo.create_route(**kwargs)
        self.commit()
        return _wire(row)

    def list_routes(self) -> list[dict[str, Any]]:
        return [_wire(row) for row in self.repo.list_routes()]

    def update_route(self, route_id: UUID, **kwargs: Any) -> dict[str, Any]:
        row = self.repo.update_route(route_id, **kwargs)
        self.commit()
        return _wire(row)

    def delete_route(self, route_id: UUID) -> None:
        self.repo.delete_route(route_id)
        self.commit()

    def create_building(self, **kwargs: Any) -> dict[str, Any]:
        row = self.repo.create_building(**kwargs)
        self.commit()
        return _wire(row)

    def list_buildings(self, route_id: UUID | None = None) -> list[dict[str, Any]]:
        return [_wire(row) for row in self.repo.list_buildings(route_id=route_id)]

    def update_building(self, building_id: UUID, **kwargs: Any) -> dict[str, Any]:
        row = self.repo.update_building(building_id, **kwargs)
        self.commit()
        return _wire(row)

    def retire_building(self, building_id: UUID, *, expected_revision: int) -> None:
        self.repo.retire_building(building_id, expected_revision=expected_revision)
        self.commit()

    def set_building_eligibility(
        self, building_id: UUID, **kwargs: Any
    ) -> dict[str, Any]:
        row = self.repo.set_building_eligibility(building_id, **kwargs)
        self.commit()
        return _wire(row)

    def set_building_plaque(
        self,
        building_id: UUID,
        *,
        expected_revision: int,
        status: str,
    ) -> dict[str, Any]:
        row = self.repo.set_plaque_status(
            building_id, expected_revision=expected_revision, status=status
        )
        self.commit()
        return _wire(row)

    def record_building_lineage(
        self,
        *,
        relation_kind: str,
        operation_id: UUID,
        predecessor_building_ids: list[UUID],
        successor_building_ids: list[UUID],
        expected_revisions: dict[str, int],
    ) -> dict[str, Any]:
        try:
            revisions = {
                UUID(key): int(value) for key, value in expected_revisions.items()
            }
        except (ValueError, TypeError) as exc:
            raise PlatformValidationError(
                "invalid lineage expected_revisions", code="LINEAGE_REVISION_SET"
            ) from exc
        rows = self.repo.record_building_lineage(
            relation_kind=relation_kind,
            operation_id=operation_id,
            predecessor_ids=predecessor_building_ids,
            successor_ids=successor_building_ids,
            expected_revisions=revisions,
        )
        self.commit()
        return {
            "operation_id": str(operation_id),
            "relation_kind": relation_kind,
            "items": [_wire(row) for row in rows],
        }

    def list_building_lineage(self, building_id: UUID) -> list[dict[str, Any]]:
        return [_wire(row) for row in self.repo.list_building_lineage(building_id)]

    def inspect(
        self,
        *,
        x: float,
        y: float,
        crs_code: int,
        route_id: UUID,
        rule_id: UUID,
        existing_building_id: UUID | None = None,
    ) -> dict[str, Any]:
        route = self.repo.route_geometry(route_id)
        rule_row = self.repo.get_rule(rule_id)
        rule = _domain_rule(rule_row)
        context = self.repo.inspection_context(existing_building_id, route_id)
        context_wire = {
            "building": _wire(context["building"]) if context["building"] else None,
            "evidence": [_wire(row) for row in context["evidence"]],
        }
        target_crs = int(route["srid"])
        point = (
            (x, y)
            if crs_code == target_crs
            else self.repo.transform_point(x, y, crs_code, target_crs)
        )
        vertices = _route_vertices(route)
        location = locate_on_route(point, vertices, crs=f"EPSG:{target_crs}")
        if location.side is None:
            return {
                "route_id": str(route_id),
                "location": {
                    "measure_m": location.measure,
                    "distance_m": location.distance,
                    "side": None,
                    "crs": location.crs,
                },
                "suggestion": {
                    "number": None,
                    "reason": "ambiguous_route_side",
                    "rule_version": rule.version,
                },
                "neighbors": {"before": None, "after": None},
                "context": context_wire,
            }

        rows = self.repo.building_points_for_route(route_id, target_crs)
        addresses: list[Address] = []
        located: list[tuple[dict[str, Any], float, str]] = []
        existing_address_id: str | None = None
        for row in rows:
            candidate_location = locate_on_route(
                (float(row["x"]), float(row["y"])),
                vertices,
                crs=f"EPSG:{target_crs}",
            )
            if candidate_location.side is None:
                continue
            located.append((row, candidate_location.measure, candidate_location.side))
            if row.get("number_text"):
                state = str(row.get("kind") or "proposed")
                if state not in {
                    "old",
                    "certificate",
                    "cad",
                    "survey",
                    "proposed",
                    "submitted",
                    "approved",
                    "reserved",
                }:
                    state = "proposed"
                address_id = str(row["number_id"] or row["id"])
                addresses.append(
                    Address(
                        address_id,
                        str(row["number_text"]),
                        candidate_location.measure,
                        candidate_location.side,
                        state,
                    )
                )
                if (
                    existing_building_id is not None
                    and row["id"] == existing_building_id
                ):
                    existing_address_id = address_id

        # Route-level reserved slots are anchors in occupancy but may have no building geometry.
        for number in self.repo.list_route_numbers(route_id):
            if number["kind"] != "reserved" or number["building_id"] is not None:
                continue
            if number["side"] is None or number["measure_m"] is None:
                continue
            addresses.append(
                Address(
                    str(number["id"]),
                    str(number["number_text"]),
                    float(number["measure_m"]),
                    str(number["side"]),
                    "reserved",
                )
            )

        suggestion = suggest_number(
            addresses,
            measure=location.measure,
            side=location.side,
            rule=rule,
            prefix=str(route.get("prefix") or ""),
            distance=location.distance,
            existing_id=existing_address_id,
        )
        same_side = sorted(
            (
                (row, measure)
                for row, measure, side in located
                if side == location.side and row.get("number_text")
            ),
            key=lambda item: item[1],
        )
        before = next(
            (item for item in reversed(same_side) if item[1] < location.measure),
            None,
        )
        after = next(
            (item for item in same_side if item[1] > location.measure),
            None,
        )

        def neighbor(
            item: tuple[dict[str, Any], float] | None,
        ) -> dict[str, Any] | None:
            if item is None:
                return None
            row, measure = item
            return {
                "building_id": str(row["id"]),
                "number": row.get("number_text"),
                "measure_m": measure,
            }

        return {
            "route_id": str(route_id),
            "location": {
                "measure_m": location.measure,
                "distance_m": location.distance,
                "side": location.side,
                "crs": location.crs,
            },
            "suggestion": {
                "number": suggestion.number,
                "reason": suggestion.reason,
                "rule_version": suggestion.rule_version,
            },
            "neighbors": {"before": neighbor(before), "after": neighbor(after)},
            "context": context_wire,
        }

    def preview_assignment(
        self,
        *,
        building_ids: list[UUID],
        rule_id: UUID,
        operation_id: UUID,
    ) -> dict[str, Any]:
        if not building_ids or len(set(building_ids)) != len(building_ids):
            raise PlatformValidationError("numbering requires unique building IDs")
        rule = self.repo.get_rule(rule_id)
        domain_rule = _domain_rule(rule)
        changes: list[dict[str, Any]] = []
        warnings: list[str] = []
        audits: list[dict[str, Any]] = []
        inspected_numbers: set[str] = set()
        base: dict[str, Any] = {}
        groups: dict[UUID, dict[str, Any]] = {}
        targets = self.repo.numbering_buildings(building_ids)
        if len(targets) != len(building_ids):
            raise PlatformNotFoundError("building not found")
        for b in targets:
            if b["route_id"] is None:
                warnings.append(f"building {b['id']}: no_route")
                continue
            groups.setdefault(b["route_id"], {})[str(b["id"])] = b
        for route_id, group in sorted(groups.items(), key=lambda item: str(item[0])):
            snapshot = self.repo.numbering_route_snapshot(route_id)
            try:
                route = self.repo.route_geometry(route_id)
                vertices = _route_vertices(route)
            except (PlatformNotFoundError, PlatformValidationError):
                warnings.extend(
                    f"building {bid}: invalid_route_geometry" for bid in group
                )
                continue
            points = self.repo.building_points_for_route(route_id, int(route["srid"]))
            numbers = [dict(row) for row in self.repo.list_route_numbers(route_id)]
            try:
                proposed, issues = build_route_plan(
                    route=route,
                    vertices=vertices,
                    rule=domain_rule,
                    points=points,
                    numbers=numbers,
                    targets=group,
                )
            except DomainError:
                warnings.extend(
                    f"building {bid}: invalid_route_or_number_data" for bid in group
                )
                continue
            if snapshot != self.repo.numbering_route_snapshot(route_id):
                raise PlatformConflictError(
                    "route numbering changed during preview", code="STALE_PREVIEW"
                )
            current = self.repo.route_geometry(route_id)
            if (
                current["content_revision"] != route["content_revision"]
                or current["revision"] != route["revision"]
            ):
                raise PlatformConflictError(
                    "route changed during preview", code="STALE_PREVIEW"
                )
            audit = audit_route_numbering(
                route=route,
                vertices=vertices,
                rule=domain_rule,
                points=points,
                numbers=numbers,
                changes=proposed,
            )
            audit["route_name"] = self.repo.get_route(route_id)["name"]
            audit["cross_route_duplicates"] = []
            audits.append(audit)
            inspected_numbers.update(str(n["number_text"]) for n in numbers)
            inspected_numbers.update(str(c["number"]) for c in proposed)
            warnings.extend(issues)
            if audit["blocking_issues"]:
                warnings.append(
                    f"route {route_id}: numbering_audit_blocked ({audit['blocking_issues']} issues)"
                )
                # A corrupt existing anchor or duplicate prevents safely placing
                # new numbers on that route. Other independent child routes remain usable.
                proposed = []
            if proposed:
                base[f"route:{route_id}"] = int(route["revision"])
                base[f"route_geometry:{route_id}"] = route["content_revision"]
                base[f"route_snapshot:{route_id}"] = snapshot
                for change in proposed:
                    bid = change["building_id"]
                    base[f"building:{bid}"] = int(group[bid]["revision"])
                changes.extend(proposed)
        tree_collisions: list[dict[str, Any]] = []
        if inspected_numbers:
            scope = {"org": self.repo.org_id, "project": self.repo.project_id}
            parents = {
                str(r["id"]): str(r["parent_route_id"])
                if r["parent_route_id"]
                else None
                for r in self.repo.session.execute(
                    text("""
                    SELECT id,parent_route_id FROM house_numbering.routes
                    WHERE organization_id=:org AND project_id=:project
                """),
                    scope,
                ).mappings()
            }
            active = (
                self.repo.session.execute(
                    text("""
                SELECT route_id,number_text,building_id FROM house_numbering.address_numbers
                WHERE organization_id=:org AND project_id=:project AND retired_at IS NULL
                  AND number_text=ANY(:numbers)
            """),
                    {**scope, "numbers": sorted(inspected_numbers)},
                )
                .mappings()
                .all()
            )
            occupied = [
                {
                    "route_id": r["route_id"],
                    "number_text": r["number_text"],
                    "building_id": r["building_id"],
                    "origin": "existing",
                }
                for r in active
            ]
            occupied.extend(
                {
                    "route_id": c["route_id"],
                    "number_text": c["number"],
                    "building_id": c["building_id"],
                    "origin": "proposal",
                }
                for c in changes
            )
            tree_collisions = find_tree_collisions(parents, occupied)
            audited_ids = {str(a["route_id"]) for a in audits}
            affected = {
                entry["route_id"]
                for issue in tree_collisions
                for entry in issue["occupants"]
                if entry["route_id"] in audited_ids
            }
            for detail in audits:
                relevant = [
                    issue
                    for issue in tree_collisions
                    if any(
                        entry["route_id"] == detail["route_id"]
                        for entry in issue["occupants"]
                    )
                ]
                if relevant:
                    detail["cross_route_duplicates"] = relevant[:40]
                    detail["blocking_issues"] += len(relevant)
            if affected:
                filtered = [c for c in changes if str(c["route_id"]) in affected]
                changes = [c for c in changes if str(c["route_id"]) not in affected]
                for bid in (str(c["building_id"]) for c in filtered):
                    base.pop(f"building:{bid}", None)
                for rid in affected:
                    for label in ("route", "route_geometry", "route_snapshot"):
                        base.pop(f"{label}:{rid}", None)
                warnings.extend(
                    f"route {rid}: cross_route_number_duplicate"
                    for rid in sorted(affected)
                )
        preview = self.repo.create_preview(
            operation_id=operation_id,
            rule_id=UUID(str(rule["id"])),
            base_revisions=base,
            changes=changes,
            warnings=warnings,
            expires_at=datetime.now(UTC) + timedelta(minutes=15),
        )
        self.commit()
        result = _wire(preview)
        result["audit"] = summarize_audit(audits)
        result["audit"]["cross_route_duplicate_count"] = len(tree_collisions)
        result["audit"]["cross_route_duplicates"] = tree_collisions[:40]
        return result

    def apply_preview(self, preview_id: UUID) -> list[dict[str, Any]]:
        rows = self.repo.apply_preview(preview_id)
        self.commit()
        return [_wire(row) for row in rows]

    def create_proposal(
        self,
        *,
        scope_kind: str,
        scope_id: UUID,
        rule_id: UUID,
        building_ids: list[UUID],
    ) -> dict[str, Any]:
        if scope_kind == "route":
            self.repo.get_route(scope_id)
        elif scope_kind == "area":
            if not any(
                UUID(str(row["id"])) == scope_id for row in self.repo.list_areas()
            ):
                raise PlatformValidationError("proposal area does not exist")
        else:
            raise PlatformValidationError("proposal scope must be route or area")
        proposal, items = self.repo.create_proposal(
            scope_kind=scope_kind,
            scope_id=scope_id,
            rule_id=rule_id,
            building_ids=building_ids,
        )
        self.commit()
        return {"proposal": _wire(proposal), "items": [_wire(row) for row in items]}

    def get_proposal(self, proposal_id: UUID) -> dict[str, Any]:
        proposal, items = self.repo.proposal(proposal_id)
        return {"proposal": _wire(proposal), "items": [_wire(row) for row in items]}

    def transition_proposal(
        self,
        proposal_id: UUID,
        *,
        expected_revision: int,
        action: str,
        comment: str | None,
        reviewer: bool,
    ) -> dict[str, Any]:
        row = self.repo.transition_proposal(
            proposal_id,
            expected_revision=expected_revision,
            action=action,
            comment=comment,
            reviewer=reviewer,
        )
        self.commit()
        return _wire(row)
