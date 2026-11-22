"""Application services for House Numbering US1-US3."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from typing import Any
from uuid import UUID

from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.platform_errors import (
    PlatformValidationError,
)
from sqlalchemy.engine import RowMapping
from sqlalchemy.orm import Session

from modules.house_numbering.domain import (
    Address,
    Rule,
    locate_on_route,
    suggest_number,
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
        if not building_ids:
            raise PlatformValidationError("numbering preview requires buildings")
        rule = self.repo.get_rule(rule_id)
        changes: list[dict[str, Any]] = []
        warnings: list[str] = []
        base: dict[str, Any] = {}
        route_revisions: dict[UUID, int] = {}

        for building_id in building_ids:
            building = self.repo.get_building(building_id)
            if building["route_id"] is None:
                warnings.append(f"building {building_id} has no route")
                continue
            if building["eligibility"] == "ineligible":
                warnings.append(f"building {building_id} is ineligible")
                continue
            route_id = UUID(str(building["route_id"]))
            route = self.repo.route_geometry(route_id)
            target_crs = int(route["srid"])
            rows = self.repo.building_points_for_route(route_id, target_crs)
            target = next((row for row in rows if row["id"] == building_id), None)
            if target is None:
                warnings.append(f"building {building_id} has no usable geometry")
                continue
            inspection = self.inspect(
                x=float(target["x"]),
                y=float(target["y"]),
                crs_code=target_crs,
                route_id=route_id,
                rule_id=rule_id,
                existing_building_id=building_id,
            )
            number = inspection["suggestion"]["number"]
            reason = str(inspection["suggestion"]["reason"])
            if number is None:
                warnings.append(f"building {building_id}: {reason}")
                continue
            existing = self.repo.proposed_number_for_building(building_id)
            if existing is not None and existing["kind"] in {"submitted", "approved"}:
                warnings.append(
                    f"building {building_id} has protected {existing['kind']} number"
                )
                continue
            route_row = self.repo.get_route(route_id)
            route_revisions[route_id] = int(route_row["revision"])
            base[f"route_geometry:{route_id}"] = route["content_revision"]
            base[f"building:{building_id}"] = int(building["revision"])
            changes.append(
                {
                    "building_id": str(building_id),
                    "route_id": str(route_id),
                    "old_number": existing["number_text"] if existing else None,
                    "number": str(number),
                    "side": inspection["location"]["side"],
                    "measure": inspection["location"]["measure_m"],
                    "reason": reason,
                }
            )
        for route_id, revision in route_revisions.items():
            base[f"route:{route_id}"] = revision
        preview = self.repo.create_preview(
            operation_id=operation_id,
            rule_id=UUID(str(rule["id"])),
            base_revisions=base,
            changes=changes,
            warnings=warnings,
            expires_at=datetime.now(UTC) + timedelta(minutes=15),
        )
        self.commit()
        return _wire(preview)

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
